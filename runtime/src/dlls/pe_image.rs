//! Image PE minimale pour une DLL intégrée (#851).
//!
//! Le HMODULE d'une DLL intégrée n'était qu'une page commençant par « MZ ».
//! Certains programmes ne passent pas par GetProcAddress mais lisent eux-mêmes
//! la table d'exports de l'image : le bac à sable de Chromium (claude.exe,
//! Electron) résout ainsi NtQueryInformationProcess et les autres fonctions de
//! ntdll. Sans en-têtes PE, tous ses pointeurs restaient nuls et le courtier
//! sautait à l'adresse 0 juste après avoir créé l'enfant.
//!
//! L'image construite ici a de vrais en-têtes DOS et PE32+, une section et un
//! répertoire d'exports trié. Chaque fonction exportée y a un relais de 16
//! octets (`jmp [rip+2]` vers la fonction Rust) : une RVA ne peut désigner
//! qu'une adresse de l'image. Les exports de données, hors du code de Winlux,
//! ne sont pas publiés dans cette table (GetProcAddress les rend toujours).
//!
//! Bac à sable de Chromium (#767) : avant de reprendre un enfant suspendu, le
//! courtier intercepte des fonctions Nt* de sa ntdll. Il vérifie que chacune
//! a la forme d'un relais d'appel système de Windows x64, recopie ce relais
//! dans l'enfant et écrit un saut par-dessus, à la même adresse que chez lui.
//! D'où deux règles :
//! - les images intégrées sont placées à des adresses fixes, identiques dans
//!   tous les processus, comme les DLL système de Windows ;
//! - les fonctions Nt* et Zw* de ntdll ont un relais au format Windows,
//!   variante « int 2e » : `mov r10, rcx ; mov eax, N ; test byte ptr
//!   [7FFE0308h], 1 ; jne ; syscall ; ret ; int 2Eh ; ret`. L'octet
//!   SharedUserData!SystemCall vaut 1, donc le relais exécute `int 2Eh` (et
//!   jamais `syscall`, qui irait au noyau Linux). Le `int 2Eh` lève une faute
//!   que [`redirect_service`] reprend : RIP passe à la fonction Rust du
//!   service N, sans toucher à la pile, comme un saut direct depuis
//!   l'appelant. La copie du relais faite par Chromium marche de même.

const PAGE: usize = 0x1000;
const STUB: usize = 32;

/// Région des images intégrées : même ordre, mêmes tailles, donc mêmes
/// adresses dans chaque processus (sous le PEB fixe, `process::PEB_FIXED`).
const IMAGES_FIXED: usize = 0x5FF0_0000_0000;
const GRANULARITY: usize = 0x10000;
static NEXT_IMAGE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(IMAGES_FIXED);

/// KUSER_SHARED_DATA : adresse publique de la page partagée de Windows.
pub const SHARED_USER_DATA: usize = 0x7FFE_0000;
/// SharedUserData!SystemCall : 1 = les relais de ntdll passent par `int 2Eh`.
const SYSTEM_CALL: usize = 0x308;

/// Service N → fonction Rust de ntdll, rempli à la construction de ntdll.
static SERVICES: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();

/// Relais d'appel système de Windows x64 (variante « int 2e »), 32 octets.
fn service_stub(id: u32) -> [u8; STUB] {
    let mut s = [0u8; STUB];
    s[..4].copy_from_slice(&[0x4C, 0x8B, 0xD1, 0xB8]); // mov r10, rcx ; mov eax,
    s[4..8].copy_from_slice(&id.to_le_bytes());
    // test byte ptr [7FFE0308h], 1
    s[8..16].copy_from_slice(&[0xF6, 0x04, 0x25, 0x08, 0x03, 0xFE, 0x7F, 0x01]);
    s[16..18].copy_from_slice(&[0x75, 0x03]); // jne +3
    s[18..20].copy_from_slice(&[0x0F, 0x05]); // syscall
    s[20] = 0xC3; // ret
    s[21..23].copy_from_slice(&[0xCD, 0x2E]); // int 2Eh
    s[23] = 0xC3; // ret
    s[24..32].copy_from_slice(&[0x0F, 0x1F, 0x84, 0, 0, 0, 0, 0]); // nop
    s
}

