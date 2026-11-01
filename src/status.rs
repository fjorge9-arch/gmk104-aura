//! Estado compartilhado entre a thread seguidora e o ícone de bandeja.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Última cor enviada ao teclado (hue, sat) em 0..255; None = apagado/desconhecido.
    pub color: Option<(u8, u8)>,
    pub keyboard_connected: bool,
    /// Nome do efeito ativo na placa ("Smart", "Intermitente", ...).
    pub effect: &'static str,
    /// Como o teclado está reproduzindo ("efeito nativo", "animado pelo programa", ...).
    pub source: &'static str,
    pub profile_ok: bool,
}

pub static STATUS: Mutex<Status> = Mutex::new(Status {
    color: None,
    keyboard_connected: false,
    effect: "",
    source: "",
    profile_ok: false,
});

pub static STOP: AtomicBool = AtomicBool::new(false);

pub fn update(f: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = STATUS.lock() {
        f(&mut s);
    }
}

pub fn snapshot() -> Status {
    STATUS.lock().map(|s| s.clone()).unwrap_or_default()
}

pub fn stop_requested() -> bool {
    STOP.load(Ordering::Relaxed)
}

pub fn request_stop() {
    STOP.store(true, Ordering::Relaxed);
}

/// Nome da cor em português a partir de (hue, sat) 0..255, para tooltip e menu.
pub fn color_name(hs: (u8, u8)) -> &'static str {
    let (h, s) = hs;
    if s < 40 {
        return "branco";
    }
    let deg = h as f64 * 360.0 / 256.0;
    match deg as u32 {
        0..=14 | 345..=360 => "vermelho",
        15..=39 => "laranja",
        40..=69 => "amarelo",
        70..=164 => "verde",
        165..=194 => "ciano",
        195..=259 => "azul",
        260..=299 => "roxo",
        _ => "rosa",
    }
}

/// (hue, sat) 0..255 com valor máximo -> (r, g, b) 0..255, para desenhar o ícone.
pub fn hs_to_rgb(hs: (u8, u8)) -> (u8, u8, u8) {
    let (h, s) = (hs.0 as f64 / 256.0 * 6.0, hs.1 as f64 / 255.0);
    let i = h.floor() as i32;
    let f = h - h.floor();
    let p = 1.0 - s;
    let q = 1.0 - s * f;
    let t = 1.0 - s * (1.0 - f);
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (1.0, t, p),
        1 => (q, 1.0, p),
        2 => (p, 1.0, t),
        3 => (p, q, 1.0),
        4 => (t, p, 1.0),
        _ => (1.0, p, q),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
