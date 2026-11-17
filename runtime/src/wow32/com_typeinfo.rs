//! TYPEATTR et FUNCDESC au passage de la frontière x86 ↔ x64 (#2796).
//!
//! `ITypeInfo::GetTypeAttr` et `GetFuncDesc` rendent l'adresse d'une
//! structure que l'appelant relit puis rend par `ReleaseTypeAttr` /
//! `ReleaseFuncDesc`. Les bibliothèques de types du runtime
//! (oleaut32_typelib.rs) les écrivent en disposition x64 ; un programme x86
//! (puits d'événements ATL de l'installeur Creative Cloud, qui lit la
//! dispinterface DWebBrowserEvents2) les lit en disposition x86. Chaque
//! structure rendue reçoit donc une image x86, gardée avec l'adresse de
//! l'original jusqu'au Release*, qui libère les deux.
//!
//! Dispositions d'après les déclarations publiques d'oaidl.h (SDK Windows ;
//! mêmes déclarations dans mingw-w64), pointeurs de 4 octets, alignement
//! naturel :
//!
//! ```text
//!                 x64                     x86
//! TYPEDESC        16 (union 0, vt 8)      8 (union 0, vt 4)
//! ARRAYDESC       20 + 8/dim              12 + 8/dim (cDims à 8)
//! PARAMDESC       16 (ex 0, flags 8)      8 (ex 0, flags 4)
//! PARAMDESCEX     32 (VARIANT à 8)        24 (VARIANT à 8)
//! ELEMDESC        32                      16
//! TYPEATTR        96                      76 (lpstrSchema 32, cbSizeInstance 36,
//!                                            typekind 40, cFuncs 44…, tdescAlias 60,
//!                                            idldescType 68)
//! FUNCDESC        88                      52 (lprgscode 4, lprgelemdescParam 8,
//!                                            funckind 12…, elemdescFunc 32,
//!                                            wFuncFlags 48)
//! ```
//!
//! Les tailles qui dépendent de celle d'un pointeur sont ramenées à
//! 4 octets : cbSizeVft et oVft (cases de table de fonctions),
//! cbSizeInstance et cbAlignment d'une interface.

use super::com_disp::Low;
use std::collections::HashMap;
use std::sync::Mutex;

const VT_PTR: u16 = 26;
const VT_SAFEARRAY: u16 = 27;
const VT_CARRAY: u16 = 28;
const PARAMFLAG_FHASDEFAULT: u16 = 0x20;
const TKIND_INTERFACE: u32 = 3;
const TKIND_DISPATCH: u32 = 4;

/// Structure rendue par une méthode d'ITypeInfo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Desc {
    TypeAttr,
    FuncDesc,
}

/// Images x86 en service : adresse x86 → (tampon, structure x64 d'origine).
fn images() -> &'static Mutex<HashMap<u32, (Low, u64)>> {
    static M: std::sync::OnceLock<Mutex<HashMap<u32, (Low, u64)>>> = std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

/// Image x86 en construction : octets, pointeurs internes à reloger
/// (champ, cible) et VARIANT de valeurs par défaut à traduire une fois
/// l'adresse connue (VARIANT x64, position de l'image).
#[derive(Default)]
struct Img {
    b: Vec<u8>,
    relocs: Vec<(usize, usize)>,
    variants: Vec<(u64, usize)>,
}