fn is_service(dll: &str, name: &str) -> bool {
    dll == "ntdll" && (name.starts_with("Nt") || name.starts_with("Zw"))
}

/// Pose la page SharedUserData avec SystemCall = 1, une fois par processus,
/// avant tout code du programme.
pub fn map_shared_user_data() {
    unsafe {
        let p = libc::mmap(
            SHARED_USER_DATA as *mut libc::c_void,
            PAGE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED_NOREPLACE,
            -1,
            0,
        );
        if p == libc::MAP_FAILED {
            return;
        }
        if p as usize != SHARED_USER_DATA {
            // Noyau ancien : l'adresse n'était qu'un indice.
            libc::munmap(p, PAGE);
            return;
        }
        ((SHARED_USER_DATA + SYSTEM_CALL) as *mut u32).write(1);
        libc::mprotect(p, PAGE, libc::PROT_READ);
    }
}

/// Faute levée par un relais de service de ntdll (ou sa copie) : reprend
/// à la fonction du service, avec la pile et les registres de l'appelant.
/// Appelée depuis le gestionnaire de SIGSEGV.
pub unsafe fn redirect_service(info: *const libc::siginfo_t, uc: *mut libc::ucontext_t) -> bool {
    let Some(table) = SERVICES.get() else {
        return false;
    };
    let g = &mut (*uc).uc_mcontext.gregs;
    let rip = g[libc::REG_RIP as usize] as usize;
    let at = |o: usize| *((rip + o) as *const u8);
    let hit = match g[libc::REG_TRAPNO as usize] {
        // int 2Eh : faute de protection générale, RIP sur l'instruction.
        13 => at(0) == 0xCD && at(1) == 0x2E && at(2) == 0xC3,
        // Page SharedUserData absente : faute sur le `test` lui-même.
        14 => {
            (*info).si_addr() as usize == SHARED_USER_DATA + SYSTEM_CALL
                && rip != SHARED_USER_DATA + SYSTEM_CALL
                && (0..8).all(|i| at(i) == service_stub(0)[8 + i])
        }
        _ => false,
    };
    if !hit {
        return false;
    }
    let id = g[libc::REG_RAX as usize] as u32 as usize;
    match table.get(id) {
        Some(&f) if f != 0 => {
            g[libc::REG_RIP as usize] = f as i64;
            true
        }
        _ => false,
    }
}

/// Réserve `size` octets à la prochaine adresse fixe de la région, ou
/// ailleurs si elle est prise.
fn map_image(size: usize) -> *mut libc::c_void {
    // Programme x86 : tout l'espace au-dessus de 4 Go est réservé, la région
    // fixe n'y est jamais libre et l'image doit de toute façon rester sous
    // 4 Go. Chaque image donnait sinon un avertissement (224 pour un
    // programme PyInstaller 32 bits, #1383).
    if crate::wow32::active() {
        return unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
    }
    let span = size.div_ceil(GRANULARITY) * GRANULARITY;
    let want = NEXT_IMAGE.fetch_add(span, std::sync::atomic::Ordering::Relaxed);
    unsafe {
        let p = libc::mmap(
            want as *mut libc::c_void,
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED_NOREPLACE,
            -1,
            0,
        );
        if p != libc::MAP_FAILED && p as usize == want {
            return p;
        }
        if p != libc::MAP_FAILED {
            libc::munmap(p, size);
        }
        // Le bac à sable de Chromium suppose la même adresse dans l'enfant.
        crate::log_warn!(
            "pe",
            "image intégrée hors de sa région fixe ({want:#x} occupée)"
        );
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    }
}

/// Plages exécutables du binaire Winlux, pour distinguer fonctions et données.
fn code_ranges() -> &'static [(usize, usize)] {
    static R: std::sync::OnceLock<Vec<(usize, usize)>> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        let me = std::fs::read_link("/proc/self/exe").ok();
        let maps = std::fs::read_to_string("/proc/self/maps").unwrap_or_default();
        maps.lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                let range = f.next()?;
                let perms = f.next()?;
                let path = l.split_whitespace().nth(5).map(std::path::Path::new);
                if !perms.contains('x') || me.as_deref().is_some_and(|m| path != Some(m)) {
                    return None;
                }
                let (a, b) = range.split_once('-')?;
                Some((
                    usize::from_str_radix(a, 16).ok()?,
                    usize::from_str_radix(b, 16).ok()?,
                ))
            })
            .collect()
    })
}

