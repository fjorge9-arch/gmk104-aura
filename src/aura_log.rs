//! Cor atual do Aura a partir do log do LightingService (não exige administrador).
//!
//! No modo Smart o LightingService registra, a cada troca de faixa térmica (e a cada reaplicação do
//! perfil), a cor que passa a aplicar na placa-mãe:
//!
//!   ... [ScriptEvent][ParseInitColor] parsed value => hue=0.333333, saturation=1.000000, lightness=0.500000, alpha=1.000000
//!
//! O arquivo é público (%ProgramData%\ASUS\ARMOURY CRATE Diagnosis\LightingService\LightingService.log),
//! gravado com flush imediato e rotacionado a 10 MB (log4cxx: .log.1 … .log.10). Acompanhamos só o que
//! foi acrescentado desde a última leitura; se o arquivo encolher (rotação), recomeçamos do início.
//! Como a cor só é registrada quando muda, logo após uma rotação o arquivo atual pode não ter nenhuma
//! linha de cor: nesse caso procuramos a última nos backups (.1, .2, .3).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::aura_profile::Hsl;
use crate::{debug, info};

const T: &str = "gmk104.auralog";
const NEEDLE: &[u8] = b"[ParseInitColor] parsed value => hue=";
const TAIL_BYTES: u64 = 512 * 1024; // quanto ler do fim na primeira passada
const KEEP: usize = 256; // resto guardado entre leituras (linha possivelmente cortada)
const BACKUPS: u32 = 3;

pub fn log_path() -> PathBuf {
    if let Some(p) = std::env::var_os("GMK104_LS_LOG") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("PROGRAMDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join("ASUS").join("ARMOURY CRATE Diagnosis").join("LightingService").join("LightingService.log")
}

/// Última ocorrência completa da linha de cor em `data`: (cor, índice logo após o trecho lido).
fn last_color(data: &[u8]) -> Option<(Hsl, usize)> {
    let n = NEEDLE.len();
    if data.len() < n {
        return None;
    }
    let mut start = data.len() - n;
    loop {
        if &data[start..start + n] == NEEDLE {
            if let Some(r) = parse_after(&data[start + n..]) {
                return Some((r.0, start + n + r.1));
            }
            // linha cortada ou malformada: procura a anterior
        }
        if start == 0 {
            return None;
        }
        start -= 1;
    }
}

/// Lê "H, saturation=S, lightness=L," a partir do início de `s`; devolve a cor e quantos bytes usou.
fn parse_after(s: &[u8]) -> Option<(Hsl, usize)> {
    let end = s.iter().position(|&b| b == b'\n').unwrap_or(s.len());
    let line = std::str::from_utf8(&s[..end]).ok()?;
    if !line.contains("lightness=") || !line.contains("alpha=") {
        return None; // ainda incompleta (o log é gravado em pedaços)
    }
    let mut h = None;
    let mut sat = None;
    let mut l = None;
    for (i, tok) in line.split(", ").enumerate() {
        let tok = tok.trim();
        if i == 0 {
            h = tok.parse().ok();
        } else if let Some(v) = tok.strip_prefix("saturation=") {
            sat = v.parse().ok();
        } else if let Some(v) = tok.strip_prefix("lightness=") {
            l = v.parse().ok();
        }
    }
    Some((Hsl { h: h?, s: sat?, l: l? }, end))
}

fn last_color_in_file(path: &PathBuf, tail: Option<u64>) -> Option<Hsl> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut data = Vec::new();
    if let Some(t) = tail {
        if size > t {
            f.seek(SeekFrom::Start(size - t)).ok()?;
        }
    }
    f.read_to_end(&mut data).ok()?;
    last_color(&data).map(|(c, _)| c)
}

/// Mantém `color` = última cor registrada no log (None até achar uma).
pub struct AuraLogWatcher {
    pub path: PathBuf,
    pub color: Option<Hsl>,
    pub available: bool, // arquivo existe e já foi lido ao menos uma vez
    pos: Option<u64>,
    buf: Vec<u8>,
}

impl AuraLogWatcher {
    pub fn new() -> Self {
        Self { path: log_path(), color: None, available: false, pos: None, buf: Vec::new() }
    }

    fn set(&mut self, c: Hsl, origem: &str) -> bool {
        if self.color == Some(c) {
            return false;
        }
        self.color = Some(c);
        info!(T, "log do LightingService ({origem}): cor da placa hue={:.4} sat={:.2} light={:.2} -> teclado {:?}", c.h, c.s, c.l, c.to_hs255());
        true
    }

    fn from_backups(&mut self) -> bool {
        for i in 1..=BACKUPS {
            let p = PathBuf::from(format!("{}.{i}", self.path.display()));
            if let Some(c) = last_color_in_file(&p, None) {
                return self.set(c, &format!("backup .{i}"));
            }
        }
        false
    }

    /// Lê o que há de novo; `true` se a cor mudou.
    pub fn poll(&mut self) -> bool {
        let Ok(size) = fs::metadata(&self.path).map(|m| m.len()) else {
            self.available = false;
            return false;
        };
        let first = self.pos.is_none();
        let mut pos = self.pos.unwrap_or(0);
        if first || size < pos {
            pos = if first { size.saturating_sub(TAIL_BYTES) } else { 0 };
            self.buf.clear();
            if !first {
                info!(T, "log do LightingService rotacionou; relendo do início");
            }
        }
        if size == pos {
            self.pos = Some(pos);
            if !self.available {
                self.available = true;
                if self.color.is_none() {
                    return self.from_backups();
                }
            }
            return false;
        }
        let mut data = std::mem::take(&mut self.buf);
        let read = (|| -> std::io::Result<()> {
            let mut f = File::open(&self.path)?;
            f.seek(SeekFrom::Start(pos))?;
            f.take(size - pos).read_to_end(&mut data).map(|_| ())
        })();
        if let Err(e) = read {
            debug!(T, "falha lendo log do LightingService: {e}");
            self.buf = data;
            return false;
        }
        self.pos = Some(size);
        self.available = true;
        match last_color(&data) {
            Some((c, end)) => {
                self.buf = data[end..].to_vec();
                self.buf.truncate(KEEP);
                self.set(c, "atual")
            }
            None => {
                let keep_from = data.len().saturating_sub(KEEP);
                self.buf = data[keep_from..].to_vec();
                if self.color.is_none() { self.from_backups() } else { false }
            }
        }
    }
}
