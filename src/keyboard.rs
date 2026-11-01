//! Controle do ZUOYA GMK104 pelo canal VIA Raw HID (interface usage page 0xFF60, usage 0x61).
//!
//! Protocolo (QMK/VIA "custom values", canal 3 = rgb_matrix), relatórios de 32 bytes:
//!   [0x07, canal, id, dados...]  id_custom_set_value
//!   [0x08, canal, id]            id_custom_get_value (resposta ecoa o valor)
//!   id 1 = brilho (0..4; o firmware limita a 4 e valores maiores apagam o teclado)
//!   id 2 = efeito (lista em `EFFECT_*`; 6 = LIGHT_MODE, cor única)
//!   id 3 = velocidade (0..4)
//!   id 4 = cor [hue 0..255, sat 0..255]
//! Os valores ficam em RAM no teclado: pode-se escrever muitas vezes sem desgaste.

use hidapi::{HidApi, HidDevice};

use crate::{debug, info, warn};

pub const VENDOR_ID: u16 = 0x320F;
pub const PRODUCT_IDS: [u16; 2] = [0x5055, 0x5088]; // cabo USB, receptor 2.4G
const VIA_USAGE_PAGE: u16 = 0xFF60;
const VIA_USAGE: u16 = 0x61;

const CMD_SET: u8 = 0x07;
const CMD_GET: u8 = 0x08;
const CH_RGB_MATRIX: u8 = 3;
const ID_BRIGHTNESS: u8 = 1;
const ID_EFFECT: u8 = 2;
const ID_SPEED: u8 = 3;
const ID_COLOR: u8 = 4;
const READ_TIMEOUT_MS: i32 = 300;

/// Efeitos nativos do firmware (lista completa no JSON do VIA do teclado).
pub const EFFECT_OFF: u8 = 0;
pub const EFFECT_BREATHE: u8 = 5;
pub const EFFECT_LIGHT: u8 = 6;
pub const EFFECT_STARRY: u8 = 10;
/// Degradê de cores atravessando o teclado (medido: é o "arco-íris" real; WAVE é onda de uma cor só).
pub const EFFECT_TRAVERSE: u8 = 12;
pub const BRIGHTNESS_MAX: u8 = 4;
pub const SPEED_MAX: u8 = 4;

const T: &str = "gmk104.keyboard";

/// O que o teclado deve mostrar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub effect: u8,
    pub hue: u8,
    pub sat: u8,
    /// Velocidade do efeito nativo (0..4); None = não mexer.
    pub speed: Option<u8>,
    /// Brilho (0..4); None = não mexer (fica o ajuste do usuário).
    pub brightness: Option<u8>,
}

impl Target {
    pub fn solid(hs: (u8, u8)) -> Self {
        Self { effect: EFFECT_LIGHT, hue: hs.0, sat: hs.1, speed: None, brightness: None }
    }
}

pub struct Gmk104 {
    api: Option<HidApi>,
    dev: Option<HidDevice>,
    /// Último alvo confirmado pelo teclado (None = desconhecido, reenviar tudo).
    cur: Option<Target>,
}

impl Gmk104 {
    pub fn new() -> Self {
        Self { api: None, dev: None, cur: None }
    }

    pub fn connected(&self) -> bool {
        self.dev.is_some()
    }

    fn api(&mut self) -> Result<&mut HidApi, String> {
        match self.api.as_mut() {
            Some(api) => api.refresh_devices().map_err(|e| e.to_string())?,
            None => self.api = Some(HidApi::new().map_err(|e| e.to_string())?),
        }
        Ok(self.api.as_mut().unwrap())
    }

