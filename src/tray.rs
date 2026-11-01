//! Ícone na área de notificação (Win32 puro, sem dependências): mostra a cor atual do teclado,
//! tooltip com o estado e menu com "Iniciar com o Windows", "Abrir log" e "Sair".

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, MessageBoxW, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetTimer, TrackPopupMenu, TranslateMessage, HICON,
    ICONINFO, MB_ICONINFORMATION, MB_ICONQUESTION, MB_YESNO, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG,
    SM_CXSMICON, SM_CYSMICON, SW_SHOWNORMAL, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
    IDYES,
};

use crate::{autostart, info, logger, status, warn};

const T: &str = "gmk104.tray";
const WM_TRAY: u32 = WM_APP + 1;
const TIMER_ID: usize = 1;
const ICON_ID: u32 = 1;

const ID_STATUS: usize = 1;
const ID_COLOR: usize = 2;
const ID_AUTOSTART: usize = 10;
const ID_OPEN_LOG: usize = 11;
const ID_EXIT: usize = 99;

struct Tray {
    hwnd: HWND,
    icon: HICON,
    icon_color: Option<(u8, u8)>,
    tip: String,
    taskbar_created: u32,
}
// SAFETY: só a thread da interface toca nestes handles.
unsafe impl Send for Tray {}

static TRAY: Mutex<Option<Tray>> = Mutex::new(None);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn fill_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    let w: Vec<u16> = tip.encode_utf16().take(127).collect();
    nid.szTip = [0; 128];
    nid.szTip[..w.len()].copy_from_slice(&w);
}

/// Ícone redondo com a cor do teclado (cinza quando desconectado), desenhado em memória.
unsafe fn make_icon(color: Option<(u8, u8)>) -> HICON {
    let w = GetSystemMetrics(SM_CXSMICON).max(16);
    let h = GetSystemMetrics(SM_CYSMICON).max(16);
    let (r, g, b) = color.map(status::hs_to_rgb).unwrap_or((120, 120, 120));

    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h, // top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..std::mem::zeroed()
    };
    let mut bits: *mut c_void = null_mut();
    let hbm_color: HBITMAP = CreateDIBSection(null_mut(), &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if hbm_color.is_null() || bits.is_null() {
        return null_mut();
    }
    let px = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let radius = (w.min(h) as f64) / 2.0 - 1.0;
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
            let a = (radius - d + 0.5).clamp(0.0, 1.0); // borda suavizada
            // anel escuro fino para o ícone aparecer em fundo claro e escuro
            let ring = ((radius - 1.5) - d + 0.5).clamp(0.0, 1.0);
            let (rr, gg, bb) = (
                (r as f64 * ring + 30.0 * (1.0 - ring)) * a,
                (g as f64 * ring + 30.0 * (1.0 - ring)) * a,
                (b as f64 * ring + 30.0 * (1.0 - ring)) * a,
            );
            let i = ((y * w + x) * 4) as usize;
            px[i] = bb as u8; // B
            px[i + 1] = gg as u8; // G
            px[i + 2] = rr as u8; // R
            px[i + 3] = (a * 255.0) as u8; // A (pré-multiplicado)
        }
    }
    let hbm_mask: HBITMAP = CreateBitmap(w, h, 1, 1, null());
    let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: hbm_mask, hbmColor: hbm_color };
    let hicon = CreateIconIndirect(&ii);
    DeleteObject(hbm_color);
    DeleteObject(hbm_mask);
    hicon
}

fn nid_base(hwnd: HWND) -> NOTIFYICONDATAW {
    // SAFETY: struct só de dados; zerada e preenchida campo a campo.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid
}

fn build_tip(s: &status::Status) -> String {
    let cor = match s.color {
        Some(c) if s.keyboard_connected => status::color_name(c).to_string(),
        _ if s.effect == "Escuro" => "apagado".to_string(),
        _ => "—".to_string(),
    };
    let kb = if s.keyboard_connected { "conectado" } else { "não encontrado" };
    let efeito = if s.effect.is_empty() { "—" } else { s.effect };
    format!("gmk104-aura\nTeclado: {kb}\nEfeito: {efeito}\nCor: {cor}")
}

