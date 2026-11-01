//! Estado atual do ASUS Aura Sync a partir dos arquivos do LightingService.
//!
//! - LastProfile.xml: `mode key="100"` = efeito definido por script (Aura Sync); outros valores
//!   (ex.: 1) = modo fixo do hardware, com a cor do LED da placa-mãe (preto = "Escuro").
//! - script/LastScript.xml: o efeito em si, como um script de ondas. O grupo da placa-mãe
//!   (viewport/group com device/type Mainboard_Master) aponta para o `effect` do `effectList`;
//!   o prefixo da chave diz o tipo (StaticSingleEff, BreathSingleEff, ColorCycleSingleEff,
//!   RainbowSingleEff, StrobingSingleEff, StarSingleEff, SmartSingleEff...) e as ondas trazem
//!   frequência e canal (ALPHA = brilho, HUE = matiz). No Smart, os limiares e as três cores.
//!
//! Os dois arquivos são reescritos pelo LightingService a cada mudança no Armoury Crate; só relemos
//! quando o mtime muda.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use roxmltree::{Document, Node};

use crate::{info, warn};

const T: &str = "gmk104.aura";
const MODE_SCRIPT: &str = "100";

pub fn ls_dir() -> PathBuf {
    std::env::var_os("GMK104_LS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\LightingService"))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsl {
    pub h: f64, // 0..1
    pub s: f64, // 0..1
    pub l: f64, // 0..1
}

impl Hsl {
    pub const RED: Hsl = Hsl { h: 0.0, s: 1.0, l: 0.5 };

    pub fn is_valid(self) -> bool {
        (0.0..=1.0).contains(&self.h) && (0.0..=1.0).contains(&self.s) && (0.0..=1.0).contains(&self.l)
    }

    pub fn is_black(self) -> bool {
        self.l <= 0.0
    }

    /// HSL -> (hue, sat) em 0..255 para o teclado. O brilho (V) fica a cargo do ajuste do teclado.
    pub fn to_hs255(self) -> (u8, u8) {
        let v = self.l + self.s * self.l.min(1.0 - self.l);
        let sv = if v == 0.0 { 0.0 } else { 2.0 * (1.0 - self.l / v) };
        let h = ((self.h * 255.0).round() as i64).rem_euclid(256) as u8;
        let s = (sv * 255.0).round().clamp(0.0, 255.0) as u8;
        (h, s)
    }

    fn from_color_int(v: u32) -> Hsl {
        // formato 0x00BBGGRR
        let (r, g, b) = ((v & 0xFF) as f64 / 255.0, ((v >> 8) & 0xFF) as f64 / 255.0, ((v >> 16) & 0xFF) as f64 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if max == min {
            return Hsl { h: 0.0, s: 0.0, l };
        }
        let d = max - min;
        let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        let mut h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        h /= 6.0;
        Hsl { h, s, l }
    }
}

/// Efeito ativo na placa-mãe, já reduzido ao que interessa para o teclado.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Luzes apagadas ("Escuro").
    Off,
    /// Cor fixa.
    Solid(Hsl),
    /// Intermitente: brilho em senoide. `freq` do script do Aura.
    Breathing { color: Hsl, freq: f64 },
    /// Ciclo de cor: matiz girando a partir de `start`; `freq` negativa = sentido inverso.
    ColorCycle { start: Hsl, freq: f64 },
    /// Arco-íris correndo pelos LEDs.
    Rainbow { freq: f64 },
    /// Estroboscópio: brilho em dente de serra.
    Strobing { color: Hsl, freq: f64 },
    /// Noite estrelada: pontos acendendo e apagando com a cor.
    Starry { color: Hsl, freq: f64 },
    /// Smart (térmico): a cor vem do log do LightingService.
    Smart,
    /// Música, Imersão, Cor adaptável, efeitos baixados...: sem como reproduzir.
    Unsupported { name: String, color: Option<Hsl> },
}

impl Effect {
    pub fn name(&self) -> &'static str {
        match self {
            Effect::Off => "Escuro",
            Effect::Solid(_) => "Estático",
            Effect::Breathing { .. } => "Intermitente",
            Effect::ColorCycle { .. } => "Ciclo de cor",
            Effect::Rainbow { .. } => "Arco-íris",
            Effect::Strobing { .. } => "Estroboscópio",
            Effect::Starry { .. } => "Noite estrelada",
            Effect::Smart => "Smart",
            Effect::Unsupported { .. } => "não suportado",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuraState {
    pub mode: String,
    pub effect: Effect,
    pub thr_low: f64,
    pub thr_high: f64,
    pub smart_colors: Vec<(String, Hsl)>, // trigger -> cor
    /// Cor do LED da placa-mãe no perfil (cor base).
    pub static_color: Hsl,
}

impl Default for AuraState {
    fn default() -> Self {
        Self {
            mode: String::new(),
            effect: Effect::Solid(Hsl::RED),
            thr_low: 65.0,
            thr_high: 85.0,
            smart_colors: Vec::new(),
            static_color: Hsl::RED,
        }
    }
}

impl AuraState {
    pub fn smart_color(&self, trigger: &str) -> Option<Hsl> {
        self.smart_colors.iter().find(|(k, _)| k == trigger).map(|(_, c)| *c)
    }
}

fn child_text<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.children().find(|c| c.has_tag_name(name)).and_then(|c| c.text()).map(str::trim)
}

fn desc_text<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.descendants().find(|c| c.has_tag_name(name)).and_then(|c| c.text()).map(str::trim)
}

