//! Pont P/Invoke : un `[DllImport]` managé est résolu dans la table d'exports
//! des DLL intégrées de Winlux (`dlls::lookup`) puis appelé avec la convention
//! Win64, exactement comme le ferait un programme natif.

pub struct Import<'a> {
    pub module: &'a str,
    pub name: &'a str,
    /// CharSet.Unicode / Auto : chaînes en UTF-16 et suffixe « W ».
    pub unicode: bool,
}

/// Argument déjà préparé par l'interpréteur.
#[derive(Debug, Clone)]
pub enum Arg {
    /// Entier, pointeur, booléen, structure de 1, 2, 4 ou 8 octets.
    Word(u64),
    F32(f32),
    F64(f64),
    /// Chaîne copiée dans un tampon natif le temps de l'appel.
    Str(String),
}

/// Valeur de retour brute : RAX et XMM0.
pub struct Raw {
    pub int: u64,
    pub float: u64,
}

thread_local! {
    /// Dernière erreur Win32 relevée après un appel `SetLastError = true`.
    pub static LAST_ERROR: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Adresse de la fonction native d'un `[DllImport]`.
pub fn resolve(import: &Import) -> Result<usize, (&'static str, String)> {
    let dll = crate::dlls::normalize(import.module);
    if !crate::dlls::is_builtin(&dll) {
        return Err((
            "DllNotFoundException",
            format!("Unable to load DLL '{}'", import.module),
        ));
    }
    let suffixed = format!("{}{}", import.name, if import.unicode { 'W' } else { 'A' });
    crate::dlls::lookup(&dll, import.name)
        .or_else(|| crate::dlls::lookup(&dll, &suffixed))
        .ok_or((
            "EntryPointNotFoundException",
            format!(
                "Unable to find an entry point named '{}' in DLL '{}'",
                import.name, import.module
            ),
        ))
}

/// Appelle `addr` avec la convention Win64. Les chaînes vivent dans des
/// tampons jusqu'au retour.
pub fn call(addr: usize, args: &[Arg], unicode: bool, last_error: bool) -> Raw {
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    let words: Vec<u64> = args
        .iter()
        .map(|a| match a {
            Arg::Word(w) => *w,
            Arg::F32(f) => f.to_bits() as u64,
            Arg::F64(f) => f.to_bits(),
            Arg::Str(text) => {
                let buf: Vec<u8> = if unicode {
                    text.encode_utf16()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect()
                } else {
                    text.bytes().chain([0]).collect()
                };
                buffers.push(buf);
                buffers.last().unwrap().as_ptr() as u64
            }
        })
        .collect();
    crate::trace!("P/Invoke {:#x} {:x?}", addr, words);
    let raw = unsafe { call_win64(addr, &words) };
    if last_error {
        if let Some(get) = crate::dlls::lookup("kernel32", "GetLastError") {
            let get: extern "win64" fn() -> u32 = unsafe { std::mem::transmute(get) };
            LAST_ERROR.with(|e| e.set(get()));
        }
    }
    drop(buffers);
    raw
}

/// Chaîne native terminée par un zéro.
pub fn read_native_string(ptr: u64, unicode: bool) -> String {
    if ptr == 0 {
        return String::new();
    }
    if unicode {
        let p = ptr as *const u16;
        let len = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
    } else {
        unsafe { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char) }
            .to_string_lossy()
            .into_owned()
    }
}

/// Appel Win64 générique : les quatre premiers arguments vont à la fois
/// dans RCX, RDX, R8, R9 et dans XMM0 à XMM3 (l'appelé lit le registre de
/// son type), les suivants sur la pile après les 32 octets réservés.
///
/// # Safety
/// `addr` doit être une fonction Win64 qui accepte ces arguments.
pub unsafe fn call_win64(addr: usize, args: &[u64]) -> Raw {
    let mut regs = [0u64; 4];
    for (r, a) in regs.iter_mut().zip(args) {
        *r = *a;
    }
    // Zone réservée (4 mots) puis arguments de pile.
    let mut stack = vec![0u64; 4];
    stack.extend(args.iter().skip(4));
    let int: u64;
    let float: u64;
    std::arch::asm!(
        "mov r12, rsp",
        "mov rax, rcx",
        "shl rax, 3",
        "sub rsp, rax",
        "and rsp, -16",
        "mov rdi, rsp",
        "rep movsq",
        "mov rcx, [r13]",
        "mov rdx, [r13 + 8]",
        "mov r8, [r13 + 16]",
        "mov r9, [r13 + 24]",
        "movq xmm0, rcx",
        "movq xmm1, rdx",
        "movq xmm2, r8",
        "movq xmm3, r9",
        "call r14",
        "mov rsp, r12",
        "movq rdx, xmm0",
        inout("rcx") stack.len() => _,
        inout("rsi") stack.as_ptr() => _,
        out("rdi") _,
        out("r12") _,
        in("r13") regs.as_ptr(),
        in("r14") addr,
        out("rax") int,
        out("rdx") float,
        clobber_abi("win64"),
    );
    Raw { int, float }
}
