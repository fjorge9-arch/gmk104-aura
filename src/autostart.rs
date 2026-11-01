//! "Iniciar com o Windows" pelo valor em HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//! (por usuário, sem administrador). Também remove o atalho antigo da pasta Inicializar, se houver.

use std::path::PathBuf;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "gmk104-aura";

fn exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

fn command() -> Option<String> {
    exe_path().map(|p| format!("\"{}\"", p.display()))
}

pub fn is_enabled() -> bool {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ) else {
        return false;
    };
    key.get_value::<String, _>(VALUE).is_ok()
}

/// `true` se o valor registrado aponta para outro executável (ex.: o programa foi movido).
pub fn points_elsewhere() -> bool {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ) else {
        return false;
    };
    match (key.get_value::<String, _>(VALUE), command()) {
        (Ok(v), Some(c)) => !v.eq_ignore_ascii_case(&c),
        _ => false,
    }
}

pub fn set(enabled: bool) -> Result<(), String> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)
        .map_err(|e| e.to_string())?;
    if enabled {
        let cmd = command().ok_or("caminho do executável desconhecido")?;
        key.set_value(VALUE, &cmd).map_err(|e| e.to_string())?;
        remove_legacy_shortcut();
    } else {
        match key.delete_value(VALUE) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// Atalho `gmk104-aura.lnk` em shell:startup, usado pelas versões anteriores: evita duas instâncias.
pub fn remove_legacy_shortcut() {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let lnk = PathBuf::from(appdata)
            .join(r"Microsoft\Windows\Start Menu\Programs\Startup\gmk104-aura.lnk");
        let _ = std::fs::remove_file(lnk);
    }
}
