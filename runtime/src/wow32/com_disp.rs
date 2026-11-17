//! DISPPARAMS au passage de la frontière x86 ↔ x64 (#2771).
//!
//! `IDispatch::Invoke` reçoit un `DISPPARAMS*` (oaidl.h, SDK Windows
//! public ; mêmes déclarations dans mingw-w64) :
//!
//! ```text
//! struct DISPPARAMS { VARIANTARG *rgvarg; DISPID *rgdispidNamedArgs;
//!                     UINT cArgs; UINT cNamedArgs; }
//! ```
//!
//! 16 octets en x86 (pointeurs à 0 et 4, compteurs à 8 et 12), 24 en x64
//! (pointeurs à 0 et 8, compteurs à 16 et 20). Les VARIANT font 16 octets
//! en x86 et 24 en x64. Un argument `VT_BYREF | VT_VARIANT` (l'URL de
//! BeforeNavigate2, NavigateComplete2, DocumentComplete) pointe vers un
//! autre VARIANT : il est recopié de l'autre côté puis relu au retour,
//! l'appelé pouvant l'écrire. Les autres `VT_BYREF` pointent des données
//! de même disposition (VARIANT_BOOL de Cancel, LONG) sous 4 Go : le
//! pointeur passe tel quel.
//!
//! Les tampons sont des `Vec` : le tas est sous 4 Go dans un processus
//! x86, et le tampon d'un `Vec` ne bouge pas quand le `Vec` est déplacé.

use super::com::{variant32, variant64, VT_BYREF, VT_VARIANT};
use super::{rd, wr};

/// Au-delà, les arguments sont ignorés (aucune méthode d'automation n'en
/// prend autant).
const MAX_ARGS: u32 = 64;

/// Tampon de mots de 32 bits que le code x86 lit : le tas du processus
/// (sous 4 Go) en service, une page MAP_32BIT dans les tests.
struct Low {
    p: *mut u32,
    n: usize,
}

impl Low {
    fn new(n: usize) -> Low {
        let n = n.max(1);
        #[cfg(not(test))]
        let p = Box::into_raw(vec![0u32; n].into_boxed_slice()) as *mut u32;
        #[cfg(test)]
        let p = unsafe {
            let p = libc::mmap(
                std::ptr::null_mut(),
                4 * n,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            );
            assert_ne!(p, libc::MAP_FAILED);
            p as *mut u32
        };
        Low { p, n }
    }

    /// Adresse x86 du mot `i`.
    fn at(&self, i: usize) -> u32 {
        debug_assert!(i < self.n);
        unsafe { self.p.add(i) as usize as u32 }
    }
}

impl Drop for Low {
    fn drop(&mut self) {
        #[cfg(not(test))]
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                self.p, self.n,
            )));
        }
        #[cfg(test)]
        unsafe {
            libc::munmap(self.p.cast(), 4 * self.n);
        }
    }
}

/// Image x86 d'un DISPPARAMS x64, gardée jusqu'au retour de l'appel :
/// en-tête (4 mots), VARIANT (4 mots chacun), VARIANT par référence
/// (4 mots chacun), DISPID nommés.
pub(super) struct Disp32 {
    buf: Low,
    /// (VARIANT x64 d'origine, sa copie x86) des arguments VT_BYREF|VT_VARIANT.
    back: Vec<(u64, u32)>,
}

impl Disp32 {
    pub(super) fn addr(&self) -> u32 {
        self.buf.at(0)
    }
}

/// DISPPARAMS x64 à `src` → image x86. `src` nul rend une image nulle.
pub(super) unsafe fn to32(src: u64) -> Option<Disp32> {
    if src == 0 {
        return None;
    }
    let s = src as usize as *const u8;
    let rgvarg = (s as *const u64).read_unaligned();
    let named = (s.add(8) as *const u64).read_unaligned();
    let n = if rgvarg == 0 {
        0
    } else {
        (s.add(16) as *const u32).read_unaligned().min(MAX_ARGS)
    };
    let m = if named == 0 {
        0
    } else {
        (s.add(20) as *const u32).read_unaligned().min(n)
    };
    let n = n as usize;
    let buf = Low::new(4 + 8 * n + m as usize);
    let mut back = Vec::new();
    for i in 0..n {
        let v = (rgvarg as usize + 24 * i) as *const u8;
        let d = buf.at(4 + 4 * i);
        let vt = (v as *const u16).read_unaligned();
        if vt == VT_BYREF | VT_VARIANT {
            let inner = (v.add(8) as *const u64).read_unaligned();
            std::ptr::copy_nonoverlapping(v, d as usize as *mut u8, 8);
            let x = if inner == 0 {
                0
            } else {
                let x = buf.at(4 + 4 * n + 4 * i);
                variant32(inner as usize as *const u8, x);
                back.push((inner, x));
                x
            };
            wr(d + 8, x);
            wr(d + 12, 0);
        } else {
            variant32(v, d);
        }
    }
    for j in 0..m as usize {
        let id = (named as usize as *const u32).add(j).read_unaligned();
        wr(buf.at(4 + 8 * n + j), id);
    }
    let h = buf.at(0);
    wr(h, if n == 0 { 0 } else { buf.at(4) });
    wr(h + 4, if m == 0 { 0 } else { buf.at(4 + 8 * n) });
    wr(h + 8, n as u32);
    wr(h + 12, m);
    Some(Disp32 { buf, back })
}

/// Après l'appel x86 : les VARIANT passés par référence sont relus.
pub(super) unsafe fn back64(d: &Disp32) {
    for &(inner, x) in &d.back {
        variant64(x, inner as usize as *mut u8);
    }
}

