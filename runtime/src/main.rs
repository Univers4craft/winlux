#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod abslots;
mod admin;
mod anticheat;
mod archive;
mod audio;
mod bootsplash;
mod boottime;
mod clipbridge;
mod clr;
mod cpu;
mod crash;
mod diskimage;
mod display;
mod dlls;
mod firewall;
mod gamemode;
mod gpu;
mod image_cache;
mod image_lock;
mod imaging;
mod intl;
mod journal;
mod launch;
mod ldr;
mod loader;
pub mod log;
mod managed;
mod mounts;
mod msi;
mod netdrive;
mod netports;
mod overlay;
mod paths;
mod pe;
mod permissions;
mod playtime;
mod power;
mod printf;
mod process;
mod profiles;
mod proxy;
pub mod quality;
mod registry;
mod sandbox_fallback;
mod setup;
mod tasks;
mod thunks;
mod uac;
mod volumes;
mod wow32;

use std::path::PathBuf;

/// Ligne de `winlux --version` : version et commit de construction (#1266),
/// par exemple « winlux v0.2.20 (commit 2e22875042ab) ».
fn version_line() -> String {
    format!(
        "winlux {} (commit {})",
        include_str!("../../VERSION").trim(),
        env!("WINLUX_BUILD_COMMIT")
    )
}

#[cfg(test)]
mod version_tests {
    #[test]
    fn version_affiche_le_commit_de_construction() {
        let line = super::version_line();
        let commit = env!("WINLUX_BUILD_COMMIT");
        assert!(line.starts_with("winlux v"), "{line}");
        assert!(line.ends_with(&format!("(commit {commit})")), "{line}");
        assert!(!commit.is_empty());
        // Construit dans le dépôt git : un vrai commit, pas « inconnu ».
        if std::env::var_os("WINLUX_COMMIT").is_none() {
            let head = commit.trim_end_matches("+modifié");
            assert!(
                head.len() >= 7 && head.bytes().all(|b| b.is_ascii_hexdigit()),
                "{commit}"
            );
        }
    }
}