    /// Procura a interface VIA do teclado e abre. `false` se não está conectado.
    pub fn connect(&mut self) -> bool {
        self.cur = None;
        let api = match self.api() {
            Ok(a) => a,
            Err(e) => {
                warn!(T, "hidapi indisponível: {e}");
                return false;
            }
        };
        // Cabo (5055) e receptor 2.4G (5088) podem estar os dois presentes; só um responde.
        // Tenta na ordem de PRODUCT_IDS e fica com a primeira interface que responde ao VIA.
        let mut candidates: Vec<(u16, std::ffi::CString)> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == VENDOR_ID
                    && PRODUCT_IDS.contains(&d.product_id())
                    && d.usage_page() == VIA_USAGE_PAGE
                    && d.usage() == VIA_USAGE
            })
            .map(|d| (d.product_id(), d.path().to_owned()))
            .collect();
        candidates.sort_by_key(|(pid, _)| PRODUCT_IDS.iter().position(|p| p == pid));
        for (pid, path) in candidates {
            let opened = self.api.as_ref().expect("hidapi inicializada").open_path(&path);
            match opened {
                Ok(dev) => {
                    self.dev = Some(dev);
                    match self.get_value(ID_EFFECT) {
                        Ok(Some(_)) => {
                            info!(T, "teclado conectado (PID {pid:04X}): {}", path.to_string_lossy());
                            return true;
                        }
                        Ok(None) => debug!(T, "interface PID {pid:04X} não respondeu; tentando a próxima"),
                        Err(e) => debug!(T, "interface PID {pid:04X}: {e}; tentando a próxima"),
                    }
                    self.dev = None;
                }
                Err(e) => warn!(T, "falha ao abrir teclado (PID {pid:04X}): {e}"),
            }
        }
        false
    }

    pub fn close(&mut self) {
        self.dev = None;
        self.cur = None;
    }

    fn xfer(&self, payload: &[u8]) -> Result<Option<[u8; 32]>, String> {
        let dev = self.dev.as_ref().ok_or("teclado desconectado")?;
        let mut buf = [0u8; 33]; // byte 0 = report id 0
        buf[1..1 + payload.len()].copy_from_slice(payload);
        dev.write(&buf).map_err(|e| e.to_string())?;
        let mut resp = [0u8; 32];
        let n = dev.read_timeout(&mut resp, READ_TIMEOUT_MS).map_err(|e| e.to_string())?;
        Ok(if n > 0 { Some(resp) } else { None })
    }

    fn set_value(&self, id: u8, data: &[u8]) -> Result<bool, String> {
        let mut p = vec![CMD_SET, CH_RGB_MATRIX, id];
        p.extend_from_slice(data);
        Ok(matches!(self.xfer(&p)?, Some(r) if r[0] == CMD_SET))
    }

    fn get_value(&self, id: u8) -> Result<Option<u8>, String> {
        Ok(self.xfer(&[CMD_GET, CH_RGB_MATRIX, id])?.map(|r| r[3]))
    }

    /// Nível de brilho atual do teclado (0..4), se conectado e respondendo.
    pub fn brightness(&mut self) -> Option<u8> {
        if self.dev.is_none() && !self.connect() {
            return None;
        }
        match self.get_value(ID_BRIGHTNESS) {
            Ok(v) => v.map(|b| b.min(BRIGHTNESS_MAX)),
            Err(e) => {
                warn!(T, "erro lendo brilho ({e}); vou reconectar");
                self.close();
                None
            }
        }
    }

    /// Leva o teclado ao alvo, enviando só o que mudou. Com `verify`, confere o efeito no teclado
    /// (o usuário pode ter trocado por Fn) e reenvia a cor. Reconecta se necessário.
    /// `false` se o teclado não respondeu; o chamador tenta de novo no próximo ciclo.
    pub fn apply(&mut self, t: &Target, verify: bool) -> bool {
        if self.dev.is_none() && !self.connect() {
            return false;
        }
        let cur = self.cur;
        let r = (|| -> Result<bool, String> {
            let mut ok = true;
            let mut effect_changed = cur.map_or(true, |c| c.effect != t.effect);
            if verify && !effect_changed && self.get_value(ID_EFFECT)? != Some(t.effect) {
                effect_changed = true;
            }
            if effect_changed {
                ok &= self.set_value(ID_EFFECT, &[t.effect])?;
            }
            if let Some(sp) = t.speed {
                if effect_changed || cur.and_then(|c| c.speed) != Some(sp) {
                    ok &= self.set_value(ID_SPEED, &[sp.min(SPEED_MAX)])?;
                }
            }
            if verify || effect_changed || cur.map_or(true, |c| (c.hue, c.sat) != (t.hue, t.sat)) {
                ok &= self.set_value(ID_COLOR, &[t.hue, t.sat])?;
            }
            if let Some(b) = t.brightness {
                if cur.and_then(|c| c.brightness) != Some(b) {
                    ok &= self.set_value(ID_BRIGHTNESS, &[b.min(BRIGHTNESS_MAX)])?;
                }
            }
            Ok(ok)
        })();
        match r {
            Ok(true) => {
                self.cur = Some(*t);
                true
            }
            Ok(false) => {
                self.cur = None;
                false
            }
            Err(e) => {
                warn!(T, "erro de comunicação com o teclado ({e}); vou reconectar");
                self.close();
                false
            }
        }
    }
}
