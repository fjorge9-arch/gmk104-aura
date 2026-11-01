//! Estado atual do ASUS Aura Sync a partir dos arquivos do LightingService.
//!
//! - LastProfile.xml: perfil atual. mode key="100" = Smart (térmico); nos demais modos usamos a cor
//!   base do LED 0 da placa-mãe (Mainboard_Master).
//! - script/LastScript.xml: no modo Smart, limiares efetivos e cores (hue/saturation/lightness) de
//!   cada faixa (Thermal_Low / Thermal_Between / Thermal_High).
//!
//! Os dois arquivos são reescritos pelo LightingService a cada mudança no Armoury Crate; só relemos
//! quando o mtime muda.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use roxmltree::{Document, Node};

use crate::{info, warn};

const T: &str = "gmk104.aura";
const MODE_SMART: &str = "100";

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

#[derive(Clone, Debug)]
pub struct AuraState {
    pub mode: String,
    pub smart: bool,
    pub thr_low: f64,
    pub thr_high: f64,
    pub smart_colors: Vec<(String, Hsl)>, // trigger -> cor
    pub static_color: Hsl,
}

impl Default for AuraState {
    fn default() -> Self {
        Self {
            mode: String::new(),
            smart: false,
            thr_low: 65.0,
            thr_high: 85.0,
            smart_colors: Vec::new(),
            static_color: Hsl { h: 0.0, s: 1.0, l: 0.5 },
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
    s.and_then(|v| v.parse().ok())
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
    st.smart = st.mode == MODE_SMART;

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
    Ok(st)
}

/// Complementa com o script: limiares efetivos e cores por faixa (modo Smart).
pub fn parse_script(mut st: AuraState, path: &Path) -> Result<AuraState, String> {
    let Ok(text) = fs::read_to_string(path) else { return Ok(st) };
    let doc = Document::parse(&text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let effects: Vec<Node> = root.descendants().filter(|n| n.has_tag_name("effect")).collect();
    for eff in effects.iter().filter(|e| e.attribute("key") == Some("Smartmode")) {
        if let (Some(lo), Some(hi)) = (parse_f(child_text(*eff, "Threshold_low")), parse_f(child_text(*eff, "Threshold_high"))) {
            st.thr_low = lo;
            st.thr_high = hi;
        }
        let Some(trig) = child_text(*eff, "trigger") else { continue };
        if st.smart_color(trig).is_some() {
            continue;
        }
        let Some(sub_key) = eff.children().find(|c| c.has_tag_name("smartmode_effect")).and_then(|c| c.attribute("key")) else { continue };
        let Some(e) = effects.iter().find(|e| e.attribute("key") == Some(sub_key)) else { continue };
        if let Some(h) = parse_f(desc_text(*e, "hue")) {
            let s = parse_f(desc_text(*e, "saturation")).unwrap_or(1.0);
            let l = parse_f(desc_text(*e, "lightness")).unwrap_or(0.5);
            st.smart_colors.push((trig.to_string(), Hsl { h, s, l }));
        }
    }
    Ok(st)
}

/// Relê os arquivos quando mudam; mantém o último estado válido.
pub struct AuraWatcher {
    profile: PathBuf,
    script: PathBuf,
    mtimes: (Option<SystemTime>, Option<SystemTime>),
    pub state: AuraState,
}

impl AuraWatcher {
    pub fn new() -> Self {
        let dir = ls_dir();
        Self {
            profile: dir.join("LastProfile.xml"),
            script: dir.join("script").join("LastScript.xml"),
            mtimes: (None, None),
            state: AuraState::default(),
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
                let cores: Vec<String> = st.smart_colors.iter().map(|(k, c)| format!("{k}={:?}", c.to_hs255())).collect();
                info!(
                    T,
                    "perfil Aura: mode={} smart={} limiares={}/{} cores=[{}] estatica={:?}",
                    st.mode, st.smart, st.thr_low, st.thr_high, cores.join(", "), st.static_color.to_hs255()
                );
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