fn main() {
    // Journal du compte (#633), fixé tant qu'il n'y a qu'un fil.
    journal::init();
    // Code de sortie Windows complet pour winlux-open (#1205).
    dlls::kernel32::exit_code_file_init();
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
    if exe == "--exports" {
        // Une ligne « dll<TAB>fonction » par export intégré : la base des
        // mesures de compatibilité (docs/COMPATIBILITE.md).
        for (dll, names) in dlls::builtin_exports() {
            for name in names {
                println!("{dll}\t{name}");
            }
        }
        // Noms x86 servis seulement par les passerelles wow32 (#1508).
        for (dll, name) in wow32::x86_only_candidates() {
            println!("x86:{dll}\t{name}");
        }
        return;
    }
    if exe == "--exports-kernel32" {
        // Ce que GetProcAddress(kernel32, …) résout : la surface figée par
        // scripts/test-kernel32-exports.py (#521).
        for name in dlls::kernel32_surface() {
            println!("{name}");
        }
        return;
    }
    if exe == "--linux-path" {
        // Chemin Windows → chemin Linux, avec les mêmes règles que les .exe
        // (lecteurs, profil, dossiers connus) : Paramètres > « Réglages de
        // l'exécutable » ouvert depuis l'Explorateur (#739).
        let Some(win) = args.next() else {
            std::process::exit(2);
        };
        let linux = paths::to_linux(&win.to_string_lossy());
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = out.write_all(linux.as_os_str().as_encoded_bytes());
        let _ = out.write_all(b"\n");
        return;
    }
    if exe == "--version" {
        // Commit compris (#1266) : deux têtes de master se distinguent.
        println!("{}", version_line());
        return;
    }
    if exe == "--power" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(power::cli::main(&rest));
    }
    if exe == "--intl" {
        std::process::exit(intl::report());
    }
    if exe == "--services" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(dlls::advapi32_services::cli(&rest));
    }
    if exe == "--text" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(intl::text::cli(&rest));
    }
    if exe == "--boot-time" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(boottime::cli(&rest));
    }
    if exe == "--ab-boot" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(abslots::cli(&rest));
    }
    if exe == "--boot-splash" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(bootsplash::cli(&rest));
    }
    if exe == "--gamemode" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(gamemode::cli(&rest));
    }
    if exe == "--clipboard-bridge" {
        std::process::exit(clipbridge::run());
    }
    if exe == "--display-restore" {
        std::process::exit(dlls::displaymode::restore_guard());
    }
    if exe == "--display" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(display::main(&rest));
    }
    if exe == "--audio-test" {
        std::process::exit(audio::self_test());
    }
    if exe == "--volumes" {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(volumes::cli(&rest));
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
        std::process::exit(inspect(&paths::launch_path(&path)));
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
    let exe = paths::launch_path(exe.as_ref());
    let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
    // msiexec.exe est fourni par Winlux lui-même (module msi).
    if exe.as_os_str() == "--msiexec" || (msi::is_msiexec(&exe) && !is_real_pe(&exe)) {
        std::process::exit(msi::cli(&rest));
    }
    // reg.exe et regedit.exe sont fournis par Winlux (module registry).
    if exe.as_os_str() == "--reg" || (registry::cli::is_reg(&exe) && !is_real_pe(&exe)) {
        std::process::exit(registry::cli::reg_main(&rest));
    }
    if exe.as_os_str() == "--regedit" || (registry::cli::is_regedit(&exe) && !is_real_pe(&exe)) {
        std::process::exit(registry::cli::regedit_main(&rest));
    }
    // Installateurs NSIS / Inno Setup et leurs désinstalleurs : pris en
    // charge par Winlux lui-même (module setup), sans exécuter leur code.
    if exe.as_os_str() == "--msi-dll-action" {
        msi_dll_action(&rest);
    }
    if exe.as_os_str() == "--msi-script-action" {
        msi_script_action(&rest);
    }
    if exe.as_os_str() == "--setup" {
        std::process::exit(setup::cli(&rest));
    }
    // Anti-triche noyau embarqué : refusé avant toute écriture (#1720).
    if let Some(code) = anticheat::check_launch(&exe) {
        std::process::exit(code);
    }
    // Programme filtré en sortie par le pare-feu : aucun de ses octets, ni
    // ceux de ses DLL, ne part avant que le service l'ait placé dans le
    // cgroup de sa règle (#2157).
    firewall::wait_for_cgroup();
    if std::env::var_os("WINLUX_SETUP_NATIVE").is_none() {
        if let Some(code) = setup::launch(&exe, &rest) {
            std::process::exit(code);
        }
    }
    // Variables d'environnement Windows écrites par les installateurs.
    msi::extra::apply_registry_environment();
    // Projeté plutôt que lu : rien n'est copié en mémoire (#997).
    let source = match loader::ImageFile::open(&exe) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("winlux: lecture impossible de {} : {error}", exe.display());
            std::process::exit(1);
        }
    };
    // Programme x86 : relancé tout de suite si la mémoire haute n'a pas
    // été réservée avant le premier malloc (#232).
    let x86 = pe::PeImage::parse(&source).is_ok_and(|pe| pe.x86);
    if x86 {
        wow32::reexec_if_needed();
    }
    registry::backup();
    paths::install_windows_env();
    // PROCESSOR_ARCHITECTURE, ProgramFiles… d'un processus 32 ou 64 bits.
    wow32::apply_env(x86);
    if let Some(name) = exe.file_name().and_then(|n| n.to_str()) {
        crash::set_exe_name(name);
    }
    if let Ok(full) = std::fs::canonicalize(&exe) {
        crash::set_exe_path(&full.to_string_lossy());
    }
    let cpu_settings = profiles::apply(&exe);
    if let Some(assembly) = managed::managed_image(&exe, &source) {
        if managed::is_squirrel_update(&assembly) {
            log_info!(
                "managed",
                "Update.exe de Squirrel : servi par winlux-squirrel-update (#1190)"
            );
            let err = managed::exec_squirrel_update(&exe);
            log_error!("managed", "winlux-squirrel-update introuvable : {err}");
            std::process::exit(1);
        }
        log_info!(
            "managed",
            "assembly .NET détectée, exécution via le CLR Winlux intégré"
        );
        cpu::apply_launch(&cpu_settings, &[]);
        let exe_path = std::fs::canonicalize(&exe).ok();
        let app_dir = exe_path
            .as_deref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        // Les DLL intégrées appelées par P/Invoke lisent leur état de fil
        // (dernière erreur Win32…) dans le TEB, comme pour un .exe natif.
        if process::teb() == 0 {
            process::init_thread();
        }
        let code = clr::execute_assembly_at(&assembly, exe_path.as_deref(), app_dir.as_deref());
        dlls::kernel32::flush_and_exit(code);
    }
    // Programme x86 32 bits : mémoire sous 4 Go et passerelles (#232),
    // avant tout chargement.
    if let Ok(pe) = pe::PeImage::parse(&source) {
        if pe.x86 {
            wow32::init(pe.characteristics & 0x0020 != 0);
        }
    }
    // La copie lue du fichier ne sert plus : sans ce drop, elle restait en
    // mémoire toute la vie du programme (300 Mo pour claude.exe, #997).
    drop(source);
    loader::take_inherited_base();
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
    uac::check_launch(image.base, &exe);
    let imports = image.import_names();
    cpu::apply_launch(&cpu_settings, &imports);
    if dlls::gui_x11::renders_itself(&imports)
        || dlls::gui_x11::renderer_in_data_sections(image.bytes(), &image.pe.sections)
    {
        dlls::gui_x11::RENDERS_ITSELF.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if cpu::is_heavy(&imports) {
        // Jeu ou application 3D : le mode jeu lui donne la priorité.
        let name = exe.file_name().map(|n| n.to_string_lossy().into_owned());
        gamemode::register_current(name.as_deref().unwrap_or("jeu"));
        // Temps de jeu affiché par l'application Jeux.
        playtime::start(&exe);
    } else if std::env::var_os("WINLUX_GAME").is_some_and(|v| v == "1") {
        // Lancé depuis l'application Jeux : un jeu même sans import
        // graphique visible (Unity, Direct3D chargé à la demande).
        playtime::start(&exe);
    }
    dlls::kernel32::init();
    // Fichiers « effacés à la fermeture » d'un processus tué (#2040).
    dlls::doomed::sweep();
    // WINLUX_THREAD_DUMP (#2147) : rien sans la variable.
    dlls::thread_ctl::install_dump();
    dlls::inherit::adopt();
    dlls::handle_broker::adopt();
    process::init_process(&image, &launch::windows_args(&rest));
    process::stop_if_suspended();
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

/// `winlux --msi-dll-action DLL ENTREE` : action personnalisée DLL d'un
/// paquet MSI (#833), lancée par le msiexec intégré dans ce processus à
/// part. Propriétés de la session : fichier `WINLUX_MSI_SESSION`.
fn msi_dll_action(args: &[String]) -> ! {
    let (Some(dll), Some(entry)) = (args.first(), args.get(1)) else {
        eprintln!("usage : winlux --msi-dll-action DLL ENTREE");
        std::process::exit(1639);
    };
    let path = std::path::PathBuf::from(dll);
    // Dans la trace, quelle action tourne dans ce processus (#1639).
    crate::trace!("action personnalisée DLL : {entry} dans {dll}");
    let source = std::fs::read(&path).unwrap_or_default();
    let x86 = pe::PeImage::parse(&source).is_ok_and(|pe| pe.x86);
    if x86 {
        wow32::reexec_if_needed();
    }
    paths::install_windows_env();
    wow32::apply_env(x86);
    if let Ok(pe) = pe::PeImage::parse(&source) {
        if pe.x86 {
            wow32::init(pe.characteristics & 0x0020 != 0);
        }
    }
    let image = match loader::load(&path, true) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("DLL {dll} non chargée : {e}");
            std::process::exit(1603);
        }
    };
    let Some(export) = loader::export_by_name(&image, entry) else {
        eprintln!("point d'entrée {entry} absent de {dll}");
        std::process::exit(1603);
    };
    dlls::kernel32::init();
    process::init_process(&image, &[]);
    process::run_dll_export(image, export, msi::session::SESSION_HANDLE)
}

