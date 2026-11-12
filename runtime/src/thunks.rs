//! Petits morceaux de code machine générés à l'exécution, et ponts pour
//! les fonctions à nombre variable d'arguments (printf…).

use std::io::Write;
use std::sync::Mutex;

/// Informations attachées à chaque fonction manquante.
#[repr(C)]
struct Missing {
    label: String,
}

struct StubPool {
    cur: usize,
    end: usize,
}

static POOL: Mutex<StubPool> = Mutex::new(StubPool { cur: 0, end: 0 });
static LOGGED: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

/// Écrit une ligne dans le journal des fonctions manquantes (une seule fois par fonction).
pub fn log_missing(label: &str, called: bool) {
    let key = format!("{label}{}", if called { "!" } else { "" });
    {
        let mut l = LOGGED.lock().unwrap();
        let set = l.get_or_insert_with(std::collections::HashSet::new);
        if !set.insert(key) {
            return;
        }
    }
    let exe = crate::loader::main_module()
        .map(|m| m.name)
        .unwrap_or_default();
    if let Some(dir) = crate::paths::data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("manquants.log"))
        {
            let _ = writeln!(
                f,
                "{}\t{}\t{}",
                exe,
                label,
                if called { "appelée" } else { "importée" }
            );
        }
    }
}

extern "win64" fn missing_called(info: *const Missing) -> u64 {
    let label = unsafe { &(*info).label };
    log_missing(label, true);
    if std::env::var("WINLUX_MISSING").as_deref() == Ok("ignore") {
        eprintln!("winlux: {label} non implémentée, appel ignoré (renvoie 0)");
        return 0;
    }
    eprintln!(
        "\nwinlux: le programme a appelé {label}, qui n'est pas encore implémentée dans Winlux."
    );
    eprintln!(
        "winlux: c'est noté dans le journal des fonctions manquantes ({}).",
        crate::paths::data_dir()
            .map(|d| d.join("manquants.log").display().to_string())
            .unwrap_or_default()
    );
    eprintln!("winlux: (WINLUX_MISSING=ignore permet de continuer en renvoyant 0, à vos risques)");
    crate::crash::record_failure("ENTRYPOINT_NOT_FOUND", &format!("fonction={label}"));
    crate::dlls::kernel32::flush_and_exit(0xC000_0139u32 as i32)
}

/// Fabrique une fonction de remplacement pour un import absent :
///   mov rcx, <infos> ; mov rax, <missing_called> ; jmp rax
pub fn missing_stub(dll: &str, func: &str) -> usize {
    let label = format!("{}!{}", dll.to_ascii_lowercase(), func);
    log_missing(&label, false);
    crate::trace!("import manquant : {label}");
    let info = Box::into_raw(Box::new(Missing { label })) as u64;
    let mut code = Vec::with_capacity(32);
    code.extend_from_slice(&[0x48, 0xB9]);
    code.extend_from_slice(&info.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&(missing_called as *const () as usize as u64).to_le_bytes());
    code.extend_from_slice(&[0xFF, 0xE0]);
    alloc_code(&code)
}

/// Diagnostic `WINLUX_GETPROC_STUBS=1` : GetProcAddress rend, pour une
/// fonction absente d'une DLL intégrée, le même bouchon journalisé qu'un
/// import statique absent au lieu de NULL. Un programme empaqueté qui
/// résout ses imports à la main et s'arrête au premier NULL (installeur
/// Creative Cloud, #1205, #1226) liste ainsi toutes ses fonctions
/// manquantes en une passe ; il ne s'arrête qu'en appelant l'une d'elles.
/// Désactivé par défaut : sans la variable, GetProcAddress rend NULL.
pub fn getproc_stub(dll: &str, func: &str) -> Option<usize> {
    if std::env::var("WINLUX_GETPROC_STUBS").as_deref() != Ok("1") {
        return None;
    }
    static STUBS: Mutex<Option<std::collections::HashMap<String, usize>>> = Mutex::new(None);
    let label = format!("{}!{}", dll.to_ascii_lowercase(), func);
    let mut m = STUBS.lock().unwrap_or_else(|e| e.into_inner());
    let m = m.get_or_insert_with(std::collections::HashMap::new);
    if let Some(&s) = m.get(&label) {
        return Some(s);
    }
    eprintln!("winlux: GetProcAddress {label} absente : bouchon rendu (WINLUX_GETPROC_STUBS=1)");
    let stub = missing_stub(dll, func);
    m.insert(label, stub);
    Some(stub)
}

struct Traced {
    label: String,
}

fn api_trace_filter() -> Option<&'static [String]> {
    static F: std::sync::OnceLock<Option<Vec<String>>> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let raw = std::env::var("WINLUX_APITRACE").ok()?;
        if raw.is_empty() || raw == "0" {
            return None;
        }
        Some(
            raw.split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty() && s != "1")
                .collect(),
        )
    })
    .as_deref()
}

/// « module+rva » pour une adresse de code invité, ou l'adresse brute.
pub fn describe_site(addr: usize) -> String {
    crate::loader::MODULES
        .try_lock()
        .ok()
        .and_then(|m| {
            m.iter()
                .find(|m| m.contains(addr))
                .map(|m| format!("{}+{:#x}", m.name, addr - m.base))
        })
        .unwrap_or_else(|| format!("{addr:#x}"))
}

