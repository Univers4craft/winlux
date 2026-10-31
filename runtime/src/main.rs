#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod admin;
mod archive;
mod audio;
mod clr;
mod cpu;
mod crash;
mod diskimage;
mod dlls;
mod firewall;
mod gpu;
mod imaging;
mod loader;
pub mod log;
mod managed;
mod mounts;
mod msi;
mod netdrive;
mod overlay;
mod paths;
mod pe;
mod power;
mod printf;
mod process;
mod profiles;
pub mod quality;
mod tasks;
mod thunks;

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args_os();
    let program = args.next();
    // `winlux-drives …` (lien vers winlux) ou `winlux --drives …` : lecteurs réseau.
    let invoked_as = program
        .as_deref()
        .map(std::path::Path::new)
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "winlux-drives");
    if invoked_as {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(netdrive::main(&rest));
    }
    let Some(exe) = args.next() else {
        eprintln!("winlux: usage: winlux PROGRAM.exe [arguments...]");
        std::process::exit(2);
    };
    if exe == "--drives" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(netdrive::main(&rest));
    }
    if exe == "--version" {
        println!("winlux {}", include_str!("../../VERSION").trim());
        return;
    }
    if exe == "--power" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(power::cli::main(&rest));
    }
    if exe == "--audio-test" {
        std::process::exit(audio::self_test());
    }
    if exe == "--tasks" || exe == "--firewall" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(if exe == "--tasks" {
            tasks::cli(&rest)
        } else {
            firewall::cli(&rest)
        });
    }
    let root_arg = |a: Option<std::ffi::OsString>| a.map(PathBuf::from);
    if exe == "--cpu-report" {
        std::process::exit(cpu::report_command(root_arg(args.next()).as_deref()));
    }
    if exe == "--cpu-profile" {
        let profile = args.next().unwrap_or_default();
        std::process::exit(cpu::profile_command(
            &profile.to_string_lossy(),
            root_arg(args.next()).as_deref(),
        ));
    }
    if exe == "--fan-daemon" {
        let sys = root_arg(args.next())
            .map(cpu::Sys::at)
            .unwrap_or_else(cpu::Sys::host);
        std::process::exit(cpu::fan_daemon(&sys));
    }
    if exe == "--inspect" {
        let Some(path) = args.next() else {
            eprintln!("winlux: usage: winlux --inspect PROGRAM.exe");
            std::process::exit(2);
        };
        std::process::exit(inspect(&PathBuf::from(path)));
    }
    if exe == "--image-info" {
        let Some(path) = args.next() else {
            eprintln!("winlux: usage: winlux --image-info IMAGE");
            std::process::exit(2);
        };
        std::process::exit(imaging::cli_info(&PathBuf::from(path)));
    }
    if let Some(code) = gpu_command(&exe, &mut args) {
        std::process::exit(code);
    }
    let exe = PathBuf::from(exe);
    let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
    // msiexec.exe est fourni par Winlux lui-même (module msi).
    if exe.as_os_str() == "--msiexec" || (msi::is_msiexec(&exe) && !is_real_pe(&exe)) {
        std::process::exit(msi::cli(&rest));
    }
    let source = match std::fs::read(&exe) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("winlux: lecture impossible de {} : {error}", exe.display());
            std::process::exit(1);
        }
    };
    backup_registry();
    paths::install_windows_env();
    if let Some(name) = exe.file_name().and_then(|n| n.to_str()) {
        crash::set_exe_name(name);
    }
    if let Ok(full) = std::fs::canonicalize(&exe) {
        crash::set_exe_path(&full.to_string_lossy());
    }
    let cpu_settings = profiles::apply(&exe);
    if let Some(assembly) = managed::managed_image(&exe, &source) {
        log_info!(
            "managed",
            "assembly .NET détectée, exécution via le CLR Winlux intégré"
        );
        cpu::apply_launch(&cpu_settings, &[]);
        let code = clr::execute_assembly(&assembly);
        dlls::kernel32::flush_and_exit(code);
    }
    let image = match loader::load(&exe, true) {
        Ok(m) => m,
        Err(e) => {
            log_error!("loader", "chargement impossible : {e}");
            std::process::exit(1);
        }
    };
    if image.pe.is_dll() {
        log_error!(
            "loader",
            "{} est une DLL, pas un exécutable lançable directement",
            exe.display()
        );
        std::process::exit(1);
    }
    trace!(
        "sous-système PE : {}, DLL flags=0x{:04x}, stack reserve/commit={}/{}",
        image.pe.subsystem_name(),
        image.pe.dll_characteristics,
        image.pe.stack_reserve,
        image.pe.stack_commit
    );
    cpu::apply_launch(&cpu_settings, &image.import_names());
    dlls::kernel32::init();
    process::init_process(&image, &rest);
    process::run(image);
}

/// Commandes GPU pour `winlux-control` et `winlux-prime-run` :
/// `--gpu-status`, `--gpu-env auto|integrated|dedicated`,
/// `--gpu-set CARTE RÉGLAGE VALEUR`.
fn gpu_command(flag: &std::ffi::OsStr, args: &mut std::env::ArgsOs) -> Option<i32> {
    let root = gpu::system_root();
    match flag.to_str()? {
        "--gpu-status" => {
            print!("{}", gpu::report(&gpu::probe(&root)));
            Some(0)
        }
        "--gpu-env" => {
            let wanted = args.next().map(|a| a.to_string_lossy().into_owned());
            let Some(pref) = wanted.as_deref().and_then(gpu::Preference::parse) else {
                eprintln!("winlux: usage: winlux --gpu-env auto|integrated|dedicated");
                return Some(2);
            };
            for (key, value) in gpu::prime_env(&gpu::probe(&root), pref) {
                println!("{key}={value}");
            }
            Some(0)
        }
        "--gpu-set" => {
            let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
            let [card, setting, value] = rest.as_slice() else {
                eprintln!("winlux: usage: winlux --gpu-set CARTE RÉGLAGE VALEUR");
                return Some(2);
            };
            match gpu::apply(&root, card, setting, value) {
                Ok(message) => {
                    println!("{message}");
                    Some(0)
                }
                Err(error) => {
                    eprintln!("{error}");
                    Some(1)
                }
            }
        }
        _ => None,
    }
}

/// Vrai si le fichier existe et commence par la signature MZ d'un PE.
fn is_real_pe(path: &std::path::Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 2];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == *b"MZ"
}

fn backup_registry() {
    let Some(data) = paths::data_dir() else {
        return;
    };
    let src = data.join("registry.tsv");
    if let Ok(meta) = std::fs::metadata(&src) {
        if meta.len() > 0 {
            let dst = data.join("registry.tsv.bak");
            if let Err(e) = std::fs::copy(&src, &dst) {
                log_warn!("registry", "sauvegarde impossible de registry.tsv : {e}");
            }
        }
    }
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
    if managed::managed_image(exe, &source).is_none() {
        match pe::PeImage::parse(&source) {
            Ok(image) if image.is_dll() => {
                eprintln!(
                    "{} est une DLL, pas un programme lançable directement",
                    exe.display()
                );
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