/// `winlux --msi-script-action LANGAGE FICHIER [FONCTION]` : action
/// personnalisée VBScript ou JScript d'un paquet MSI (#973), avec l'objet
/// Session ; le code Windows Installer revient par le fichier de session.
fn msi_script_action(args: &[String]) -> ! {
    let (Some(lang), Some(file)) = (args.first(), args.get(1)) else {
        eprintln!("usage : winlux --msi-script-action JScript|VBScript FICHIER [FONCTION]");
        std::process::exit(1639);
    };
    paths::install_windows_env();
    let function = args.get(2).map(String::as_str).unwrap_or("");
    let code = msi::script::msi_action(lang, file, function);
    msi::session::save(code);
    std::process::exit(code as i32)
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

/// Indique comment lancer un fichier sans l'exécuter : `console` ou `gui` sur
/// la sortie standard, ou la raison du refus sur la sortie d'erreur. Utilisé
/// par `winlux-open` pour décider d'ouvrir un terminal lors d'un double-clic.
/// Les installateurs que Winlux déroule lui-même (WiX Burn, NSIS, Inno Setup)
/// donnent `setup` et leurs désinstalleurs connus `uninstall` : souvent en
/// 32 bits, ils s'installent quand même puisque leur code n'est pas exécuté.
fn inspect(exe: &std::path::Path) -> i32 {
    let source = match std::fs::read(exe) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("lecture impossible de {} : {error}", exe.display());
            return 1;
        }
    };
    if setup::Manifest::of_uninstaller(exe).is_some() {
        println!("uninstall");
        return 0;
    }
    if setup::burn::find_section(&source).is_some() || setup::detect(&source).is_some() {
        println!("setup");
        return 0;
    }
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