/// Secondes depuis le premier appel, pour situer les lignes du journal
/// d'API les unes par rapport aux autres entre fils (#2161).
pub fn api_clock() -> f64 {
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    T0.get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}

extern "win64" fn traced_called(info: *const Traced, ret: usize) {
    let label = unsafe { &(*info).label };
    let site = describe_site(ret);
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    api_log(&format!(
        "winlux[api] {tid} {label} <- {site} @{:.6}",
        api_clock()
    ));
}

/// Plafond d'un fichier de journal d'API ; au-delà, il devient `.1` (le
/// précédent `.1` est effacé), si bien qu'un processus garde au plus deux
/// fois cette taille, et toujours ses dernières lignes.
const API_LOG_MAX: u64 = 32 << 20;
/// Plafond du dossier des journaux : les plus anciens sont effacés.
const API_LOG_DIR_MAX: u64 = 512 << 20;

/// Dossier des journaux d'API : `$WINLUX_APITRACE_DIR`, sinon
/// `~/.cache/winlux/apitrace`. Jamais /tmp, un tmpfs en mémoire qu'une
/// trace de quelques minutes remplit (le bureau ne s'en remettait pas).
fn api_log_dir() -> Option<std::path::PathBuf> {
    if let Some(d) = std::env::var_os("WINLUX_APITRACE_DIR").filter(|d| !d.is_empty()) {
        return Some(d.into());
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".cache")))?;
    Some(cache.join("winlux").join("apitrace"))
}

