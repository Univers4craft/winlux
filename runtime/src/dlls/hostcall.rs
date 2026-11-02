//! Ponts génériques Windows x64 → System V vers les bibliothèques de l'hôte
//! (libGL, libvulkan…).
//!
//! Un programme Windows appelle `glUniform4f` avec la convention Windows x64 :
//! quatre premiers arguments dans rcx/rdx/r8/r9 **ou** xmm0–xmm3 selon leur
//! position, les suivants sur la pile après 32 octets réservés. La libGL de
//! Linux attend la convention System V : entiers dans rdi, rsi, rdx, rcx, r8,
//! r9 et flottants dans xmm0–xmm7, comptés séparément. Sans connaître les
//! types, impossible de répartir les arguments : chaque fonction a donc une
//! signature, tirée des registres Khronos (`sigs/gl.txt`, `sigs/vk.txt`,
//! générés par `scripts/gen-host-signatures.py`).
//!
//! Pour chaque fonction, un minuscule morceau de code généré charge
//! l'adresse de sa description dans r10 puis saute vers un pont commun
//! (`winlux_hostcall_bridge`) qui :
//! 1. sauvegarde les registres que Windows considère préservés et que
//!    System V ne préserve pas (rdi, rsi, xmm6–xmm15) ;
//! 2. demande à [`marshal`] de répartir les arguments ;
//! 3. charge les registres System V, recopie les arguments de pile et appelle
//!    la fonction de l'hôte. La valeur de retour (rax ou xmm0) est la même
//!    dans les deux conventions : rien à convertir.
//!
//! La fonction de l'hôte est cherchée au premier appel ([`Resolver`]) : créer
//! les exports d'opengl32.dll ne charge pas libGL.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// Nombre maximal d'arguments d'une fonction pontée.
const MAX_ARGS: usize = 32;

/// Comment trouver la fonction de l'hôte au premier appel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resolver {
    /// L'adresse est déjà connue.
    Fixed,
    /// `glXGetProcAddressARB` de libGL.
    Gl,
    /// `dlsym` dans libvulkan.
    Vulkan,
}

/// Description d'une fonction pontée (pointée par r10 dans le pont).
#[repr(C)]
pub struct HostFn {
    host: AtomicUsize,
    resolver: Resolver,
    name: CString,
    kinds: Box<[u8]>,
}

/// Résultat de la répartition, lu par le pont en assembleur. L'ordre des
/// champs et leurs positions sont fixés par l'assembleur ci-dessous.
#[repr(C)]
struct Out {
    gp: [u64; 6],
    xmm: [u64; 8],
    nstack: u64,
    func: u64,
    stack: [u64; MAX_ARGS],
}

const _: () = assert!(std::mem::size_of::<Out>() == 384);

fn is_float(k: u8) -> bool {
    k == b'f' || k == b'd'
}

/// Convertit un argument Windows en valeur System V : les petits entiers sont
/// étendus (clang suppose les octets et mots déjà étendus à 32 bits).
fn convert(k: u8, v: u64) -> u64 {
    match k {
        b'b' => v & 0xff,
        b'c' => v as u8 as i8 as i32 as u32 as u64,
        b'h' => v & 0xffff,
        b's' => v as u16 as i16 as i32 as u32 as u64,
        b'A' => 0,
        _ => v,
    }
}

/// Répartit les arguments d'un appel Windows selon la convention System V.
/// `win` : rcx, rdx, r8, r9 puis xmm0–xmm3 ; `wstack` : 5ᵉ argument et suivants.
unsafe extern "sysv64" fn marshal(
    d: *const HostFn,
    win: *const u64,
    wstack: *const u64,
    out: *mut Out,
) {
    let d = &*d;
    let out = &mut *out;
    let (mut ng, mut nx, mut ns) = (0usize, 0usize, 0usize);
    for (i, &k) in d.kinds.iter().enumerate() {
        let raw = if i < 4 {
            if is_float(k) {
                *win.add(4 + i)
            } else {
                *win.add(i)
            }
        } else {
            *wstack.add(i - 4)
        };
        let v = convert(k, raw);
        if is_float(k) && nx < 8 {
            out.xmm[nx] = v;
            nx += 1;
        } else if !is_float(k) && ng < 6 {
            out.gp[ng] = v;
            ng += 1;
        } else {
            out.stack[ns] = v;
            ns += 1;
        }
    }
    out.nstack = ns as u64;
    out.func = resolve(d) as u64;
}

extern "C" fn host_missing() -> u64 {
    0
}