fn parse_f(s: Option<&str>) -> Option<f64> {
    s.and_then(|v| v.replace(',', ".").parse().ok())
}

fn init_color(e: Node) -> Option<Hsl> {
    let ic = e.children().find(|c| c.has_tag_name("initColor"))?;
    let c = Hsl {
        h: parse_f(child_text(ic, "hue"))?,
        s: parse_f(child_text(ic, "saturation"))?,
        l: parse_f(child_text(ic, "lightness"))?,
    };
    c.is_valid().then_some(c)
}

fn first_wave_freq(e: Node) -> f64 {
    e.descendants()
        .find(|c| c.has_tag_name("wave"))
        .and_then(|w| parse_f(child_text(w, "freq")))
        .unwrap_or(0.0)
}

pub fn parse_profile(path: &Path) -> Result<AuraState, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc = Document::parse(&text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let mut st = AuraState::default();

    let scenes: Vec<Node> = root.descendants().filter(|n| n.has_tag_name("scene")).collect();
    let scene = scenes
        .iter()
        .find(|s| s.attribute("key") == Some("S0"))
        .or(scenes.first())
        .copied()
        .ok_or("perfil sem <scene>")?;
    let mode = scene.descendants().find(|n| n.has_tag_name("mode")).ok_or("perfil sem <mode>")?;
    st.mode = mode.attribute("key").unwrap_or("").to_string();

    if let Some(led) = mode
        .descendants()
        .find(|n| n.has_tag_name("led") && child_text(*n, "DeviceName") == Some("Mainboard_Master"))
    {
        let hsl = (parse_f(child_text(led, "hue")), parse_f(child_text(led, "saturation")), parse_f(child_text(led, "lightness")));
        st.static_color = match hsl {
            (Some(h), Some(s), Some(l)) if s > 0.0 || l > 0.0 => Hsl { h, s, l },
            _ => Hsl::from_color_int(child_text(led, "color").and_then(|v| v.parse().ok()).unwrap_or(0)),
        };
    }
    if let (Some(t1), Some(t2)) = (parse_f(desc_text(root, "thermal_threshold_one")), parse_f(desc_text(root, "thermal_threshold_two"))) {
        st.thr_low = t1;
        st.thr_high = t2;
    }
    // Fora do modo por script, o que vale é a cor do LED no perfil (preto = apagado).
    st.effect = if st.mode == MODE_SCRIPT {
        Effect::Solid(st.static_color) // provisório: o script decide
    } else if st.static_color.is_black() {
        Effect::Off
    } else {
        Effect::Solid(st.static_color)
    };
    Ok(st)
}