/// Efface les journaux les plus anciens tant que le dossier dépasse `max`.
fn prune_api_logs(dir: &std::path::Path, max: u64) {
    let mut files: Vec<(std::time::SystemTime, u64, std::path::PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok().filter(|m| m.is_file())?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= max {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

struct ApiLog {
    path: std::path::PathBuf,
    file: std::fs::File,
    written: u64,
}

/// Nom du programme Windows pour le fichier de journal (« claude »), et non
/// celui de l'exécutable Winlux qui le porte.
fn api_log_program() -> String {
    let stem = |n: &str| {
        let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
        let lower = base.to_ascii_lowercase();
        lower
            .strip_suffix(".exe")
            .map(|_| base[..base.len() - 4].to_string())
    };
    let from_modules = crate::loader::MODULES
        .try_lock()
        .ok()
        .and_then(|m| m.iter().find_map(|m| stem(&m.name)));
    from_modules
        .or_else(|| std::env::args().skip(1).find_map(|a| stem(&a)))
        .filter(|n| !n.is_empty())
        .map(|n| {
            n.chars()
                .map(|c| {
                    if c.is_alphanumeric() || "-_.".contains(c) {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| "winlux".into())
}

fn open_api_log() -> Option<ApiLog> {
    let dir = api_log_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    prune_api_logs(&dir, API_LOG_DIR_MAX);
    let exe = api_log_program();
    let path = dir.join(format!("{exe}-{}.log", std::process::id()));
    let file = std::fs::File::create(&path).ok()?;
    eprintln!("winlux[api] journal des appels : {}", path.display());
    Some(ApiLog {
        path,
        file,
        written: 0,
    })
}

/// Note de diagnostic dans le journal d'API, seulement quand
/// `WINLUX_APITRACE` est actif : exceptions, texte d'OutputDebugString,
/// arrêt d'un autre processus. Utile pour les enfants de Chromium dont
/// stderr ne mène nulle part.
pub fn api_trace_active() -> bool {
    api_trace_filter().is_some()
}

pub fn api_note(line: &str) {
    if api_trace_filter().is_some() {
        api_log(line);
    }
}

/// Écrit une ligne dans le journal d'API du processus (stderr si le
/// fichier ne peut pas être créé).
fn api_log(line: &str) {
    static LOG: Mutex<Option<Option<ApiLog>>> = Mutex::new(None);
    // Pas d'arrêt par SuspendThread en tenant le journal (#2065).
    let _hold = crate::dlls::thread_ctl::no_park();
    let mut g = LOG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let slot = g.get_or_insert_with(open_api_log);
    let Some(log) = slot.as_mut() else {
        eprintln!("{line}");
        return;
    };
    if log.written >= API_LOG_MAX {
        let mut old = log.path.clone().into_os_string();
        old.push(".1");
        let _ = std::fs::rename(&log.path, &old);
        match std::fs::File::create(&log.path) {
            Ok(f) => {
                log.file = f;
                log.written = 0;
            }
            Err(_) => {
                *slot = None;
                eprintln!("{line}");
                return;
            }
        }
    }
    if writeln!(log.file, "{line}").is_ok() {
        log.written += line.len() as u64 + 1;
    }
}

/// Avec `WINLUX_APITRACE=1` (ou une liste de fragments de noms séparés par
/// des virgules), chaque appel d'une fonction intégrée importée est
/// journalisé avec l'adresse de l'appelant (voir [`api_log_dir`]), puis transmis tel
/// quel. Outil de diagnostic pour les programmes qui se bloquent.
pub fn api_trace(dll: &str, func: &str, target: usize) -> usize {
    // Le pont de remplissage de pile (#2120) passe sous la trace, qui
    // garde ainsi l'adresse de l'appelant.
    let target = stack_scrub(dll, func, target);
    let Some(filter) = api_trace_filter() else {
        return target;
    };
    let label = format!("{}!{}", dll.to_ascii_lowercase(), func);
    let lower = label.to_ascii_lowercase();
    if !filter.is_empty() && !filter.iter().any(|f| lower.contains(f.as_str())) {
        return target;
    }
    // Les exportations de données (msvcrt!_fmode, __initenv, _acmdln…) ne
    // sont pas des fonctions : les envelopper dans un pont de code ferait
    // lire du code machine au programme à la place de la variable.
    if !is_executable(target) {
        return target;
    }
    static CACHE: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    if let Some(&(_, t)) = CACHE.lock().unwrap().iter().find(|(a, _)| *a == target) {
        return t;
    }
    let info = Box::into_raw(Box::new(Traced { label })) as u64;
    let mut c: Vec<u8> = Vec::with_capacity(128);
    // push rcx; push rdx; push r8; push r9; sub rsp, 0x68
    c.extend_from_slice(&[0x51, 0x52, 0x41, 0x50, 0x41, 0x51, 0x48, 0x83, 0xEC, 0x68]);
    // movdqu [rsp+0x20+16*i], xmm0..3
    for (i, disp) in [0x20u8, 0x30, 0x40, 0x50].iter().enumerate() {
        c.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x44 | ((i as u8) << 3), 0x24, *disp]);
    }
    // mov rcx, info ; mov rdx, [rsp+0x88] ; mov rax, traced_called ; call rax
    c.extend_from_slice(&[0x48, 0xB9]);
    c.extend_from_slice(&info.to_le_bytes());
    c.extend_from_slice(&[0x48, 0x8B, 0x94, 0x24, 0x88, 0, 0, 0]);
    c.extend_from_slice(&[0x48, 0xB8]);
    c.extend_from_slice(&(traced_called as *const () as usize as u64).to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]);
    for (i, disp) in [0x20u8, 0x30, 0x40, 0x50].iter().enumerate() {
        c.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x44 | ((i as u8) << 3), 0x24, *disp]);
    }
    // add rsp, 0x68; pop r9; pop r8; pop rdx; pop rcx; mov rax, target; jmp rax
    c.extend_from_slice(&[0x48, 0x83, 0xC4, 0x68, 0x41, 0x59, 0x41, 0x58, 0x5A, 0x59]);
    c.extend_from_slice(&[0x48, 0xB8]);
    c.extend_from_slice(&(target as u64).to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xE0]);
    let t = alloc_code(&c);
    CACHE.lock().unwrap().push((target, t));
    t
}

/// Motif de `WINLUX_STACK_SCRUB` : quadmot non canonique, si bien qu'un
/// pointeur lu dans ces restes fait une faute de protection générale dès
/// qu'il est suivi, et se reconnaît tel quel dans les registres de
/// crash.log (#2120).
pub const STACK_SCRUB_PATTERN: u64 = 0xF1F1_5C2B_F1F1_5C2B;
/// Pile morte remplie sous le rsp de l'appelant à chaque retour.
const STACK_SCRUB_BYTES: usize = 16 << 10;
/// Octets laissés intacts juste sous le rsp de l'appelant. Windows x64 n'a
/// pas de zone rouge (tout ce qui est sous rsp est volatil) ; la marge
/// n'est qu'une précaution.
const STACK_SCRUB_MARGIN: usize = 32;

/// Fonctions jamais enveloppées : leur cadre, leur retour ou leur pile ne
/// doivent pas changer (setjmp revient deux fois, les exceptions déroulent
/// les cadres, `__chkstk` a une convention d'appel à part…).
const STACK_SCRUB_EXCLUDED: &[&str] = &[
    "setjmp",
    "longjmp",
    "raiseexception",
    "cxxthrowexception",
    "rtlunwind",
    "restorecontext",
    "ntcontinue",
    "capturecontext",
    "capturestackbacktrace",
    "virtualunwind",
    "chkstk",
    "alloca_probe",
    "specific_handler",
    "framehandler",
    "gshandler",
    "fiber",
    "exitthread",
    "exitprocess",
    "terminatethread",
    "terminateprocess",
    "_exit",
    "abort",
];

fn stack_scrub_filter() -> Option<&'static [String]> {
    static F: std::sync::OnceLock<Option<Vec<String>>> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let raw = std::env::var("WINLUX_STACK_SCRUB").ok()?;
        if raw.is_empty() || raw == "0" {
            return None;
        }
        eprintln!(
            "winlux: WINLUX_STACK_SCRUB actif, pile morte remplie de {STACK_SCRUB_PATTERN:#018x} au retour des API (#2120)"
        );
        Some(
            raw.split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty() && s != "1")
                .collect(),
        )
    })
    .as_deref()
}

/// Vrai si `label` (« dll!fonction ») doit passer par le pont de
/// remplissage pour ce filtre.
fn stack_scrub_wanted(filter: &[String], label: &str) -> bool {
    let lower = label.to_ascii_lowercase();
    let func = lower.rsplit('!').next().unwrap_or(&lower);
    if STACK_SCRUB_EXCLUDED.iter().any(|x| func.contains(x)) {
        return false;
    }
    filter.is_empty() || filter.iter().any(|f| lower.contains(f.as_str()))
}