fn resolve(d: &HostFn) -> usize {
    let f = d.host.load(Ordering::Acquire);
    if f != 0 {
        return f;
    }
    let found = match d.resolver {
        Resolver::Fixed => 0,
        Resolver::Gl => gl_proc(d.name.as_bytes_with_nul()),
        Resolver::Vulkan => vk_sym(d.name.as_bytes_with_nul()),
    };
    let f = if found == 0 {
        crate::thunks::log_missing(&format!("hôte!{}", d.name.to_string_lossy()), true);
        crate::log_warn!(
            "hostcall",
            "{} absente de l'hôte : renvoie 0",
            d.name.to_string_lossy()
        );
        host_missing as *const () as usize
    } else {
        found
    };
    d.host.store(f, Ordering::Release);
    f
}

// Pile après `push rbp; mov rbp, rsp; sub rsp, 624` (rbp aligné sur 16) :
//   rbp-8 rdi, rbp-16 rsi, rbp-176..-16 xmm6–xmm15,
//   rbp-240 rcx rdx r8 r9 xmm0 xmm1 xmm2 xmm3, rbp-624 structure `Out`.
core::arch::global_asm!(
    ".globl winlux_hostcall_bridge",
    ".p2align 4",
    "winlux_hostcall_bridge:",
    "push rbp",
    "mov rbp, rsp",
    "sub rsp, 624",
    "mov [rbp-8], rdi",
    "mov [rbp-16], rsi",
    "movdqu [rbp-176], xmm6",
    "movdqu [rbp-160], xmm7",
    "movdqu [rbp-144], xmm8",
    "movdqu [rbp-128], xmm9",
    "movdqu [rbp-112], xmm10",
    "movdqu [rbp-96], xmm11",
    "movdqu [rbp-80], xmm12",
    "movdqu [rbp-64], xmm13",
    "movdqu [rbp-48], xmm14",
    "movdqu [rbp-32], xmm15",
    "mov [rbp-240], rcx",
    "mov [rbp-232], rdx",
    "mov [rbp-224], r8",
    "mov [rbp-216], r9",
    "movq [rbp-208], xmm0",
    "movq [rbp-200], xmm1",
    "movq [rbp-192], xmm2",
    "movq [rbp-184], xmm3",
    "mov rdi, r10",
    "lea rsi, [rbp-240]",
    "lea rdx, [rbp+48]",
    "lea rcx, [rbp-624]",
    "call {marshal}",
    // Arguments de pile System V, rsp restant aligné sur 16.
    "mov rcx, [rbp-512]",
    "lea rax, [rcx*8+15]",
    "and rax, -16",
    "sub rsp, rax",
    "xor eax, eax",
    ".Lwinlux_hostcall_copy:",
    "cmp rax, rcx",
    "jae .Lwinlux_hostcall_regs",
    "mov r11, [rbp+rax*8-496]",
    "mov [rsp+rax*8], r11",
    "inc rax",
    "jmp .Lwinlux_hostcall_copy",
    ".Lwinlux_hostcall_regs:",
    "movq xmm0, [rbp-576]",
    "movq xmm1, [rbp-568]",
    "movq xmm2, [rbp-560]",
    "movq xmm3, [rbp-552]",
    "movq xmm4, [rbp-544]",
    "movq xmm5, [rbp-536]",
    "movq xmm6, [rbp-528]",
    "movq xmm7, [rbp-520]",
    "mov rdi, [rbp-624]",
    "mov rsi, [rbp-616]",
    "mov rdx, [rbp-608]",
    "mov rcx, [rbp-600]",
    "mov r8, [rbp-592]",
    "mov r9, [rbp-584]",
    "mov r11, [rbp-504]",
    "mov eax, 8",
    "call r11",
    "mov rdi, [rbp-8]",
    "mov rsi, [rbp-16]",
    "movdqu xmm6, [rbp-176]",
    "movdqu xmm7, [rbp-160]",
    "movdqu xmm8, [rbp-144]",
    "movdqu xmm9, [rbp-128]",
    "movdqu xmm10, [rbp-112]",
    "movdqu xmm11, [rbp-96]",
    "movdqu xmm12, [rbp-80]",
    "movdqu xmm13, [rbp-64]",
    "movdqu xmm14, [rbp-48]",
    "movdqu xmm15, [rbp-32]",
    "mov rsp, rbp",
    "pop rbp",
    "ret",
    marshal = sym marshal,
);

extern "C" {
    fn winlux_hostcall_bridge();
}

