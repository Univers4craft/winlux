mod crash;
mod loader;
mod managed;
mod paths;
mod pe;
mod printf;
mod process;
mod thunks;
mod dlls;

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        if std::env::var_os("WINLUX_TRACE").is_some() {
            eprintln!("winlux: {}", format_args!($($arg)*));
        }
    };
}

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args_os();
    let _program = args.next();
    let Some(exe) = args.next() else {
        eprintln!("winlux: usage: winlux PROGRAM.exe [arguments...]");
        std::process::exit(2);
    };
    if exe == "--version" {
        println!("winlux {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if exe == "--inspect" {
        let Some(path) = args.next() else {
            eprintln!("winlux: usage: winlux --inspect PROGRAM.exe");
            std::process::exit(2);
        };
        std::process::exit(inspect(&PathBuf::from(path)));
    }
    let exe = PathBuf::from(exe);
    let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
    let source = match std::fs::read(&exe) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("winlux: lecture impossible de {} : {error}", exe.display());
            std::process::exit(1);
        }
    };
    if let Some(target) = managed::detect(&exe, &source) {
        eprintln!("winlux: {} détectée, utilisation du pont natif", managed::description(&target));
        match managed::run(&target, &rest) {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(error) => {
                eprintln!("winlux: {error}");
                eprintln!("winlux: les interfaces WPF/WinForms restent limitées tant que leurs API Windows ne sont pas complètes");
                std::process::exit(1);
            }
        }
    }
    let image = match loader::load(&exe, true) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("winlux: chargement impossible : {e}");
            std::process::exit(1);
        }
    };
    if image.pe.is_dll() {
        eprintln!("winlux: {} est une DLL, pas un exécutable lançable directement", exe.display());
        std::process::exit(1);
    }
    trace!("sous-système PE : {}, DLL flags=0x{:04x}, stack reserve/commit={}/{}",
        image.pe.subsystem_name(), image.pe.dll_characteristics, image.pe.stack_reserve, image.pe.stack_commit);
    dlls::kernel32::init();
    process::init_process(&image, &rest);
    process::run(image);
}

/// Indique comment lancer un fichier sans l'exécuter : `console` ou `gui` sur
/// la sortie standard, ou la raison du refus sur la sortie d'erreur. Utilisé
/// par `winlux-open` pour décider d'ouvrir un terminal lors d'un double-clic.
fn inspect(exe: &std::path::Path) -> i32 {
    let source = match std::fs::read(exe) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("lecture impossible de {} : {error}", exe.display());
            return 1;
        }
    };
    if managed::detect(exe, &source).is_none() {
        match pe::PeImage::parse(&source) {
            Ok(image) if image.is_dll() => {
                eprintln!("{} est une DLL, pas un programme lançable directement", exe.display());
                return 1;
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        }
    }
    match pe::peek_subsystem(&source) {
        Some(2) => println!("gui"),
        Some(3) => println!("console"),
        Some(other) => {
            eprintln!("sous-système Windows {other} non pris en charge (pilote, EFI ou application native)");
            return 1;
        }
        None => {
            eprintln!("en-tête PE invalide : sous-système illisible");
            return 1;
        }
    }
    0
}
