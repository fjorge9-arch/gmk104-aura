//! gmk104-aura: faz o teclado ZUOYA GMK104 acompanhar a cor que o ASUS Aura Sync aplica na placa-mãe.
//!
//! App de bandeja sem janela: a thread seguidora (`follower`) lê o perfil do Aura e o log do
//! LightingService e fala com o teclado; a thread principal mostra o ícone com a cor atual e o menu.
//! Log em %LOCALAPPDATA%\gmk104-aura\gmk104-aura.log.
#![windows_subsystem = "windows"]

mod aura_log;
mod aura_profile;
mod autostart;
mod follower;
mod keyboard;
mod logger;
mod status;
mod tray;

use std::ptr::null;

use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const T: &str = "gmk104";

struct Args {
    cfg: follower::Config,
    verbose: bool,
    no_tray: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        cfg: follower::Config { fps: 20.0, refresh: 5.0, duration: 0.0 },
        verbose: false,
        no_tray: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut num = |name: &str| -> Result<f64, String> {
            it.next()
                .ok_or(format!("{name} precisa de um valor"))?
                .parse()
                .map_err(|_| format!("{name}: número inválido"))
        };
        match arg.as_str() {
            "--fps" => a.cfg.fps = num("--fps")?,
            "--refresh" => a.cfg.refresh = num("--refresh")?,
            "--duration" => a.cfg.duration = num("--duration")?,
            "--no-tray" => a.no_tray = true,
            "-v" | "--verbose" => a.verbose = true,
            "-h" | "--help" => {
                return Err("uso: gmk104-aura [--fps N] [--refresh S] [--duration S] [--no-tray] [-v]".into())
            }
            other => return Err(format!("argumento desconhecido: {other}")),
        }
    }
    if a.cfg.fps <= 0.0 {
        return Err("--fps deve ser > 0".into());
    }
    Ok(a)
}

/// Só uma instância por sessão: a segunda avisa e sai.
fn single_instance() -> bool {
    let name: Vec<u16> = "Local\\gmk104-aura-instancia".encode_utf16().chain([0]).collect();
    // SAFETY: nome terminado em zero; o handle fica aberto até o processo terminar (de propósito).
    let h = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
    !(h.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS)
}

fn first_run_setup() {
    let marker = logger::log_dir().join("configurado");
    if marker.exists() {
        if autostart::points_elsewhere() {
            // o exe foi movido: atualiza o caminho registrado sem perguntar
            if autostart::set(true).is_ok() {
                info!(T, "início automático atualizado para o novo caminho do executável");
            }
        }
        return;
    }
    let _ = std::fs::write(&marker, VERSION);
    autostart::remove_legacy_shortcut();
    if !autostart::is_enabled() {
        let yes = tray::ask_yes_no(
            "gmk104-aura",
            "Iniciar o gmk104-aura junto com o Windows?\n\n\
             Ele fica como um ícone ao lado do relógio e mantém o teclado GMK104 \
             na mesma cor da placa-mãe (ASUS Aura Sync).\n\n\
             Dá para mudar depois pelo menu do ícone.",
        );
        if yes {
            match autostart::set(true) {
                Ok(()) => info!(T, "início automático ativado na primeira execução"),
                Err(e) => warn!(T, "não consegui ativar o início automático: {e}"),
            }
        }
    }
    tray::balloon(
        "gmk104-aura está rodando",
        "O ícone ao lado do relógio mostra a cor atual do teclado. Clique nele para ver o menu.",
    );
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            logger::init(true);
            logger::write("ERROR", T, &e);
            std::process::exit(2);
        }
    };
    logger::init(args.verbose);
    info!(T, "gmk104-aura {VERSION} iniciando");

    if args.no_tray {
        follower::run(&args.cfg);
        return;
    }
    if !single_instance() {
        info!(T, "já há uma instância em execução; saindo");
        tray::message("gmk104-aura", "O gmk104-aura já está em execução.\nProcure o ícone redondo ao lado do relógio.");
        return;
    }

    let cfg = args.cfg;
    let worker = std::thread::Builder::new()
        .name("seguidor".into())
        .spawn(move || follower::run(&cfg))
        .expect("thread seguidora");

    // o ícone precisa existir antes do balão da primeira execução, por isso a pergunta vai em outra thread
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(800));
        first_run_setup();
    });

    tray::run(); // bloqueia até "Sair"
    status::request_stop();
    let _ = worker.join();
    info!(T, "encerrado");
}