/// Bornes [début, fin) de la projection lisible et inscriptible qui
/// contient `addr`, d'après /proc/self/maps.
fn rw_mapping_containing(maps: &str, addr: usize) -> Option<(usize, usize)> {
    maps.lines().find_map(|l| {
        let mut f = l.split_whitespace();
        let (lo, hi) = f.next()?.split_once('-')?;
        let perms = f.next()?;
        let lo = usize::from_str_radix(lo, 16).ok()?;
        let hi = usize::from_str_radix(hi, 16).ok()?;
        (perms.starts_with("rw") && addr >= lo && addr < hi).then_some((lo, hi))
    })
}

/// Début du remplissage pour un appelant dont le rsp, au retour, vaut
/// `caller_rsp`. La zone rendue, [début, caller_rsp - marge), reste dans la
/// projection de la pile du thread ; début = fin quand il n'y a rien à
/// remplir sans risque. Appelé depuis le pont, sur la pile même qui sera
/// remplie : son propre cadre est mort dès son retour.
extern "win64" fn stack_scrub_start(caller_rsp: usize) -> usize {
    use std::cell::Cell;
    thread_local!(static STACK: Cell<(usize, usize)> = const { Cell::new((0, 0)) });
    let Some(top) = caller_rsp.checked_sub(STACK_SCRUB_MARGIN) else {
        return caller_rsp;
    };
    if !caller_rsp.is_multiple_of(8) {
        return top;
    }
    let inside = |(lo, hi): (usize, usize)| top > lo && caller_rsp <= hi;
    let mut range = STACK.with(Cell::get);
    if !inside(range) {
        let Some(r) = std::fs::read_to_string("/proc/self/maps")
            .ok()
            .and_then(|m| rw_mapping_containing(&m, top))
        else {
            return top;
        };
        range = r;
        STACK.with(|s| s.set(r));
        if !inside(range) {
            return top;
        }
    }
    stack_scrub_bounds(top, range.0)
}

/// Début aligné du remplissage sous `top`, jamais sous `lo`.
fn stack_scrub_bounds(top: usize, lo: usize) -> usize {
    let want = top.saturating_sub(STACK_SCRUB_BYTES).max(lo);
    // Multiple de 8 sous `top` : rep stosq remplit exactement [début, top).
    top - ((top - want) & !7)
}

/// Pont qui appelle `target` (jusqu'à 20 arguments win64, rax et xmm0
/// rendus intacts, rdi préservé), puis, une fois son propre cadre défait,
/// remplit la pile morte sous le rsp de l'appelant du motif
/// [`STACK_SCRUB_PATTERN`] avant de revenir à l'appelant.
fn stack_scrub_thunk(target: usize) -> usize {
    let mut c: Vec<u8> = Vec::with_capacity(320);
    // push rbp; mov rbp, rsp; sub rsp, 0xE0
    c.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x81, 0xEC, 0xE0, 0, 0, 0]);
    // Arguments 5 à 20 recopiés : mov rax, [rbp+0x30+8i]; mov [rsp+0x20+8i], rax
    for i in 0..16u32 {
        c.extend_from_slice(&[0x48, 0x8B, 0x85]);
        c.extend_from_slice(&(0x30 + 8 * i).to_le_bytes());
        c.extend_from_slice(&[0x48, 0x89, 0x84, 0x24]);
        c.extend_from_slice(&(0x20 + 8 * i).to_le_bytes());
    }
    // mov rax, target; call rax
    c.extend_from_slice(&[0x48, 0xB8]);
    c.extend_from_slice(&(target as u64).to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]);
    // mov [rsp+0xC0], rax; movdqu [rsp+0xD0], xmm0
    c.extend_from_slice(&[0x48, 0x89, 0x84, 0x24, 0xC0, 0, 0, 0]);
    c.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x84, 0x24, 0xD0, 0, 0, 0]);
    // lea rcx, [rbp+0x10]; mov rax, stack_scrub_start; call rax
    c.extend_from_slice(&[0x48, 0x8D, 0x4D, 0x10]);
    c.extend_from_slice(&[0x48, 0xB8]);
    c.extend_from_slice(&(stack_scrub_start as *const () as usize as u64).to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]);
    // mov r10, rax; mov r11, [rsp+0xC0]; movdqu xmm0, [rsp+0xD0]
    c.extend_from_slice(&[0x49, 0x89, 0xC2]);
    c.extend_from_slice(&[0x4C, 0x8B, 0x9C, 0x24, 0xC0, 0, 0, 0]);
    c.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x84, 0x24, 0xD0, 0, 0, 0]);
    // mov rsp, rbp; pop rbp; pop r9 (adresse de retour) : rsp = celui de l'appelant
    c.extend_from_slice(&[0x48, 0x89, 0xEC, 0x5D, 0x41, 0x59]);
    // mov rdx, rdi; mov rdi, r10; lea rcx, [rsp-marge]; sub rcx, rdi; shr rcx, 3
    c.extend_from_slice(&[0x48, 0x89, 0xFA, 0x4C, 0x89, 0xD7]);
    c.extend_from_slice(&[
        0x48,
        0x8D,
        0x4C,
        0x24,
        (STACK_SCRUB_MARGIN as u8).wrapping_neg(),
    ]);
    c.extend_from_slice(&[0x48, 0x29, 0xF9, 0x48, 0xC1, 0xE9, 0x03]);
    // mov rax, motif; cld; rep stosq
    c.extend_from_slice(&[0x48, 0xB8]);
    c.extend_from_slice(&STACK_SCRUB_PATTERN.to_le_bytes());
    c.extend_from_slice(&[0xFC, 0xF3, 0x48, 0xAB]);
    // mov rdi, rdx; mov rax, r11; jmp r9
    c.extend_from_slice(&[0x48, 0x89, 0xD7, 0x4C, 0x89, 0xD8, 0x41, 0xFF, 0xE1]);
    alloc_code(&c)
}

