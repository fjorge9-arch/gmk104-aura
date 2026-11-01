//! Log mínimo: arquivo em %LOCALAPPDATA%\gmk104-aura\gmk104-aura.log (rotaciona em 1 MB, guarda .1)
//! e, com `-v`, também o console do terminal que nos chamou (AttachConsole).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

const MAX_BYTES: u64 = 1_000_000;

struct State {
    file: Option<File>,
    path: PathBuf,
    console: bool,
    debug: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn log_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("gmk104-aura")
}

pub fn init(verbose: bool) {
    let dir = log_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("gmk104-aura.log");
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    // SAFETY: chamada simples da API do Windows; falha (sem console pai) é ignorada.
    let console = verbose && unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;
    *STATE.lock().unwrap() = Some(State { file, path, console, debug: verbose });
}

fn timestamp() -> String {
    let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: GetLocalTime só escreve na struct fornecida.
    unsafe { GetLocalTime(&mut t) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02},{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

fn rotate(st: &mut State) {
    let big = st
        .file
        .as_ref()
        .and_then(|f| f.metadata().ok())
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false);
    if !big {
        return;
    }
    st.file = None;
    let bak = st.path.with_extension("log.1");
    let _ = fs::remove_file(&bak);
    let _ = fs::rename(&st.path, &bak);
    st.file = OpenOptions::new().create(true).append(true).open(&st.path).ok();
}

pub fn write(level: &str, target: &str, msg: &str) {
    let mut guard = STATE.lock().unwrap();
    let Some(st) = guard.as_mut() else {
        eprintln!("{level} {target}: {msg}");
        return;
    };
    if level == "DEBUG" && !st.debug {
        return;
    }
    let line = format!("{} {} {}: {}\n", timestamp(), level, target, msg);
    rotate(st);
    if let Some(f) = st.file.as_mut() {
        let _ = f.write_all(line.as_bytes());
    }
    if st.console {
        let _ = std::io::stdout().write_all(line.as_bytes());
        let _ = std::io::stdout().flush();
    }
}

#[macro_export]
macro_rules! info {
    ($target:expr, $($arg:tt)*) => { $crate::logger::write("INFO", $target, &format!($($arg)*)) };
}
#[macro_export]
macro_rules! warn {
    ($target:expr, $($arg:tt)*) => { $crate::logger::write("WARNING", $target, &format!($($arg)*)) };
}
#[macro_export]
macro_rules! debug {
    ($target:expr, $($arg:tt)*) => { $crate::logger::write("DEBUG", $target, &format!($($arg)*)) };
}
