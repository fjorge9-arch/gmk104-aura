//! Laço seguidor: lê o perfil/script do Aura e o log do LightingService, decide o que o teclado
//! deve mostrar e envia ao GMK104.
//!
//! Roda a `fps` Hz. Efeitos fixos só geram comando quando algo muda (mais um reforço a cada
//! `refresh` segundos, que recupera de troca de efeito por Fn ou reconexão). Efeitos animados pelo
//! programa (ciclo de cor, estroboscópio) geram comandos a cada quadro. Publica o estado em
//! `status::STATUS`.

use std::time::{Duration, Instant};

use crate::aura_log::AuraLogWatcher;
use crate::aura_profile::{AuraWatcher, Effect, Hsl};
use crate::keyboard::{self, Gmk104, Target};
use crate::status;
use crate::{debug, info, warn};

const T: &str = "gmk104";

pub struct Config {
    pub fps: f64,
    pub refresh: f64,
    pub duration: f64,
}

/// Período (s) de um ciclo da respiração nativa do GMK104 em cada nível 0..4, medido com webcam.
const KB_BREATH_PERIODS: [f64; 5] = [8.0, 5.0, 3.8, 2.5, 1.25];
/// Período (s) com que a matiz dá uma volta num ponto do teclado no efeito TRAVERSE, por nível (medido).
const KB_TRAVERSE_PERIODS: [f64; 5] = [8.0, 6.0, 4.6, 3.1, 1.54];
/// Arco-íris do Aura medido com webcam: 14,5 s por volta da matiz num ponto, com freq 0.2 no script.
const RAINBOW_SECONDS_AT_DEFAULT: f64 = 14.5;
/// Cadência média das estrelas do efeito STARRY do teclado por nível (medida; nível 0 é bem mais lento).
const KB_STARRY_PERIODS: [f64; 5] = [1.5, 0.8, 0.61, 0.45, 0.30];
const RAINBOW_DEFAULT_FREQ: f64 = 0.2;