/// Diagnostic `WINLUX_STACK_SCRUB` (#2120) : avec `1` ou une liste de
/// fragments de noms, chaque fonction intégrée importée par une image x64
/// passe par [`stack_scrub_thunk`], qui remplit au retour la pile morte de
/// l'appelant d'un motif reconnaissable. Un programme qui lit une variable
/// locale jamais initialisée lit alors ce motif au lieu des restes de nos
/// fonctions (Acrobat, #1818). Jamais actif par défaut ; sans effet dans
/// un processus x86.
pub fn stack_scrub(dll: &str, func: &str, target: usize) -> usize {
    let Some(filter) = stack_scrub_filter() else {
        return target;
    };
    if crate::wow32::active() || !stack_scrub_wanted(filter, &format!("{dll}!{func}")) {
        return target;
    }
    if !is_executable(target) {
        return target;
    }
    static CACHE: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&(_, t)) = cache.iter().find(|(a, _)| *a == target) {
        return t;
    }
    let t = stack_scrub_thunk(target);
    cache.push((target, t));
    t
}

/// Fragments de `WINLUX_COMTRACE` (vide : toutes les tables), ou `None`
/// quand la trace des méthodes COM est éteinte.
fn com_trace_filter() -> Option<&'static [String]> {
    static F: std::sync::OnceLock<Option<Vec<String>>> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let raw = std::env::var("WINLUX_COMTRACE").ok()?;
        if raw.is_empty() || raw == "0" {
            return None;
        }
        Some(
            raw.split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty() && s != "1")
                .collect(),
        )
    })
    .as_deref()
}

/// Méthode COM tracée : sa table et sa case.
struct ComTraced {
    label: String,
    slot: usize,
}

/// Appels COM tracés depuis le démarrage (pour les tests).
static COM_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Journal d'une méthode COM tracée, après son retour : table et case,
/// `this` et les trois arguments suivants, valeur rendue (HRESULT dans
/// eax), appelant. Pour QueryInterface (case 0), l'IID demandé.
extern "win64" fn com_traced(
    info: *const ComTraced,
    ret: usize,
    args: *const usize,
    caller: usize,
) {
    COM_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let info = unsafe { &*info };
    let a = unsafe { std::slice::from_raw_parts(args, 4) };
    let mut extra = String::new();
    if info.slot == 0 && a[1] != 0 {
        let g = unsafe { std::ptr::read_unaligned(a[1] as *const [u8; 16]) };
        let d1 = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
        let d2 = u16::from_le_bytes([g[4], g[5]]);
        let d3 = u16::from_le_bytes([g[6], g[7]]);
        extra = format!(
            " iid={d1:08x}-{d2:04x}-{d3:04x}-{:02x}{:02x}-{}",
            g[8],
            g[9],
            g[10..]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    let site = describe_site(caller);
    let line = format!(
        "winlux[com] {tid} {}#{} this={:#x} ({:#x}, {:#x}, {:#x}){extra} = {:#x} (eax {:#010x}) <- {site}",
        info.label, info.slot, a[0], a[1], a[2], a[3], ret, ret as u32
    );
    #[cfg(test)]
    {
        *COM_LAST.lock().unwrap_or_else(|e| e.into_inner()) = line;
    }
    #[cfg(not(test))]
    api_log(&line);
}

#[cfg(test)]
static COM_LAST: Mutex<String> = Mutex::new(String::new());

/// Pont qui appelle `target` (jusqu'à 20 arguments win64, xmm0 rendu
/// intact) puis journalise l'appel par [`com_traced`].
fn com_trace_thunk(info: *const ComTraced, target: usize) -> usize {
    let mut c: Vec<u8> = Vec::with_capacity(320);
    // push rbp; mov rbp, rsp; sub rsp, 0xE0
    c.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x81, 0xEC, 0xE0, 0, 0, 0]);
    // Arguments 5 à 20 recopiés : mov rax, [rbp+0x30+8i]; mov [rsp+0x20+8i], rax
    for i in 0..16u32 {
        c.extend_from_slice(&[0x48, 0x8B, 0x85]);
        c.extend_from_slice(&(0x30 + 8 * i).to_le_bytes());
        c.extend_from_slice(&[0x48, 0x89, 0x84, 0x24]);
        c.extend_from_slice(&(0x20 + 8 * i).to_le_bytes());
    }
    // mov [rsp+0xA0..0xB8], rcx/rdx/r8/r9
    c.extend_from_slice(&[0x48, 0x89, 0x8C, 0x24, 0xA0, 0, 0, 0]);
    c.extend_from_slice(&[0x48, 0x89, 0x94, 0x24, 0xA8, 0, 0, 0]);
    c.extend_from_slice(&[0x4C, 0x89, 0x84, 0x24, 0xB0, 0, 0, 0]);
    c.extend_from_slice(&[0x4C, 0x89, 0x8C, 0x24, 0xB8, 0, 0, 0]);
    // mov rax, target; call rax
    c.extend_from_slice(&[0x48, 0xB8]);
    c.extend_from_slice(&(target as u64).to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]);
    // mov [rsp+0xC0], rax; movdqu [rsp+0xD0], xmm0
    c.extend_from_slice(&[0x48, 0x89, 0x84, 0x24, 0xC0, 0, 0, 0]);
    c.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x84, 0x24, 0xD0, 0, 0, 0]);
    // mov rcx, info; mov rdx, rax; lea r8, [rsp+0xA0]; mov r9, [rbp+8]
    c.extend_from_slice(&[0x48, 0xB9]);
    c.extend_from_slice(&(info as u64).to_le_bytes());
    c.extend_from_slice(&[0x48, 0x89, 0xC2]);
    c.extend_from_slice(&[0x4C, 0x8D, 0x84, 0x24, 0xA0, 0, 0, 0]);
    c.extend_from_slice(&[0x4C, 0x8B, 0x4D, 0x08]);
    // mov rax, com_traced; call rax
    c.extend_from_slice(&[0x48, 0xB8]);
    c.extend_from_slice(&(com_traced as *const () as usize as u64).to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]);
    // mov rax, [rsp+0xC0]; movdqu xmm0, [rsp+0xD0]; mov rsp, rbp; pop rbp; ret
    c.extend_from_slice(&[0x48, 0x8B, 0x84, 0x24, 0xC0, 0, 0, 0]);
    c.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x84, 0x24, 0xD0, 0, 0, 0]);
    c.extend_from_slice(&[0x48, 0x89, 0xEC, 0x5D, 0xC3]);
    alloc_code(&c)
}