fn is_code(addr: usize) -> bool {
    code_ranges().iter().any(|&(a, b)| addr >= a && addr < b)
}

fn put16(img: &mut [u8], at: usize, v: u16) {
    img[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(img: &mut [u8], at: usize, v: u32) {
    img[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(img: &mut [u8], at: usize, v: u64) {
    img[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// Construit l'image de `dll` (nom sans « .dll ») et renvoie son adresse.
pub fn build(dll: &str, exports: &super::Exports) -> usize {
    let mut names: Vec<(&str, usize)> = exports
        .iter()
        .filter(|(_, &a)| is_code(a))
        .map(|(&n, &a)| (n, a))
        .collect();
    names.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let n = names.len();
    let dll_name = format!("{dll}.dll");

    // Disposition : en-têtes (1 page), puis répertoire d'exports et tables,
    // puis noms, puis relais alignés sur 16 octets.
    let dir = PAGE;
    let funcs = dir + 40;
    let name_ptrs = funcs + 4 * n;
    let ords = name_ptrs + 4 * n;
    let mut strings = ords + 2 * n;
    let dll_name_rva = strings;
    strings += dll_name.len() + 1;
    let mut name_rvas = Vec::with_capacity(n);
    for (name, _) in &names {
        name_rvas.push(strings);
        strings += name.len() + 1;
    }
    let export_end = strings;
    let stubs = (export_end + 15) & !15;
    let size = (stubs + STUB * n).div_ceil(PAGE) * PAGE;

    let base = map_image(size);
    if base == libc::MAP_FAILED {
        // Repli : l'ancienne page « MZ » seule.
        return super::fake_module();
    }
    let img = unsafe { std::slice::from_raw_parts_mut(base as *mut u8, size) };

    // En-tête DOS.
    img[0] = b'M';
    img[1] = b'Z';
    put32(img, 0x3c, 0x40);
    // En-tête PE.
    img[0x40..0x44].copy_from_slice(b"PE\0\0");
    let fh = 0x44;
    put16(img, fh, 0x8664);
    put16(img, fh + 2, 1); // une section
    put16(img, fh + 16, 0xF0);
    put16(img, fh + 18, 0x2022); // EXECUTABLE | LARGE_ADDRESS_AWARE | DLL
    let oh = fh + 20;
    put16(img, oh, 0x20b);
    put32(img, oh + 4, (size - PAGE) as u32); // SizeOfCode
    put32(img, oh + 20, PAGE as u32); // BaseOfCode
    put64(img, oh + 24, base as u64); // ImageBase
    put32(img, oh + 32, PAGE as u32);
    put32(img, oh + 36, 0x200);
    put16(img, oh + 40, 10); // MajorOperatingSystemVersion
    put16(img, oh + 48, 10); // MajorSubsystemVersion
    put32(img, oh + 56, size as u32); // SizeOfImage
    put32(img, oh + 60, PAGE as u32); // SizeOfHeaders
    put16(img, oh + 68, 3); // console
    put16(img, oh + 70, 0x160); // HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT
    put64(img, oh + 72, 0x40000);
    put64(img, oh + 80, 0x1000);
    put64(img, oh + 88, 0x100000);
    put64(img, oh + 96, 0x1000);
    put32(img, oh + 108, 16);
    put32(img, oh + 112, dir as u32); // exports
    put32(img, oh + 116, (export_end - dir) as u32);
    // Section unique.
    let sh = oh + 0xF0;
    img[sh..sh + 5].copy_from_slice(b".text");
    put32(img, sh + 8, (size - PAGE) as u32);
    put32(img, sh + 12, PAGE as u32);
    put32(img, sh + 16, (size - PAGE) as u32);
    put32(img, sh + 20, PAGE as u32);
    put32(img, sh + 36, 0x6000_0020); // CODE | EXECUTE | READ

    // Répertoire d'exports.
    put32(img, dir + 12, dll_name_rva as u32);
    put32(img, dir + 16, 1); // Base des ordinaux
    put32(img, dir + 20, n as u32);
    put32(img, dir + 24, n as u32);
    put32(img, dir + 28, funcs as u32);
    put32(img, dir + 32, name_ptrs as u32);
    put32(img, dir + 36, ords as u32);
    img[dll_name_rva..dll_name_rva + dll_name.len()].copy_from_slice(dll_name.as_bytes());
    let mut services = Vec::new();
    for (i, ((name, addr), name_rva)) in names.iter().zip(&name_rvas).enumerate() {
        let stub = stubs + STUB * i;
        put32(img, funcs + 4 * i, stub as u32);
        put32(img, name_ptrs + 4 * i, *name_rva as u32);
        put16(img, ords + 2 * i, i as u16);
        img[*name_rva..*name_rva + name.len()].copy_from_slice(name.as_bytes());
        if is_service(dll, name) {
            // Service N = rang de l'export : identique dans chaque processus.
            if services.len() <= i {
                services.resize(i + 1, 0);
            }
            services[i] = *addr;
            img[stub..stub + STUB].copy_from_slice(&service_stub(i as u32));
            continue;
        }
        // jmp qword ptr [rip+2] ; ud2 ; .quad addr
        img[stub..stub + 6].copy_from_slice(&[0xFF, 0x25, 0x02, 0, 0, 0]);
        img[stub + 6..stub + 8].copy_from_slice(&[0x0F, 0x0B]);
        put64(img, stub + 8, *addr as u64);
    }
    if !services.is_empty() {
        let _ = SERVICES.set(services);
    }
    unsafe {
        libc::mprotect(base, PAGE, libc::PROT_READ);
        libc::mprotect(
            (base as usize + PAGE) as *mut libc::c_void,
            size - PAGE,
            libc::PROT_READ | libc::PROT_EXEC,
        );
    }
    base as usize
}

#[cfg(test)]
mod tests {
    /// Lecture de la table d'exports comme le fait le bac à sable de
    /// Chromium : en-têtes PE, répertoire, recherche par nom.
    unsafe fn manual_lookup(base: usize, wanted: &str) -> Option<usize> {
        let rd32 = |a: usize| (a as *const u32).read_unaligned() as usize;
        let rd16 = |a: usize| (a as *const u16).read_unaligned() as usize;
        if rd16(base) != 0x5A4D {
            return None;
        }
        let pe = base + rd32(base + 0x3c);
        if rd32(pe) != 0x4550 {
            return None;
        }
        let oh = pe + 24;
        let dir = base + rd32(oh + 112);
        let n = rd32(dir + 24);
        for i in 0..n {
            let name_ptr = base + rd32(base + rd32(dir + 32) + 4 * i);
            let name = std::ffi::CStr::from_ptr(name_ptr as *const std::ffi::c_char);
            if name.to_bytes() == wanted.as_bytes() {
                let ord = rd16(base + rd32(dir + 36) + 2 * i);
                return Some(base + rd32(base + rd32(dir + 28) + 4 * ord));
            }
        }
        None
    }

    extern "win64" fn add(a: u32, b: u32) -> u32 {
        a + b
    }

    #[test]
    fn table_d_exports_lisible_et_relais_appelable() {
        let mut e: super::super::Exports = std::collections::HashMap::new();
        e.insert("WinluxAdd", add as *const () as usize);
        static DATA: u32 = 7;
        e.insert("WinluxData", std::ptr::addr_of!(DATA) as usize);
        let base = super::build("winluxtest", &e);
        unsafe {
            let f = manual_lookup(base, "WinluxAdd").expect("export trouvé");
            assert!(f > base);
            let f: extern "win64" fn(u32, u32) -> u32 = std::mem::transmute(f);
            assert_eq!(f(40, 2), 42);
            assert!(manual_lookup(base, "WinluxData").is_none());
            assert!(manual_lookup(base, "Absent").is_none());
        }
    }

    /// Relais Nt* de ntdll (#767) : forme vérifiée par le bac à sable de
    /// Chromium, adresse fixe, et appel réel par `int 2Eh`, depuis l'image
    /// comme depuis une copie du relais.
    #[test]
    fn relais_de_service_au_format_windows() {
        let ntdll = super::super::module_handle("ntdll").expect("ntdll intégrée");
        assert!(ntdll >= super::IMAGES_FIXED && ntdll < crate::process::PEB_FIXED);
        let stub = unsafe { manual_lookup(ntdll, "NtClose") }.expect("NtClose exporté");
        let bytes = unsafe { std::slice::from_raw_parts(stub as *const u8, super::STUB) };
        assert_eq!(&bytes[..4], &[0x4C, 0x8B, 0xD1, 0xB8]);
        assert_eq!(
            &bytes[8..16],
            &[0xF6, 0x04, 0x25, 0x08, 0x03, 0xFE, 0x7F, 0x01]
        );
        assert_eq!(
            &bytes[16..24],
            &[0x75, 0x03, 0x0F, 0x05, 0xC3, 0xCD, 0x2E, 0xC3]
        );
        let id = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(
            super::SERVICES.get().and_then(|t| t.get(id)).copied(),
            Some(super::super::ntdll::nt_close as *const () as usize)
        );
        // Les autres fonctions gardent le relais par saut.
        let rtl = unsafe { manual_lookup(ntdll, "RtlInitializeSRWLock") }.expect("export Rtl");
        assert_eq!(unsafe { *(rtl as *const u16) }, 0x25FF);

        // Appel réel dans un processus neuf, seul à poser son gestionnaire :
        // le binaire de test relancé sur ce seul test (#1984). Un fork de
        // l'exécuteur multithread pouvait hériter d'un verrou tenu par un
        // autre test (tables de handles de NtClose) et rester bloqué.
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "dlls::pe_image::tests::relais_de_service_enfant",
                "--test-threads=1",
            ])
            .env(RELAIS_ENFANT, "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            if let Some(st) = child.try_wait().unwrap() {
                break st;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("processus enfant bloqué plus de 60 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(status.code(), Some(0), "processus enfant : {status:?}");
    }

    /// Variable qui fait de `relais_de_service_enfant` le processus enfant.
    const RELAIS_ENFANT: &str = "WINLUX_TEST_RELAIS_ENFANT";

    /// Partie enfant de `relais_de_service_au_format_windows` : ne fait
    /// rien dans l'exécuteur ordinaire, appelle le relais dans le processus
    /// neuf lancé par ce test, et sort avec 0 si tout est conforme.
    #[test]
    fn relais_de_service_enfant() {
        if std::env::var_os(RELAIS_ENFANT).is_none() {
            return;
        }
        let ntdll = super::super::module_handle("ntdll").expect("ntdll intégrée");
        let stub = unsafe { manual_lookup(ntdll, "NtClose") }.expect("NtClose exporté");
        extern "C" fn on_segv(_: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
            unsafe {
                if !super::redirect_service(info, ctx.cast()) {
                    libc::_exit(90);
                }
            }
        }
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_segv as *const () as usize;
            sa.sa_flags = libc::SA_SIGINFO;
            libc::sigaction(libc::SIGSEGV, &sa, std::ptr::null_mut());
            // Sans la page SharedUserData, la faute tombe sur le `test`.
            let f: extern "win64" fn(usize) -> u32 = std::mem::transmute(stub);
            let absent = f(0xDEAD_0000);
            super::map_shared_user_data();
            let present = f(0xDEAD_0000);
            // Copie du relais ailleurs, comme le fait le courtier.
            let copy = libc::mmap(
                std::ptr::null_mut(),
                0x1000,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            std::ptr::copy_nonoverlapping(stub as *const u8, copy as *mut u8, super::STUB);
            let g: extern "win64" fn(usize) -> u32 = std::mem::transmute(copy);
            let copied = g(0xDEAD_0000);
            let invalid = 0xC000_0008; // STATUS_INVALID_HANDLE
            let ok = absent == invalid && present == invalid && copied == invalid;
            libc::_exit(if ok { 0 } else { 1 });
        }
    }
}
