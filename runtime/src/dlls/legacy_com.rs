//! Objets COM « deux ABI » des API de jeux anciens (#237) : DirectDraw et
//! Direct3D 8.
//!
//! Ces API servent surtout à des programmes 32 bits. Plutôt que de passer
//! par les mandataires génériques de wow32 (qui ne savent pas convertir
//! DDSURFACEDESC, DDBLTFX, D3DLOCKED_RECT…), les objets lisent eux-mêmes la
//! disposition de leurs structures selon le programme en cours, et leur
//! table de méthodes est celle de son ABI : cases de 8 octets win64, ou
//! passerelles x86 stdcall (`wow32::x86_vtable`) dans un programme 32 bits.
//! Un même objet n'est jamais vu des deux côtés : un processus est soit
//! 32 bits soit 64 bits.

use std::collections::HashMap;
use std::sync::Mutex;

/// Le programme en cours est-il un programme x86 ?
pub fn x86() -> bool {
    crate::wow32::active()
}

/// Taille d'un pointeur du programme.
pub fn ptr_size() -> usize {
    if x86() {
        4
    } else {
        8
    }
}

/// Écrit un pointeur rendu au programme (4 ou 8 octets).
///
/// # Safety
/// `at` est une case de pointeur valide du programme, ou 0.
pub unsafe fn put_ptr(at: usize, v: usize) {
    if at == 0 {
        return;
    }
    if x86() {
        (at as *mut u32).write_unaligned(v as u32);
    } else {
        (at as *mut usize).write_unaligned(v);
    }
}

/// Lit un pointeur dans une structure du programme.
///
/// # Safety
/// `at` est une case de pointeur valide.
pub unsafe fn get_ptr(at: usize) -> usize {
    if x86() {
        (at as *const u32).read_unaligned() as usize
    } else {
        (at as *const usize).read_unaligned()
    }
}

/// Lit et écrit des DWORD à une adresse du programme.
pub unsafe fn rd32(at: usize) -> u32 {
    (at as *const u32).read_unaligned()
}
pub unsafe fn wr32(at: usize, v: u32) {
    (at as *mut u32).write_unaligned(v)
}

/// Appelle une fonction de rappel du programme (convention win64, ou
/// stdcall x86 dans un programme 32 bits). Arguments de la taille d'un
/// pointeur, au plus six.
///
/// # Safety
/// `f` est une fonction du programme qui attend ces arguments.
pub unsafe fn call_back(f: usize, args: &[usize]) -> usize {
    if x86() {
        let a: Vec<u32> = args.iter().map(|&v| v as u32).collect();
        return crate::wow32::call32(f, &a) as u32 as usize;
    }
    let mut a = [0usize; 6];
    a[..args.len().min(6)].copy_from_slice(&args[..args.len().min(6)]);
    let g: unsafe extern "win64" fn(usize, usize, usize, usize, usize, usize) -> usize =
        std::mem::transmute(f);
    g(a[0], a[1], a[2], a[3], a[4], a[5])
}

/// Table de méthodes de l'interface `name` pour l'ABI du programme :
/// `methods[i]` = (fonction win64, nombre d'arguments après `this`).
/// Construite une fois par interface.
pub fn vtable(name: &'static str, methods: &[(usize, u8)]) -> usize {
    static CACHE: Mutex<Option<HashMap<(&'static str, bool), usize>>> = Mutex::new(None);
    let x = x86();
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(&v) = map.get(&(name, x)) {
        return v;
    }
    let v = if x {
        let v = crate::wow32::x86_vtable(name, methods) as usize;
        let mut t = X86_TABLES.lock().unwrap_or_else(|e| e.into_inner());
        t.get_or_insert_with(HashMap::new)
            .insert(v, methods.iter().map(|m| m.0).collect());
        v
    } else {
        let t: Vec<usize> = methods.iter().map(|m| m.0).collect();
        Box::leak(t.into_boxed_slice()).as_ptr() as usize
    };
    map.insert((name, x), v);
    v
}

/// Table x86 de passerelles pour une table win64 existante. `methods[i]`
/// = (fonction appelée par la passerelle, fonction win64 native de la
/// méthode, signature : voir `wow32::x86_vtable_sig`). La passerelle peut
/// viser une variante qui convertit une structure x86 ; `method` rend la
/// fonction native, appelée par le runtime avec ses propres structures.
/// Construite une fois par nom.
pub fn x86_table(name: &'static str, methods: &[(usize, usize, &str)]) -> usize {
    static CACHE: Mutex<Option<HashMap<&'static str, usize>>> = Mutex::new(None);
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(&v) = map.get(name) {
        return v;
    }
    let stubs: Vec<(usize, &str)> = methods.iter().map(|m| (m.0, m.2)).collect();
    let v = crate::wow32::x86_vtable_sig(name, &stubs) as usize;
    let mut t = X86_TABLES.lock().unwrap_or_else(|e| e.into_inner());
    t.get_or_insert_with(HashMap::new)
        .insert(v, methods.iter().map(|m| m.1).collect());
    map.insert(name, v);
    v
}

/// Tables x86 construites par `vtable` → leurs fonctions win64.
static X86_TABLES: Mutex<Option<HashMap<usize, Vec<usize>>>> = Mutex::new(None);

/// Fonction win64 de la méthode `idx` d'un objet COM du runtime, quelle que
/// soit l'ABI de sa table : une table x86 de passerelles (`vtable` dans un
/// programme 32 bits) est retrouvée par son adresse, une table win64 est lue
/// telle quelle. Sert aux couches qui en appellent d'autres (D3DX et
/// Direct3D 8 sur Direct3D 9).
///
/// # Safety
/// `obj` est un objet COM valide créé par le runtime.
pub unsafe fn method(obj: usize, idx: usize) -> usize {
    if x86() {
        let vt = rd32(obj) as usize;
        let t = X86_TABLES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = t.as_ref().and_then(|m| m.get(&vt)).and_then(|m| m.get(idx)) {
            return *f;
        }
    }
    let vt = (obj as *const usize).read_unaligned();
    *(vt as *const usize).add(idx)
}

/// Appelle la méthode `idx` d'un objet COM du runtime (voir `method`).
#[macro_export]
macro_rules! com_call {
    ($obj:expr, $idx:expr, fn($($t:ty),*) -> $r:ty $(, $a:expr)* $(,)?) => {{
        let o: usize = $obj;
        let f: unsafe extern "win64" fn(usize $(, $t)*) -> $r =
            std::mem::transmute($crate::dlls::legacy_com::method(o, $idx));
        f(o $(, $a)*)
    }};
}

/// Lit un D3DLOCKED_RECT { INT Pitch; void *pBits; } rempli par une
/// méthode native du runtime (obtenue par `method`, disposition 64 bits
/// quel que soit le programme) : (pas, adresse).
///
/// # Safety
/// `at` pointe une telle structure remplie.
pub unsafe fn locked_rect(at: usize) -> (i32, usize) {
    (
        rd32(at) as i32,
        (at as *const usize).add(1).read_unaligned(),
    )
}

/// Écrit une chaîne ANSI terminée par 0 dans un tampon de `cap` octets.
pub unsafe fn put_str(at: usize, cap: usize, s: &str) {
    if at == 0 || cap == 0 {
        return;
    }
    let b = crate::dlls::encode_ansi(s);
    let n = b.len().min(cap - 1);
    std::ptr::copy_nonoverlapping(b.as_ptr(), at as *mut u8, n);
    *((at + n) as *mut u8) = 0;
}