/// Copie tracée d'une table de méthodes COM win64 (`label` : nom de sa
/// structure) quand `WINLUX_COMTRACE` la vise, sinon `None`. Chaque case
/// sauf AddRef et Release devient un pont qui journalise l'appel dans le
/// journal d'API du processus (voir [`api_log_dir`]). Diagnostic de #996 :
/// voir quelle méthode DXGI ou Direct3D 11 rend une valeur refusée.
pub fn com_trace_table<T>(t: &'static T, module: &str, label: &str) -> Option<*const T> {
    let filter = com_trace_filter()?;
    let full = format!("{module}::{label}").to_ascii_lowercase();
    if !filter.is_empty() && !filter.iter().any(|f| full.contains(f.as_str())) {
        return None;
    }
    Some(com_trace_table_for(t, label) as *const T)
}

fn com_trace_table_for<T>(t: &'static T, label: &str) -> usize {
    static CACHE: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    let key = t as *const T as usize;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&(_, v)) = cache.iter().find(|(k, _)| *k == key) {
        return v;
    }
    let n = std::mem::size_of::<T>() / 8;
    let fns = unsafe { std::slice::from_raw_parts(key as *const usize, n) };
    let table: Vec<usize> = fns
        .iter()
        .enumerate()
        .map(|(slot, &f)| {
            if slot == 1 || slot == 2 || f == 0 {
                return f;
            }
            let info = Box::into_raw(Box::new(ComTraced {
                label: label.to_string(),
                slot,
            }));
            com_trace_thunk(info, f)
        })
        .collect();
    let v = Box::leak(table.into_boxed_slice()).as_ptr() as usize;
    cache.push((key, v));
    v
}

/// Vrai si `addr` est dans une projection exécutable du processus
/// (d'après /proc/self/maps, relu quand l'adresse n'est pas connue).
fn is_executable(addr: usize) -> bool {
    static RANGES: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    let known = |r: &[(usize, usize)]| r.iter().any(|&(lo, hi)| addr >= lo && addr < hi);
    let mut ranges = RANGES.lock().unwrap_or_else(|e| e.into_inner());
    if known(&ranges) {
        return true;
    }
    let Ok(maps) = std::fs::read_to_string("/proc/self/maps") else {
        return true;
    };
    *ranges = parse_exec_ranges(&maps);
    known(&ranges)
}

fn parse_exec_ranges(maps: &str) -> Vec<(usize, usize)> {
    maps.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let range = parts.next()?;
            let perms = parts.next()?;
            if perms.as_bytes().get(2) != Some(&b'x') {
                return None;
            }
            let (lo, hi) = range.split_once('-')?;
            Some((
                usize::from_str_radix(lo, 16).ok()?,
                usize::from_str_radix(hi, 16).ok()?,
            ))
        })
        .collect()
}

/// Journalise un GetProcAddress qui échoue sur une DLL intégrée, quand le
/// traçage d'API est actif.
pub fn api_trace_missing(dll: &str, func: &str) {
    if api_trace_filter().is_some() {
        let line = format!("winlux[api] GetProcAddress {dll}!{func} : absent");
        eprintln!("{line}");
        api_log(&line);
    }
}

/// Copie du code machine dans une zone exécutable.
pub fn alloc_code(code: &[u8]) -> usize {
    let mut p = POOL.lock().unwrap();
    let need = (code.len() + 15) & !15;
    if p.cur + need > p.end {
        let size = 64 * 1024;
        let m = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert!(m != libc::MAP_FAILED, "mmap des ponts");
        p.cur = m as usize;
        p.end = p.cur + size;
    }
    let at = p.cur;
    unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), at as *mut u8, code.len()) };
    p.cur += need;
    at
}