impl Img {
    fn alloc(&mut self, n: usize, align: usize) -> usize {
        let off = self.b.len().next_multiple_of(align);
        self.b.resize(off + n, 0);
        off
    }
    fn u16(&mut self, at: usize, v: u16) {
        self.b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, at: usize, v: u32) {
        self.b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// TYPEDESC x64 à `src` → 8 octets à `dst`.
    unsafe fn tdesc(&mut self, src: *const u8, dst: usize, depth: u32) {
        let vt: u16 = at(src, 8);
        self.u16(dst + 4, vt);
        let inner: *const u8 = at(src, 0);
        match vt {
            VT_PTR | VT_SAFEARRAY if !inner.is_null() && depth < 16 => {
                let t = self.alloc(8, 4);
                self.tdesc(inner, t, depth + 1);
                self.relocs.push((dst, t));
            }
            VT_CARRAY if !inner.is_null() && depth < 16 => {
                let dims = at::<u16>(inner, 16).min(64) as usize;
                let t = self.alloc(12 + 8 * dims, 4);
                self.tdesc(inner, t, depth + 1);
                self.u16(t + 8, dims as u16);
                for i in 0..dims {
                    let b = inner.add(20 + 8 * i);
                    self.u32(t + 12 + 8 * i, at(b, 0));
                    self.u32(t + 16 + 8 * i, at(b, 4));
                }
                self.relocs.push((dst, t));
            }
            VT_PTR | VT_SAFEARRAY | VT_CARRAY => {}
            // HREFTYPE de VT_USERDEFINED, rien pour les autres.
            _ => self.u32(dst, at(src, 0)),
        }
    }

    /// ELEMDESC x64 à `src` → 16 octets à `dst`.
    unsafe fn elem(&mut self, src: *const u8, dst: usize) {
        self.tdesc(src, dst, 0);
        let flags: u16 = at(src, 24);
        self.u16(dst + 12, flags);
        let ex: u64 = at(src, 16);
        if flags & PARAMFLAG_FHASDEFAULT != 0 && ex != 0 {
            let t = self.alloc(24, 8);
            self.u32(t, 24);
            self.variants.push((ex + 8, t + 8));
            self.relocs.push((dst + 8, t));
        }
    }

    /// Pose l'image en mémoire basse et l'inscrit ; rend son adresse x86.
    unsafe fn place(self, original: u64) -> u32 {
        let words = self.b.len().div_ceil(4);
        let low = Low::new(words);
        let base = low.at(0);
        let p = base as usize as *mut u8;
        std::ptr::copy_nonoverlapping(self.b.as_ptr(), p, self.b.len());
        for (field, target) in self.relocs {
            (p.add(field) as *mut u32).write_unaligned(base + target as u32);
        }
        for (v, at) in self.variants {
            super::com::variant32(v as usize as *const u8, base + at as u32);
        }
        images()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(base, (low, original));
        base
    }
}

/// Lit `T` à `src + off`, sans supposer d'alignement : toutes les
/// lectures des structures x64 passent par ici (#2796).
unsafe fn at<T: Copy>(src: *const u8, off: usize) -> T {
    (src.add(off) as *const T).read_unaligned()
}

/// TYPEATTR x64 → image x86 (76 octets).
unsafe fn type_attr(src: *const u8) -> Img {
    let mut m = Img::default();
    let a = m.alloc(76, 4);
    m.b[a..a + 16].copy_from_slice(std::slice::from_raw_parts(src, 16));
    for off in [16, 20, 24, 28] {
        m.u32(a + off, at(src, off));
    }
    m.u32(a + 32, at::<u64>(src, 32) as u32); // lpstrSchema
    let kind: u32 = at(src, 44);
    let iface = matches!(kind, TKIND_INTERFACE | TKIND_DISPATCH);
    let instance: u32 = at(src, 40);
    m.u32(a + 36, if iface && instance == 8 { 4 } else { instance });
    m.u32(a + 40, kind);
    for (from, to) in [(48, 44), (50, 46), (52, 48)] {
        m.u16(a + to, at(src, from));
    }
    m.u16(a + 50, at::<u16>(src, 54) / 2); // cbSizeVft
    let align: u16 = at(src, 56);
    m.u16(a + 52, if iface && align == 8 { 4 } else { align });
    for (from, to) in [(58, 54), (60, 56), (62, 58)] {
        m.u16(a + to, at(src, from));
    }
    m.tdesc(src.add(64), a + 60, 0);
    m.u32(a + 68, at::<u64>(src, 80) as u32);
    m.u16(a + 72, at(src, 88));
    m
}

/// FUNCDESC x64 → image x86 (52 octets).
unsafe fn func_desc(src: *const u8) -> Img {
    let mut m = Img::default();
    let a = m.alloc(52, 4);
    m.u32(a, at(src, 0));
    let params = at::<i16>(src, 36).max(0) as usize;
    let scodes = at::<i16>(src, 42).max(0) as usize;
    let sc: *const u32 = at(src, 8);
    if scodes > 0 && !sc.is_null() {
        let t = m.alloc(4 * scodes, 4);
        for i in 0..scodes {
            m.u32(t + 4 * i, sc.add(i).read_unaligned());
        }
        m.relocs.push((a + 4, t));
    }
    let ps: *const u8 = at(src, 16);
    if params > 0 && !ps.is_null() {
        let t = m.alloc(16 * params, 4);
        for i in 0..params {
            m.elem(ps.add(32 * i), t + 16 * i);
        }
        m.relocs.push((a + 8, t));
    }
    for (from, to) in [(24, 12), (28, 16), (32, 20)] {
        m.u32(a + to, at(src, from));
    }
    m.u16(a + 24, at(src, 36));
    m.u16(a + 26, at(src, 38));
    m.u16(a + 28, (at::<i16>(src, 40) / 2) as u16); // oVft
    m.u16(a + 30, at(src, 42));
    m.elem(src.add(48), a + 32);
    m.u16(a + 48, at(src, 80));
    m
}

/// Structure x64 rendue à `src` → adresse de son image x86 (0 si `src` est
/// nul).
pub(super) unsafe fn to32(kind: Desc, src: u64) -> u32 {
    if src == 0 {
        return 0;
    }
    let p = src as usize as *const u8;
    let img = match kind {
        Desc::TypeAttr => type_attr(p),
        Desc::FuncDesc => func_desc(p),
    };
    img.place(src)
}

/// Release* d'une image x86 : la libère et rend la structure x64 d'origine,
/// que la méthode du runtime libère à son tour. Une adresse inconnue est
/// rendue telle quelle (le runtime ignore un bloc qu'il n'a pas donné).
pub(super) fn take64(x86: u32) -> u64 {
    match images()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&x86)
    {
        Some((_low, original)) => original,
        None => x86 as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn r32(a: u32) -> u32 {
        (a as usize as *const u32).read_unaligned()
    }
    unsafe fn r16(a: u32) -> u16 {
        (a as usize as *const u16).read_unaligned()
    }

    #[test]
    fn funcdesc_et_typeattr_en_disposition_x86_2796() {
        unsafe {
            // FUNCDESC x64 : memid 259, deux paramètres (VT_DISPATCH,
            // VT_PTR → VT_VARIANT), FUNC_DISPATCH, retour VT_VOID.
            let mut inner = [0u8; 16];
            inner[8..10].copy_from_slice(&12u16.to_le_bytes());
            let mut params = [0u8; 64];
            params[8..10].copy_from_slice(&9u16.to_le_bytes());
            params[24..26].copy_from_slice(&1u16.to_le_bytes());
            params[32..40].copy_from_slice(&(inner.as_ptr() as u64).to_le_bytes());
            params[40..42].copy_from_slice(&26u16.to_le_bytes());
            params[56..58].copy_from_slice(&1u16.to_le_bytes());
            let mut f = [0u8; 88];
            f[0..4].copy_from_slice(&259i32.to_le_bytes());
            f[16..24].copy_from_slice(&(params.as_ptr() as u64).to_le_bytes());
            f[24..28].copy_from_slice(&4u32.to_le_bytes());
            f[28..32].copy_from_slice(&1u32.to_le_bytes());
            f[32..36].copy_from_slice(&4u32.to_le_bytes());
            f[36..38].copy_from_slice(&2i16.to_le_bytes());
            f[56..58].copy_from_slice(&24u16.to_le_bytes());
            let x = to32(Desc::FuncDesc, f.as_ptr() as u64);
            assert_ne!(x, 0);
            assert_eq!(r32(x), 259);
            assert_eq!(r32(x + 12), 4, "funckind");
            assert_eq!(r32(x + 16), 1, "invkind");
            assert_eq!(r32(x + 20), 4, "callconv");
            assert_eq!(r16(x + 24), 2, "cParams");
            assert_eq!(r16(x + 32 + 4), 24, "retour VT_VOID");
            let p = r32(x + 8);
            assert_eq!(r16(p + 4), 9, "pDisp VT_DISPATCH");
            assert_eq!(r16(p + 12), 1, "PARAMFLAG_FIN");
            assert_eq!(r16(p + 16 + 4), 26, "URL VT_PTR");
            assert_eq!(r16(r32(p + 16) + 4), 12, "… vers VT_VARIANT");
            assert_eq!(take64(x), f.as_ptr() as u64);

            // TYPEATTR x64 d'une dispinterface : 7 méthodes d'IDispatch.
            let mut t = [0u8; 96];
            t[0] = 0xa0;
            t[40..44].copy_from_slice(&8u32.to_le_bytes());
            t[44..48].copy_from_slice(&TKIND_DISPATCH.to_le_bytes());
            t[48..50].copy_from_slice(&41u16.to_le_bytes());
            t[52..54].copy_from_slice(&1u16.to_le_bytes());
            t[54..56].copy_from_slice(&56u16.to_le_bytes());
            t[56..58].copy_from_slice(&8u16.to_le_bytes());
            t[58..60].copy_from_slice(&0x10u16.to_le_bytes());
            t[72..74].copy_from_slice(&0u16.to_le_bytes());
            let x = to32(Desc::TypeAttr, t.as_ptr() as u64);
            assert_eq!(*(x as usize as *const u8), 0xa0);
            assert_eq!(r32(x + 36), 4, "cbSizeInstance");
            assert_eq!(r32(x + 40), TKIND_DISPATCH);
            assert_eq!(r16(x + 44), 41, "cFuncs");
            assert_eq!(r16(x + 48), 1, "cImplTypes");
            assert_eq!(r16(x + 50), 28, "cbSizeVft");
            assert_eq!(r16(x + 52), 4, "cbAlignment");
            assert_eq!(r16(x + 54), 0x10, "wTypeFlags");
            assert_eq!(take64(x), t.as_ptr() as u64);
            assert_eq!(take64(x), x as u64, "image déjà rendue");
        }
    }
}
