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

extern "win64" fn traced_called(info: *const Traced, ret: usize) {
    let label = unsafe { &(*info).label };
    let site = describe_site(ret);
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    eprintln!("winlux[api] {tid} {label} <- {site}");
}

/// Avec `WINLUX_APITRACE=1` (ou une liste de fragments de noms séparés par
/// des virgules), chaque appel d'une fonction intégrée importée est
/// journalisé sur stderr avec l'adresse de l'appelant, puis transmis tel
/// quel. Outil de diagnostic pour les programmes qui se bloquent.
pub fn api_trace(dll: &str, func: &str, target: usize) -> usize {
    let Some(filter) = api_trace_filter() else {
        return target;
    };
    let label = format!("{}!{}", dll.to_ascii_lowercase(), func);
    let lower = label.to_ascii_lowercase();
    if !filter.is_empty() && !filter.iter().any(|f| lower.contains(f.as_str())) {
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

/// Journalise un GetProcAddress qui échoue sur une DLL intégrée, quand le
/// traçage d'API est actif.
pub fn api_trace_missing(dll: &str, func: &str) {
    if api_trace_filter().is_some() {
        eprintln!("winlux[api] GetProcAddress {dll}!{func} : absent");
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
