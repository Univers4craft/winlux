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

const PAGE: usize = 0x1000;
const STUB: usize = 16;

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

    let base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
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
    for (i, ((name, addr), name_rva)) in names.iter().zip(&name_rvas).enumerate() {
        let stub = stubs + STUB * i;
        put32(img, funcs + 4 * i, stub as u32);
        put32(img, name_ptrs + 4 * i, *name_rva as u32);
        put16(img, ords + 2 * i, i as u16);
        img[*name_rva..*name_rva + name.len()].copy_from_slice(name.as_bytes());
        // jmp qword ptr [rip+2] ; ud2 ; .quad addr
        img[stub..stub + 6].copy_from_slice(&[0xFF, 0x25, 0x02, 0, 0, 0]);
        img[stub + 6..stub + 8].copy_from_slice(&[0x0F, 0x0B]);
        put64(img, stub + 8, *addr as u64);
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
}