/// Arguments d'une fonction variadique Windows x64.
///
/// Sous Windows x64, les 4 premiers arguments passent par rcx/rdx/r8/r9 et
/// les suivants sur la pile, juste après les 32 octets réservés à l'appelant.
/// Une fois les registres recopiés dans cet espace réservé, tous les
/// arguments sont contigus en mémoire (8 octets chacun, les flottants
/// variadiques étant dupliqués dans les registres entiers). C'est exactement
/// le format d'un `va_list` Windows : un simple pointeur.
pub struct VaList {
    base: *const u64,
    p: *const u64,
}

impl VaList {
    pub fn new(p: *const u64) -> Self {
        VaList { base: p, p }
    }
    pub fn next_u64(&mut self) -> u64 {
        unsafe {
            let v = *self.p;
            self.p = self.p.add(1);
            v
        }
    }
    pub fn next_f64(&mut self) -> f64 {
        f64::from_bits(self.next_u64())
    }
    pub fn next_ptr<T>(&mut self) -> *const T {
        self.next_u64() as usize as *const T
    }
    /// Accès positionnel (index 0-basé depuis le premier argument variadique).
    pub unsafe fn get_u64(&self, index: usize) -> u64 {
        *self.base.add(index)
    }
    pub unsafe fn get_f64(&self, index: usize) -> f64 {
        f64::from_bits(*self.base.add(index))
    }
}

/// Déclare un pont variadique : `$thunk` reçoit l'appel Windows, recopie
/// rcx/rdx/r8/r9 dans l'espace réservé et appelle `$imp(args: *const u64)`.
#[macro_export]
macro_rules! varargs_thunk {
    ($thunk:ident => $imp:path) => {
        core::arch::global_asm!(
            concat!(".globl ", stringify!($thunk)),
            concat!(".p2align 4"),
            concat!(stringify!($thunk), ":"),
            "mov [rsp+8], rcx",
            "mov [rsp+16], rdx",
            "mov [rsp+24], r8",
            "mov [rsp+32], r9",
            "lea rcx, [rsp+8]",
            "sub rsp, 40",
            "call {imp}",
            "add rsp, 40",
            "ret",
            imp = sym $imp,
        );
        extern "C" {
            pub fn $thunk();
        }
    };
}

#[cfg(test)]
mod api_trace_tests {
    use super::*;

