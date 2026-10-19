mod crash;
mod loader;
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
    let exe = PathBuf::from(exe);
    let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
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