/// Complementa com o script: tipo de efeito, parâmetros e, no Smart, limiares e cores por faixa.
pub fn parse_script(mut st: AuraState, path: &Path) -> Result<AuraState, String> {
    let Ok(text) = fs::read_to_string(path) else { return Ok(st) };
    let doc = Document::parse(&text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let all_effects: Vec<Node> = root.descendants().filter(|n| n.has_tag_name("effect")).collect();

    // Smart: limiares efetivos e cores das faixas
    let smart_effects: Vec<Node> = all_effects
        .iter()
        .copied()
        .filter(|e| e.attribute("key").unwrap_or("").starts_with("SmartSingleEff"))
        .collect();
    let mut has_smart = false;
    for eff in all_effects.iter().filter(|e| e.attribute("key") == Some("Smartmode")) {
        has_smart = true;
        if let (Some(lo), Some(hi)) = (parse_f(child_text(*eff, "Threshold_low")), parse_f(child_text(*eff, "Threshold_high"))) {
            st.thr_low = lo;
            st.thr_high = hi;
        }
        let Some(trig) = child_text(*eff, "trigger") else { continue };
        if st.smart_color(trig).is_some() {
            continue;
        }
        let Some(sub_key) = eff.children().find(|c| c.has_tag_name("smartmode_effect")).and_then(|c| c.attribute("key")) else { continue };
        let Some(e) = smart_effects.iter().find(|e| e.attribute("key") == Some(sub_key)) else { continue };
        if let Some(h) = parse_f(desc_text(*e, "hue")) {
            let s = parse_f(desc_text(*e, "saturation")).unwrap_or(1.0);
            let l = parse_f(desc_text(*e, "lightness")).unwrap_or(0.5);
            st.smart_colors.push((trig.to_string(), Hsl { h, s, l }));
        }
    }
    if st.mode != MODE_SCRIPT {
        return Ok(st); // modo fixo do hardware: o perfil já decidiu (Off/Solid)
    }
    if has_smart {
        st.effect = Effect::Smart;
        return Ok(st);
    }

    // Grupo da placa-mãe no viewport -> chave do efeito no effectList
    let mb_keys: Vec<&str> = root
        .descendants()
        .filter(|n| n.has_tag_name("group"))
        .filter(|g| g.descendants().any(|d| d.has_tag_name("type") && d.text().map(str::trim) == Some("Mainboard_Master")))
        .filter_map(|g| g.attribute("key"))
        .collect();
    let list_effects: Vec<Node> = root
        .descendants()
        .find(|n| n.has_tag_name("effectList"))
        .map(|l| l.descendants().filter(|n| n.has_tag_name("effect")).collect())
        .unwrap_or_default();
    let pick = |pred: &dyn Fn(&str) -> bool| -> Option<Node> {
        list_effects.iter().copied().find(|e| {
            let k = e.attribute("key").unwrap_or("");
            mb_keys.contains(&k) && pred(k)
        })
    };
    // Noite estrelada tem dois grupos na placa (fundo + estrelas): prefere as estrelas.
    let node = pick(&|k| k.starts_with("Star"))
        .or_else(|| pick(&|_| true))
        .or_else(|| list_effects.iter().copied().find(|e| e.attribute("key").unwrap_or("").ends_with('0')));
    let Some(node) = node else {
        st.effect = Effect::Unsupported { name: "script sem efeito da placa".into(), color: None };
        return Ok(st);
    };
    let key = node.attribute("key").unwrap_or("");
    let prefix: String = key.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
    let color = init_color(node);
    let freq = first_wave_freq(node);
    st.effect = match prefix.as_str() {
        "StaticSingleEff" => match color {
            Some(c) if c.is_black() => Effect::Off,
            Some(c) => Effect::Solid(c),
            None => Effect::Solid(st.static_color),
        },
        "BreathSingleEff" => Effect::Breathing { color: color.unwrap_or(st.static_color), freq },
        "ColorCycleSingleEff" => Effect::ColorCycle { start: color.unwrap_or(st.static_color), freq },
        "RainbowSingleEff" => Effect::Rainbow { freq },
        "StrobingSingleEff" => Effect::Strobing { color: color.unwrap_or(st.static_color), freq },
        "StarSingleEff" => Effect::Starry { color: color.unwrap_or(st.static_color), freq },
        _ => Effect::Unsupported { name: prefix, color },
    };
    Ok(st)
}

/// Relê os arquivos quando mudam; mantém o último estado válido.
pub struct AuraWatcher {
    profile: PathBuf,
    script: PathBuf,
    mtimes: (Option<SystemTime>, Option<SystemTime>),
    pub state: AuraState,
    /// Instante em que o Aura gravou o estado atual (mtime mais recente dos dois arquivos): é quando a
    /// placa começou o efeito, e serve de origem de fase para as animações feitas pelo programa.
    pub changed_at: Option<SystemTime>,
}

impl AuraWatcher {
    pub fn new() -> Self {
        let dir = ls_dir();
        Self {
            profile: dir.join("LastProfile.xml"),
            script: dir.join("script").join("LastScript.xml"),
            mtimes: (None, None),
            state: AuraState::default(),
            changed_at: None,
        }
    }

    fn mtime(p: &Path) -> Option<SystemTime> {
        fs::metadata(p).and_then(|m| m.modified()).ok()
    }

    pub fn poll(&mut self) -> bool {
        let m = (Self::mtime(&self.profile), Self::mtime(&self.script));
        if m == self.mtimes {
            return false;
        }
        if m.0.is_none() {
            if self.mtimes.0.is_some() || self.state.mode.is_empty() {
                warn!(T, "perfil do Aura não encontrado em {}", self.profile.display());
            }
            self.mtimes = m;
            return false;
        }
        match parse_profile(&self.profile).and_then(|s| parse_script(s, &self.script)) {
            Ok(st) => {
                self.mtimes = m;
                self.changed_at = match m {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                let extra = match &st.effect {
                    Effect::Smart => {
                        let cores: Vec<String> = st.smart_colors.iter().map(|(k, c)| format!("{k}={:?}", c.to_hs255())).collect();
                        format!("limiares={}/{} cores=[{}]", st.thr_low, st.thr_high, cores.join(", "))
                    }
                    e => format!("{e:?}"),
                };
                info!(T, "perfil Aura: mode={} efeito={} {} base={:?}", st.mode, st.effect.name(), extra, st.static_color.to_hs255());
                self.state = st;
                true
            }
            Err(e) => {
                // arquivo sendo reescrito pelo LightingService, etc.: tenta de novo no próximo ciclo
                warn!(T, "não consegui ler o perfil do Aura: {e}");
                false
            }
        }
    }
}