/// Fabrique la fonction Windows qui appelle `name` de l'hôte avec la
/// signature `kinds`. `host` vaut 0 pour une recherche au premier appel.
pub fn make_bridge(name: &str, kinds: &str, resolver: Resolver, host: usize) -> usize {
    let kinds: Box<[u8]> = if kinds == "-" {
        Box::new([])
    } else {
        kinds.as_bytes().into()
    };
    assert!(kinds.len() <= MAX_ARGS, "{name} : trop d'arguments");
    let d = Box::leak(Box::new(HostFn {
        host: AtomicUsize::new(host),
        resolver,
        name: CString::new(name).unwrap_or_default(),
        kinds,
    })) as *const HostFn as u64;
    // mov r10, d ; mov r11, pont ; jmp r11
    let mut c = Vec::with_capacity(24);
    c.extend_from_slice(&[0x49, 0xBA]);
    c.extend_from_slice(&d.to_le_bytes());
    c.extend_from_slice(&[0x49, 0xBB]);
    c.extend_from_slice(&(winlux_hostcall_bridge as *const () as usize as u64).to_le_bytes());
    c.extend_from_slice(&[0x41, 0xFF, 0xE3]);
    crate::thunks::alloc_code(&c)
}

/// Table de signatures : nom → (codes, exportée directement par la DLL).
pub struct Signatures {
    map: HashMap<&'static str, (&'static str, bool)>,
}

impl Signatures {
    fn parse(text: &'static str) -> Self {
        let mut map = HashMap::new();
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let (core, rest) = match line.strip_prefix("* ") {
                Some(r) => (true, r),
                None => (false, line),
            };
            if let Some((name, kinds)) = rest.split_once(' ') {
                map.insert(name, (kinds, core));
            }
        }
        Signatures { map }
    }

    pub fn get(&self, name: &str) -> Option<&'static str> {
        self.map.get(name).map(|(k, _)| *k)
    }

    /// Fonctions exportées directement par la DLL Windows correspondante.
    pub fn core(&self) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
        self.map
            .iter()
            .filter(|(_, (_, c))| *c)
            .map(|(n, (k, _))| (*n, *k))
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

pub fn gl_signatures() -> &'static Signatures {
    static S: OnceLock<Signatures> = OnceLock::new();
    S.get_or_init(|| Signatures::parse(include_str!("sigs/gl.txt")))
}

pub fn vk_signatures() -> &'static Signatures {
    static S: OnceLock<Signatures> = OnceLock::new();
    S.get_or_init(|| Signatures::parse(include_str!("sigs/vk.txt")))
}

/// Ponts déjà fabriqués, par (nom, adresse hôte) : les fonctions Vulkan de
/// périphérique ont une adresse par périphérique.
fn cache() -> &'static Mutex<HashMap<(String, usize), usize>> {
    static C: OnceLock<Mutex<HashMap<(String, usize), usize>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Pont vers une fonction de l'hôte déjà trouvée (cache par adresse).
pub fn bridge_for(name: &str, kinds: &str, host: usize) -> usize {
    let key = (name.to_string(), host);
    let mut c = super::lock(cache());
    *c.entry(key)
        .or_insert_with(|| make_bridge(name, kinds, Resolver::Fixed, host))
}

/// Pont résolu au premier appel (un seul par nom et par résolveur).
pub fn lazy_bridge(name: &str, kinds: &str, resolver: Resolver) -> usize {
    let tag = match resolver {
        Resolver::Gl => usize::MAX,
        Resolver::Vulkan => usize::MAX - 1,
        Resolver::Fixed => usize::MAX - 2,
    };
    let key = (name.to_string(), tag);
    let mut c = super::lock(cache());
    *c.entry(key)
        .or_insert_with(|| make_bridge(name, kinds, resolver, 0))
}

// ---------- Bibliothèques de l'hôte ----------

struct Lib(usize);