/// Atualiza ícone e tooltip se algo mudou. Chamado pelo timer.
unsafe fn refresh(tray: &mut Tray, force: bool) {
    let s = status::snapshot();
    let shown = if s.keyboard_connected { s.color } else { None };
    let tip = build_tip(&s);
    if !force && shown == tray.icon_color && tip == tray.tip {
        return;
    }
    let mut nid = nid_base(tray.hwnd);
    nid.uFlags = NIF_TIP;
    if shown != tray.icon_color || tray.icon.is_null() || force {
        let new_icon = make_icon(shown);
        if !new_icon.is_null() {
            if !tray.icon.is_null() {
                DestroyIcon(tray.icon);
            }
            tray.icon = new_icon;
            tray.icon_color = shown;
        }
    }
    nid.uFlags |= NIF_ICON;
    nid.hIcon = tray.icon;
    fill_tip(&mut nid, &tip);
    tray.tip = tip;
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

unsafe fn add_icon(tray: &mut Tray) {
    let mut nid = nid_base(tray.hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    if tray.icon.is_null() {
        tray.icon = make_icon(None);
    }
    nid.hIcon = tray.icon;
    fill_tip(&mut nid, "gmk104-aura");
    Shell_NotifyIconW(NIM_ADD, &nid);
    refresh(tray, true);
}

pub fn balloon(title: &str, text: &str) {
    let Ok(mut g) = TRAY.lock() else { return };
    let Some(tray) = g.as_mut() else { return };
    let mut nid = nid_base(tray.hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_INFO;
    let t: Vec<u16> = title.encode_utf16().take(63).collect();
    nid.szInfoTitle[..t.len()].copy_from_slice(&t);
    let x: Vec<u16> = text.encode_utf16().take(255).collect();
    nid.szInfo[..x.len()].copy_from_slice(&x);
    // SAFETY: nid está completamente inicializada.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

unsafe fn show_menu(hwnd: HWND) {
    let s = status::snapshot();
    let menu = CreatePopupMenu();
    let kb = if s.keyboard_connected { "Teclado GMK104: conectado" } else { "Teclado GMK104: não encontrado" };
    AppendMenuW(menu, MF_STRING | MF_GRAYED, ID_STATUS, wide(kb).as_ptr());
    let cor = match s.color {
        Some(c) if s.keyboard_connected => format!("Efeito: {} — {} ({})", s.effect, status::color_name(c), s.source),
        _ if !s.profile_ok => "Perfil do Aura não encontrado".to_string(),
        _ if s.effect == "Escuro" => "Efeito: Escuro — apagado".to_string(),
        _ => "Cor: aguardando".to_string(),
    };
    AppendMenuW(menu, MF_STRING | MF_GRAYED, ID_COLOR, wide(&cor).as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    let chk = if autostart::is_enabled() { MF_CHECKED } else { 0 };
    AppendMenuW(menu, MF_STRING | chk, ID_AUTOSTART, wide("Iniciar com o Windows").as_ptr());
    AppendMenuW(menu, MF_STRING, ID_OPEN_LOG, wide("Abrir log").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, ID_EXIT, wide("Sair").as_ptr());

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd); // necessário para o menu fechar ao clicar fora
    let cmd = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x,
        pt.y,
        0,
        hwnd,
        null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if cmd > 0 {
        handle_command(hwnd, cmd as usize);
    }
}

unsafe fn handle_command(hwnd: HWND, id: usize) {
    match id {
        ID_AUTOSTART => {
            let enable = !autostart::is_enabled();
            match autostart::set(enable) {
                Ok(()) => info!(T, "iniciar com o Windows: {}", if enable { "ativado" } else { "desativado" }),
                Err(e) => {
                    warn!(T, "não consegui alterar o início automático: {e}");
                    MessageBoxW(
                        hwnd,
                        wide(&format!("Não consegui alterar o início automático:\n{e}")).as_ptr(),
                        wide("gmk104-aura").as_ptr(),
                        MB_ICONINFORMATION,
                    );
                }
            }
        }
        ID_OPEN_LOG => {
            let path = logger::log_dir().join("gmk104-aura.log");
            ShellExecuteW(hwnd, wide("open").as_ptr(), wide(&path.to_string_lossy()).as_ptr(), null(), null(), SW_SHOWNORMAL);
        }
        ID_EXIT => {
            status::request_stop();
            if let Ok(mut g) = TRAY.lock() {
                if let Some(tray) = g.as_mut() {
                    let nid = nid_base(tray.hwnd);
                    Shell_NotifyIconW(NIM_DELETE, &nid);
                }
            }
            PostQuitMessage(0);
        }
        _ => {}
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            if lparam as u32 == WM_RBUTTONUP || lparam as u32 == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_TIMER if wparam == TIMER_ID => {
            if let Ok(mut g) = TRAY.lock() {
                if let Some(tray) = g.as_mut() {
                    refresh(tray, false);
                }
            }
            0
        }
        WM_COMMAND => {
            handle_command(hwnd, (wparam & 0xFFFF) as usize);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => {
            // Explorer reiniciou: recria o ícone
            if let Ok(mut g) = TRAY.lock() {
                if let Some(tray) = g.as_mut() {
                    if msg == tray.taskbar_created && msg != 0 {
                        add_icon(tray);
                        return 0;
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

/// Pergunta sim/não ao usuário (janela padrão do Windows).
pub fn ask_yes_no(title: &str, text: &str) -> bool {
    // SAFETY: strings terminadas em zero, sem janela pai.
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide(title).as_ptr(), MB_YESNO | MB_ICONQUESTION) == IDYES }
}

pub fn message(title: &str, text: &str) {
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide(title).as_ptr(), MB_ICONINFORMATION) };
}

/// Cria a janela oculta e o ícone e roda o laço de mensagens até "Sair". Bloqueia a thread atual.
pub fn run() {
    unsafe {
        let hinst = GetModuleHandleW(null());
        let class = wide("gmk104-aura-tray");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0, class.as_ptr(), wide("gmk104-aura").as_ptr(), WS_OVERLAPPED,
            0, 0, 0, 0, null_mut(), null_mut(), hinst, null(),
        );
        if hwnd.is_null() {
            warn!(T, "não consegui criar a janela da bandeja");
            return;
        }
        let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        let mut tray = Tray { hwnd, icon: null_mut(), icon_color: None, tip: String::new(), taskbar_created };
        add_icon(&mut tray);
        *TRAY.lock().unwrap() = Some(tray);
        SetTimer(hwnd, TIMER_ID, 500, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