    #[test]
    fn le_tracage_n_enveloppe_pas_les_exportations_de_donnees() {
        static DONNEE: u64 = 42;
        let data = &DONNEE as *const u64 as usize;
        assert!(!is_executable(data));
        assert!(is_executable(describe_site as *const () as usize));
        let r = parse_exec_ranges("1000-2000 r-xp 0 0:0 0 x\n3000-4000 rw-p 0 0:0 0 y\n");
        assert_eq!(r, vec![(0x1000, 0x2000)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "win64" fn somme_six(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> u64 {
        // Un cadre un peu épais, pour salir la pile sous l'appelant.
        let mut t = [0u64; 64];
        for (i, x) in t.iter_mut().enumerate() {
            *x = std::hint::black_box(i as u64);
        }
        std::hint::black_box(&t);
        a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f
    }

    extern "win64" fn flottant(x: f64) -> f64 {
        x * 2.5
    }

    #[test]
    fn remplissage_de_pile_rend_valeurs_et_remplit_la_pile_morte() {
        let t = stack_scrub_thunk(somme_six as *const () as usize);
        let f: extern "win64" fn(u64, u64, u64, u64, u64, u64) -> u64 =
            unsafe { std::mem::transmute(t) };
        assert_eq!(f(1, 2, 3, 4, 5, 6), 1 + 4 + 9 + 16 + 25 + 36);
        let g: extern "win64" fn(f64) -> f64 =
            unsafe { std::mem::transmute(stack_scrub_thunk(flottant as *const () as usize)) };
        assert_eq!(g(4.0), 10.0);
        // Appel en assembleur pour lire la pile morte juste après le retour,
        // avant que du code Rust ne la réutilise ; rdi doit être préservé.
        let (lu, rdi_apres, rendu): (u64, u64, u64);
        unsafe {
            std::arch::asm!(
                "mov r12, rsp",
                "and rsp, -16",
                "sub rsp, 0x40",
                "mov rcx, 1", "mov rdx, 1", "mov r8, 1", "mov r9, 1",
                "mov qword ptr [rsp+0x20], 1",
                "mov qword ptr [rsp+0x28], 1",
                "call r13",
                "mov rsi, [rsp-0x800]",
                "mov rsp, r12",
                in("r13") t,
                out("rsi") lu,
                inout("rdi") 0x1234_5678u64 => rdi_apres,
                out("rax") rendu,
                out("r12") _,
                clobber_abi("win64"),
            );
        }
        assert_eq!(rendu, 21);
        assert_eq!(rdi_apres, 0x1234_5678);
        assert_eq!(lu, STACK_SCRUB_PATTERN);
    }

    #[test]
    fn remplissage_de_pile_bornes_et_exclusions() {
        // Hors de toute projection : rien à remplir.
        assert_eq!(stack_scrub_start(0x1000), 0x1000 - STACK_SCRUB_MARGIN);
        // Jamais sous le bas de la projection, toujours un multiple de 8.
        assert_eq!(stack_scrub_bounds(0x10_0000, 0x0F_F000), 0x0F_F000);
        assert_eq!(
            stack_scrub_bounds(0x10_0000, 0x1000),
            0x10_0000 - STACK_SCRUB_BYTES
        );
        assert_eq!(
            (0x10_0000 - stack_scrub_bounds(0x10_0000, 0x0F_F003)) % 8,
            0
        );
        assert!(stack_scrub_bounds(0x10_0000, 0x0F_F003) >= 0x0F_F003);
        let maps = "7f0000000000-7f0000200000 rw-p 00000000 00:00 0\n\
                    7f0000200000-7f0000201000 ---p 00000000 00:00 0\n";
        assert_eq!(
            rw_mapping_containing(maps, 0x7f00001fff00),
            Some((0x7f0000000000, 0x7f0000200000))
        );
        assert_eq!(rw_mapping_containing(maps, 0x7f0000200800), None);
        let tout: Vec<String> = Vec::new();
        assert!(stack_scrub_wanted(&tout, "shlwapi!PathAddBackslashW"));
        assert!(!stack_scrub_wanted(&tout, "msvcrt!_setjmp"));
        assert!(!stack_scrub_wanted(&tout, "kernel32!RaiseException"));
        assert!(!stack_scrub_wanted(&tout, "ntdll!__chkstk"));
        assert!(!stack_scrub_wanted(&tout, "kernel32!SwitchToFiber"));
        let shlwapi = vec!["shlwapi".to_string()];
        assert!(stack_scrub_wanted(&shlwapi, "shlwapi!PathAppendW"));
        assert!(!stack_scrub_wanted(&shlwapi, "kernel32!CreateFileW"));
    }

    #[test]
    fn journal_api_plafonne() {
        let dir = std::env::temp_dir().join(format!("winlux-apitrace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, name) in ["a.log", "b.log", "c.log"].iter().enumerate() {
            std::fs::write(dir.join(name), vec![b'x'; 1000]).unwrap();
            let t =
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i as u64);
            std::fs::File::options()
                .write(true)
                .open(dir.join(name))
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        // 3000 octets pour un plafond de 2000 : le plus ancien part.
        prune_api_logs(&dir, 2000);
        assert!(!dir.join("a.log").exists());
        assert!(dir.join("b.log").exists() && dir.join("c.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!api_log_dir().unwrap().starts_with("/tmp"));
    }

    /// Table d'essai : QueryInterface, AddRef, Release, une méthode à six
    /// arguments et une méthode qui rend un flottant.
    #[repr(C)]
    struct EssaiVt {
        qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32,
        add_ref: unsafe extern "win64" fn(usize) -> u32,
        release: unsafe extern "win64" fn(usize) -> u32,
        somme: unsafe extern "win64" fn(usize, u64, u64, u64, u64, u64) -> u64,
        flottant: unsafe extern "win64" fn(usize, f32) -> f32,
    }
    unsafe extern "win64" fn e_qi(_: usize, _: *const u8, out: *mut usize) -> i32 {
        *out = 0;
        0x8000_4002u32 as i32
    }
    unsafe extern "win64" fn e_one(_: usize) -> u32 {
        1
    }
    unsafe extern "win64" fn e_somme(this: usize, a: u64, b: u64, c: u64, d: u64, e: u64) -> u64 {
        this as u64 + a + 10 * b + 100 * c + 1000 * d + 10000 * e
    }
    unsafe extern "win64" fn e_flottant(_: usize, x: f32) -> f32 {
        x * 2.0
    }
    static ESSAI_VT: EssaiVt = EssaiVt {
        qi: e_qi,
        add_ref: e_one,
        release: e_one,
        somme: e_somme,
        flottant: e_flottant,
    };

    #[test]
    fn trace_com_rend_les_valeurs_et_journalise() {
        let t = com_trace_table_for(&ESSAI_VT, "EssaiVt") as *const EssaiVt;
        assert_ne!(t, &ESSAI_VT as *const EssaiVt);
        // Même table rendue au second appel.
        assert_eq!(
            com_trace_table_for(&ESSAI_VT, "EssaiVt") as *const EssaiVt,
            t
        );
        unsafe {
            let vt = &*t;
            // AddRef et Release gardent la fonction d'origine.
            assert_eq!(vt.add_ref as usize, e_one as usize);
            let avant = COM_CALLS.load(std::sync::atomic::Ordering::Relaxed);
            assert_eq!(
                (vt.somme)(7, 1, 2, 3, 4, 5),
                7 + 1 + 20 + 300 + 4000 + 50000
            );
            assert_eq!((vt.flottant)(0, 1.5), 3.0);
            let iid = [
                0xf7, 0x45, 0x0a, 0x79, 0x42, 0x0d, 0x76, 0x48, 0x98, 0x3a, 0x0a, 0x55, 0xcf, 0xe6,
                0xf4, 0xaa,
            ];
            let mut out = 1usize;
            assert_eq!((vt.qi)(9, iid.as_ptr(), &mut out), 0x8000_4002u32 as i32);
            assert_eq!(out, 0);
            assert!(COM_CALLS.load(std::sync::atomic::Ordering::Relaxed) >= avant + 3);
        }
        let last = COM_LAST.lock().unwrap().clone();
        assert!(last.contains("EssaiVt#0 this=0x9"), "{last}");
        assert!(
            last.contains("iid=790a45f7-0d42-4876-983a-0a55cfe6f4aa"),
            "{last}"
        );
        assert!(last.contains("eax 0x80004002"), "{last}");
    }
}