fn open_lib(names: &[&std::ffi::CStr]) -> usize {
    for n in names {
        let h = unsafe { libc::dlopen(n.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if !h.is_null() {
            return h as usize;
        }
    }
    0
}

pub fn libgl() -> usize {
    static L: OnceLock<Lib> = OnceLock::new();
    L.get_or_init(|| Lib(open_lib(&[c"libGL.so.1", c"libGL.so"])))
        .0
}

pub fn libvulkan() -> usize {
    static L: OnceLock<Lib> = OnceLock::new();
    L.get_or_init(|| Lib(open_lib(&[c"libvulkan.so.1", c"libvulkan.so"])))
        .0
}

pub fn libx11() -> usize {
    static L: OnceLock<Lib> = OnceLock::new();
    L.get_or_init(|| Lib(open_lib(&[c"libX11.so.6", c"libX11.so"])))
        .0
}

/// `dlsym` ; `name` se termine par un octet nul.
pub fn sym(lib: usize, name: &[u8]) -> usize {
    if lib == 0 || name.last() != Some(&0) {
        return 0;
    }
    unsafe { libc::dlsym(lib as *mut libc::c_void, name.as_ptr().cast()) as usize }
}

/// Adresse d'une fonction OpenGL (cœur ou extension) via glXGetProcAddressARB.
pub fn gl_proc(name: &[u8]) -> usize {
    let lib = libgl();
    let gpa = sym(lib, b"glXGetProcAddressARB\0");
    if gpa != 0 {
        let f: unsafe extern "C" fn(*const u8) -> usize = unsafe { std::mem::transmute(gpa) };
        let p = unsafe { f(name.as_ptr()) };
        if p != 0 {
            return p;
        }
    }
    sym(lib, name)
}

pub fn vk_sym(name: &[u8]) -> usize {
    sym(libvulkan(), name)
}

/// Connexion X11 propre aux ponts graphiques (Vulkan), ouverte une fois.
pub fn x_display() -> usize {
    static D: OnceLock<Lib> = OnceLock::new();
    D.get_or_init(|| {
        let open = sym(libx11(), b"XOpenDisplay\0");
        if open == 0 {
            return Lib(0);
        }
        let f: unsafe extern "C" fn(*const i8) -> usize = unsafe { std::mem::transmute(open) };
        let d = unsafe { f(std::ptr::null()) };
        if d != 0 {
            super::dxgi::install_x_error_guard();
        }
        Lib(d)
    })
    .0
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn mix(
        a: i32,
        x: f32,
        b: u8,
        y: f64,
        c: i64,
        d: i16,
        e: i32,
        f: i32,
        g: i32,
        z: f32,
        h: i32,
    ) -> f64 {
        a as f64
            + x as f64 * 10.0
            + b as f64 * 100.0
            + y * 1000.0
            + c as f64 * 1e4
            + d as f64 * 1e5
            + (e + f + g + h) as f64 * 1e6
            + z as f64 * 1e7
    }

    #[test]
    fn pont_reordonne_les_arguments() {
        let host = mix as *const () as usize;
        let p = make_bridge("mix", "ifbdpsiiifi", Resolver::Fixed, host);
        let f: extern "win64" fn(i32, f32, u64, f64, i64, u64, i32, i32, i32, f32, i32) -> f64 =
            unsafe { std::mem::transmute(p) };
        // b reçoit des bits parasites au-dessus de l'octet, d un mot négatif.
        let r = f(1, 2.0, 0xdead_0003, 4.0, 5, 0xffff_fffe, 1, 1, 1, 0.5, 1);
        assert_eq!(r, 1.0 + 20.0 + 300.0 + 4000.0 + 5e4 - 2e5 + 4e6 + 5e6);
    }

    extern "C" fn many(
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        e: f64,
        f: f64,
        g: f64,
        h: f64,
        i: f64,
        j: i64,
    ) -> f64 {
        a + b + c + d + e + f + g + h + i * 100.0 + j as f64 * 1000.0
    }

    #[test]
    fn pont_debordement_sur_la_pile() {
        let p = make_bridge(
            "many",
            "dddddddddp",
            Resolver::Fixed,
            many as *const () as usize,
        );
        let f: extern "win64" fn(f64, f64, f64, f64, f64, f64, f64, f64, f64, i64) -> f64 =
            unsafe { std::mem::transmute(p) };
        assert_eq!(
            f(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 3),
            8.0 + 200.0 + 3000.0
        );
    }

    #[test]
    fn tables_de_signatures() {
        let gl = gl_signatures();
        assert!(gl.len() > 3000);
        assert_eq!(gl.get("glRotatef"), Some("ffff"));
        assert_eq!(gl.get("glColor4ub"), Some("bbbb"));
        assert_eq!(gl.core().count(), 336);
        let vk = vk_signatures();
        assert_eq!(vk.get("vkCmdSetDepthBias"), Some("pfff"));
        assert_eq!(vk.get("vkCreateInstance"), Some("pAp"));
        assert!(vk.core().any(|(n, _)| n == "vkCreateDevice"));
    }
}