/// Nível cuja duração de ciclo mais se aproxima de `period` segundos (comparação em escala log).
fn speed_for_period(period: f64, table: &[f64; 5]) -> u8 {
    let p = period.max(0.05).ln();
    let mut best = 0usize;
    let mut best_d = f64::MAX;
    for (i, t) in table.iter().enumerate() {
        let d = (t.ln() - p).abs();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best as u8
}

/// Ciclo de cor: segundos por volta completa da matiz. Medido com webcam na ROG STRIX X570-E:
/// 12,5 s por volta com o script em freq 0.02 (ajuste padrão de "Tempo"); outras posições do
/// controle escalam proporcionalmente.
const CYCLE_SECONDS_AT_DEFAULT: f64 = 12.5;
const CYCLE_DEFAULT_FREQ: f64 = 0.02;
/// Matiz (graus) da placa ao longo de um ciclo, em 24 passos iguais de fase a partir do vermelho,
/// medida com webcam: o giro não é uniforme (QuarterSineWave no script): rápido no início, lento no fim.
const CYCLE_HUE_TABLE: [f64; 25] = [
    0.0, 24.0, 47.0, 72.0, 95.0, 126.0, 141.0, 148.0, 160.0, 185.0, 215.0, 252.0, 267.0,
    277.0, 288.0, 294.0, 299.0, 305.0, 314.0, 323.0, 331.0, 338.0, 345.0, 352.0, 360.0,
];
/// O ciclo (fase 0 = vermelho) começa este tanto depois de o Aura gravar o script (medido: o verde,
/// que está em ~19 % do ciclo, aparece 2,5 s após a gravação).
const CYCLE_RED_OFFSET: f64 = -1.9; // medido: a placa já está girando ~1,9 s antes de o script ser gravado

fn cycle_hue(phase: f64) -> f64 {
    let p = phase.rem_euclid(1.0) * 24.0;
    let i = p.floor() as usize;
    let f = p - i as f64;
    let (a, b) = (CYCLE_HUE_TABLE[i.min(23)], CYCLE_HUE_TABLE[(i + 1).min(24)]);
    (a + (b - a) * f) / 360.0
}

fn cycle_seconds(freq: f64) -> f64 {
    let f = freq.abs().max(0.001);
    (CYCLE_SECONDS_AT_DEFAULT * CYCLE_DEFAULT_FREQ / f).clamp(2.0, 300.0)
}

pub fn run(cfg: &Config) {
    let mut aura = AuraWatcher::new();
    let mut auralog = AuraLogWatcher::new();
    let mut kb = Gmk104::new();

    let period = Duration::from_secs_f64(1.0 / cfg.fps);
    let refresh = Duration::from_secs_f64(cfg.refresh);
    let start = Instant::now();
    let mut last_sent: Option<Target> = None;
    let mut last_sent_at = Instant::now() - refresh;
    let mut warned_no_source = false;
    let mut was_connected = false;
    let mut last_effect = String::new();
    let mut anim_t0 = Instant::now();
    let mut strobe_base: Option<u8> = None; // brilho do usuário antes do estroboscópio
    let mut restore_brightness: Option<u8> = None;

    while !status::stop_requested() {
        if cfg.duration > 0.0 && start.elapsed().as_secs_f64() >= cfg.duration {
            info!(T, "duração encerrada");
            break;
        }
        aura.poll();
        let st = aura.state.clone();
        let now = Instant::now();

        let effect_key = format!("{:?}", st.effect);
        if effect_key != last_effect {
            last_effect = effect_key;
            anim_t0 = now;
            if !matches!(st.effect, Effect::Strobing { .. }) {
                if let Some(b) = strobe_base.take() {
                    restore_brightness = Some(b); // devolve o brilho que o estroboscópio alterou
                }
            }
        }

        let mut animated = false;
        let (mut target, source): (Target, &'static str) = match &st.effect {
            Effect::Smart => {
                auralog.poll();
                match auralog.color {
                    // cor que o LightingService registrou no log (a decisão exata do Aura)
                    Some(c) => (Target::solid(c.to_hs255()), "Smart (log do Aura)"),
                    None => {
                        if !warned_no_source {
                            let motivo = if auralog.available {
                                "sem cor".to_string()
                            } else {
                                format!("ausente em {}", auralog.path.display())
                            };
                            warn!(T, "modo Smart sem fonte ainda (log do LightingService {motivo}); assumindo a faixa Thermal_Low");
                            warned_no_source = true;
                        }
                        (Target::solid(st.smart_color("Thermal_Low").unwrap_or(st.static_color).to_hs255()), "Smart (faixa fria assumida)")
                    }
                }
            }
            Effect::Off => (
                Target { effect: keyboard::EFFECT_OFF, hue: 0, sat: 0, speed: None, brightness: None },
                "apagado",
            ),
            Effect::Solid(c) => (Target::solid(c.to_hs255()), "cor fixa"),
            Effect::Breathing { color, freq } => {
                let (hue, sat) = color.to_hs255();
                // no script do Aura a freq é em Hz (medido: 0.8 -> 1,25 s por respiração)
                let period = 1.0 / freq.abs().max(0.01);
                (Target { effect: keyboard::EFFECT_BREATHE, hue, sat, speed: Some(speed_for_period(period, &KB_BREATH_PERIODS)), brightness: None }, "efeito nativo")
            }
            Effect::Rainbow { freq } => {
                // medido na placa (anel ARGB): 14,5 s por volta da matiz com freq 0.2; escala com o Tempo
                let period = RAINBOW_SECONDS_AT_DEFAULT * RAINBOW_DEFAULT_FREQ / freq.abs().max(0.01);
                (Target { effect: keyboard::EFFECT_TRAVERSE, hue: 0, sat: 255, speed: Some(speed_for_period(period, &KB_TRAVERSE_PERIODS)), brightness: None }, "efeito nativo")
            }
            Effect::Starry { color, freq } => {
                let (hue, sat) = color.to_hs255();
                // cada estrela da placa acende em senoide de 1/freq s (0.15 Hz -> 6,7 s): bem mais calmo que o teclado
                let period = 1.0 / freq.abs().max(0.01);
                (Target { effect: keyboard::EFFECT_STARRY, hue, sat, speed: Some(speed_for_period(period, &KB_STARRY_PERIODS)), brightness: None }, "efeito nativo")
            }
            Effect::ColorCycle { start: c0, freq } => {
                animated = true;
                let secs = cycle_seconds(*freq);
                // medido na placa: matiz crescente (verde -> ciano -> azul -> rosa -> vermelho -> amarelo),
                // começando no verde ~2 s depois da gravação do perfil; a cor do script não é usada
                let dir = if *freq < 0.0 { 1.0 } else { -1.0 };
                // origem da fase: quando o Aura gravou o script (sobrevive a reinícios do programa)
                let since_change = aura
                    .changed_at
                    .and_then(|c| std::time::SystemTime::now().duration_since(c).ok())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or_else(|| now.duration_since(anim_t0).as_secs_f64());
                let dt = since_change - CYCLE_RED_OFFSET;
                if dt < 0.0 {
                    // a placa ainda mostra a cor anterior: o teclado também
                    match last_sent {
                        Some(t) => (Target::solid((t.hue, t.sat)), "animado pelo programa"),
                        None => (Target::solid(Hsl::RED.to_hs255()), "animado pelo programa"),
                    }
                } else {
                    let h = cycle_hue(dir * dt / secs);
                    let c = Hsl { h, s: if c0.s > 0.0 { c0.s } else { 1.0 }, l: if c0.l > 0.0 { c0.l } else { 0.5 } };
                    (Target::solid(c.to_hs255()), "animado pelo programa")
                }
            }
            Effect::Strobing { color, freq } => {
                animated = true;
                if strobe_base.is_none() {
                    strobe_base = kb.brightness().map(|b| b.max(1));
                }
                let (hue, sat) = color.to_hs255();
                let level = strobe_base.map(|base| {
                    let f = freq.abs().max(0.2);
                    let phase = (now.duration_since(anim_t0).as_secs_f64() * f).fract();
                    ((1.0 - phase) * base as f64).round() as u8 // dente de serra: acende e apaga
                });
                (Target { effect: keyboard::EFFECT_LIGHT, hue, sat, speed: None, brightness: level }, "animado pelo programa")
            }
            Effect::Unsupported { color, .. } => match color.filter(|c| !c.is_black()) {
                Some(c) => (Target::solid(c.to_hs255()), "não suportado: cor base"),
                // sem cor utilizável (Música, Cor adaptável, efeitos baixados): mantém o que está
                None => match last_sent {
                    Some(t) => (Target::solid((t.hue, t.sat)), "não suportado: mantém a cor"),
                    None => (Target::solid(Hsl::RED.to_hs255()), "não suportado: cor padrão"),
                },
            },
        };
        if let Some(b) = restore_brightness {
            target.brightness = Some(b);
        }

        let verify = now.duration_since(last_sent_at) >= refresh;
        if last_sent != Some(target) || verify {
            if kb.apply(&target, verify) {
                let changed = last_sent.map_or(true, |t| t.effect != target.effect || (!animated && (t.hue, t.sat) != (target.hue, target.sat)));
                if changed {
                    info!(T, "teclado <- efeito {} ({}) hue={} sat={} vel={:?}", st.effect.name(), source, target.hue, target.sat, target.speed);
                }
                last_sent = Some(target);
                last_sent_at = now;
                restore_brightness = None;
            } else {
                debug!(T, "teclado indisponível; tentando de novo");
                last_sent_at = now - refresh + Duration::from_secs(1); // tenta de novo em ~1 s
            }
        }

        let connected = kb.connected();
        if connected != was_connected {
            if !connected {
                info!(T, "teclado desconectado; aguardando");
            }
            was_connected = connected;
        }
        let shown = if connected && target.effect != keyboard::EFFECT_OFF { Some((target.hue, target.sat)) } else { None };
        let effect_name = st.effect.name();
        status::update(|s| {
            s.color = if connected { shown } else { s.color };
            s.keyboard_connected = connected;
            s.effect = effect_name;
            s.source = source;
            s.profile_ok = !st.mode.is_empty();
        });
        std::thread::sleep(period);
    }
    // ao sair durante um estroboscópio (ou com restauração pendente), devolve o brilho do usuário
    if let Some(b) = strobe_base.or(restore_brightness) {
        let mut t = last_sent.unwrap_or(Target::solid((0, 255)));
        t.brightness = Some(b);
        if kb.apply(&t, false) {
            info!(T, "brilho do teclado devolvido ao nível {b}");
        }
    }
    info!(T, "encerrando");
}