/// Image x64 d'un DISPPARAMS x86, gardée jusqu'au retour de l'appel.
pub(super) struct Disp64 {
    head: Vec<u64>,
    _vars: Vec<u64>,
    _nested: Vec<u64>,
    _named: Vec<i32>,
    /// (VARIANT x86 d'origine, sa copie x64) des arguments VT_BYREF|VT_VARIANT.
    back: Vec<(u32, usize)>,
}

impl Disp64 {
    pub(super) fn addr(&self) -> u64 {
        self.head.as_ptr() as u64
    }
}

/// DISPPARAMS x86 à `src` → image x64.
pub(super) unsafe fn to64(src: u32) -> Option<Disp64> {
    if src == 0 {
        return None;
    }
    let rgvarg = rd(src);
    let named = rd(src + 4);
    let n = if rgvarg == 0 {
        0
    } else {
        rd(src + 8).min(MAX_ARGS)
    };
    let m = if named == 0 { 0 } else { rd(src + 12).min(n) };
    let mut vars = vec![0u64; 3 * n as usize];
    let mut nested = vec![0u64; 3 * n as usize];
    let mut back = Vec::new();
    for i in 0..n as usize {
        let v = rgvarg + 16 * i as u32;
        let d = vars.as_mut_ptr().add(3 * i) as *mut u8;
        let vt = (v as usize as *const u16).read_unaligned();
        if vt == VT_BYREF | VT_VARIANT {
            let inner = rd(v + 8);
            std::ptr::copy_nonoverlapping(v as usize as *const u8, d, 8);
            let x = if inner == 0 {
                0
            } else {
                let x = nested.as_mut_ptr().add(3 * i) as *mut u8;
                variant64(inner, x);
                back.push((inner, x as usize));
                x as u64
            };
            (d.add(8) as *mut u64).write_unaligned(x);
        } else {
            variant64(v, d);
        }
    }
    let named: Vec<i32> = (0..m).map(|j| rd(named + 4 * j) as i32).collect();
    let mut head = vec![0u64; 3];
    if n != 0 {
        head[0] = vars.as_ptr() as u64;
    }
    if m != 0 {
        head[1] = named.as_ptr() as u64;
    }
    head[2] = n as u64 | (m as u64) << 32;
    Some(Disp64 {
        head,
        _vars: vars,
        _nested: nested,
        _named: named,
        back,
    })
}

/// Après l'appel du runtime : les VARIANT passés par référence sont
/// réécrits côté x86.
pub(super) unsafe fn back32(d: &Disp64) {
    for &(inner, x) in &d.back {
        variant32(x as *const u8, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VT_BSTR: u16 = 8;
    const VT_I4: u16 = 3;
    const VT_BOOL: u16 = 11;

    /// #2771 : un DISPPARAMS x64 à trois arguments (VT_I4, VT_BYREF|VT_BOOL,
    /// VT_BYREF|VT_VARIANT) et un argument nommé arrive en x86 à la
    /// disposition de 16 octets, et le VARIANT passé par référence revient.
    #[test]
    fn dispparams_aller_retour_x86() {
        unsafe {
            let mut cancel: i16 = 0;
            let mut inner = [0u64; 3];
            *(inner.as_mut_ptr() as *mut u16) = VT_I4;
            inner[1] = 7;
            let mut vars = [[0u64; 3]; 3];
            *(vars[0].as_mut_ptr() as *mut u16) = VT_I4;
            vars[0][1] = 42;
            *(vars[1].as_mut_ptr() as *mut u16) = VT_BYREF | VT_BOOL;
            vars[1][1] = &mut cancel as *mut i16 as u64;
            *(vars[2].as_mut_ptr() as *mut u16) = VT_BYREF | VT_VARIANT;
            vars[2][1] = inner.as_mut_ptr() as u64;
            let named = [-3i32];
            let mut p = [0u64; 3];
            p[0] = vars.as_ptr() as u64;
            p[1] = named.as_ptr() as u64;
            p[2] = 3 | 1 << 32;
            let d = to32(p.as_ptr() as u64).unwrap();
            let a = d.addr();
            assert_eq!(rd(a + 8), 3);
            assert_eq!(rd(a + 12), 1);
            assert_eq!(rd(rd(a + 4)) as i32, -3);
            let v = rd(a);
            assert_eq!(rd(v) & 0xFFFF, VT_I4 as u32);
            assert_eq!(rd(v + 8), 42);
            assert_eq!(rd(v + 16) & 0xFFFF, (VT_BYREF | VT_BOOL) as u32);
            assert_eq!(rd(v + 24), &mut cancel as *mut i16 as usize as u32);
            assert_eq!(rd(v + 32) & 0xFFFF, (VT_BYREF | VT_VARIANT) as u32);
            let x = rd(v + 40);
            assert_eq!(rd(x) & 0xFFFF, VT_I4 as u32);
            assert_eq!(rd(x + 8), 7);
            // L'appelé x86 écrit dans le VARIANT par référence.
            wr(x, VT_BSTR as u32);
            wr(x + 8, 0x1234);
            back64(&d);
            assert_eq!(*(inner.as_ptr() as *const u16), VT_BSTR);
            assert_eq!(inner[1], 0x1234);
            // Et le retour en x64 rend la disposition de 24 octets.
            let e = to64(a).unwrap();
            let h = e.addr() as usize as *const u64;
            assert_eq!(*h.add(2), 3 | 1 << 32);
            let w = *h as usize as *const u64;
            assert_eq!(*w.add(1), 42);
            assert_eq!(*(w.add(6) as *const u16), VT_BYREF | VT_VARIANT);
            let y = *w.add(7) as usize as *mut u64;
            assert_eq!(*(y as *const u16), VT_BSTR);
            *y.add(1) = 0x5678;
            back32(&e);
            assert_eq!(rd(x + 8), 0x5678);
        }
    }
}
