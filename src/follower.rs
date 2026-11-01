//! Laço seguidor: lê o perfil do Aura e o log do LightingService, decide a cor e envia ao GMK104.
//!
//! Roda a `fps` Hz. O teclado só recebe comando quando a cor muda ou a cada `refresh` segundos
//! (recupera de troca de efeito por Fn ou reconexão). Publica o estado em `status::STATUS`.

use std::time::{Duration, Instant};

use crate::aura_log::AuraLogWatcher;
use crate::aura_profile::AuraWatcher;
use crate::keyboard::Gmk104;
use crate::status;
use crate::{debug, info, warn};

const T: &str = "gmk104";

pub struct Config {
    pub fps: f64,
    pub refresh: f64,
    pub duration: f64,
}

pub fn run(cfg: &Config) {
    let mut aura = AuraWatcher::new();
    let mut auralog = AuraLogWatcher::new();
    let mut kb = Gmk104::new();

    let period = Duration::from_secs_f64(1.0 / cfg.fps);
    let refresh = Duration::from_secs_f64(cfg.refresh);
    let start = Instant::now();
    let mut last_sent: Option<(u8, u8)> = None;
    let mut last_sent_at = Instant::now() - refresh;
    let mut warned_no_source = false;
    let mut was_connected = false;

    while !status::stop_requested() {
        if cfg.duration > 0.0 && start.elapsed().as_secs_f64() >= cfg.duration {
            info!(T, "duração encerrada");
            return;
        }
        aura.poll();
        let st = &aura.state;

        let (color, source): ((u8, u8), &'static str) = if st.smart {
            auralog.poll();
            match auralog.color {
                // fonte: cor que o LightingService registrou no log (a decisão exata do Aura)
                Some(c) => (c.to_hs255(), "modo Smart (log do Aura)"),
                None => {
                    // sem fonte não há como saber a faixa: assume a faixa fria (verde no perfil padrão)
                    if !warned_no_source {
                        let motivo = if auralog.available {
                            "sem cor".to_string()
                        } else {
                            format!("ausente em {}", auralog.path.display())
                        };
                        warn!(T, "modo Smart sem fonte ainda (log do LightingService {motivo}); assumindo a faixa Thermal_Low");
                        warned_no_source = true;
                    }
                    (st.smart_color("Thermal_Low").unwrap_or(st.static_color).to_hs255(), "modo Smart (faixa fria assumida)")
                }
            }
        } else {
            (st.static_color.to_hs255(), "cor base do perfil")
        };

        let now = Instant::now();
        if last_sent != Some(color) || now.duration_since(last_sent_at) >= refresh {
            if kb.apply(color.0, color.1) {
                if last_sent != Some(color) {
                    info!(T, "teclado <- hue={} sat={}", color.0, color.1);
                }
                last_sent = Some(color);
                last_sent_at = now;
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
        let smart = st.smart;
        let profile_ok = !st.mode.is_empty();
        status::update(|s| {
            s.color = if connected { last_sent } else { s.color };
            s.keyboard_connected = connected;
            s.smart = smart;
            s.source = source;
            s.profile_ok = profile_ok;
        });
        std::thread::sleep(period);
    }
    info!(T, "encerrando a pedido do usuário");
}
