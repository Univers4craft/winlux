//! Structures Win32 dont la disposition change entre x86 et x64 (#232).
//!
//! Une structure qui contient des pointeurs ou des handles n'a pas les
//! mêmes décalages en 32 et en 64 bits (OPENFILENAME, LVITEM, NMHDR…).
//! Chaque structure est décrite par la liste de ses champs, avec leur
//! décalage de chaque côté ; `to64` recopie une structure x86 dans un
//! tampon 64 bits avant l'appel, `to32` recopie le résultat au retour.

/// Un champ : décalage x86, décalage x64.
#[derive(Clone, Copy)]
pub(super) enum F {
    /// Entier de 4 octets (DWORD, int, COLORREF…).
    D(u16, u16),
    /// Entier de 2 octets.
    W(u16, u16),
    /// Octet.
    B(u16, u16),
    /// Pointeur ou handle : 4 octets ↔ 8 octets. Les valeurs
    /// 0xFFFF0000 et au-delà (LPSTR_TEXTCALLBACK, TVI_ROOT…) gardent leur
    /// signe, comme pour les arguments.
    P(u16, u16),
    /// Octets recopiés tels quels (RECT, POINT, LOGFONT…), de longueur donnée.
    N(u16, u16, u16),
    /// Champ « taille de la structure » : la taille x64 est écrite à l'aller,
    /// la valeur x86 est laissée telle quelle au retour.
    Size(u16, u16),
}

pub(super) struct Layout {
    pub size32: usize,
    pub size64: usize,
    pub fields: &'static [F],
}

/// Pointeur x86 étendu en 64 bits (signe gardé pour les constantes hautes).
pub(super) fn widen(v: u32) -> u64 {
    if v >= 0xffff_0000 {
        v as i32 as i64 as u64
    } else {
        v as u64
    }
}

impl Layout {
    /// Recopie la structure x86 `src` dans `dst` (au moins `size64` octets).
    ///
    /// # Safety
    /// `src` doit pointer sur `size32` octets lisibles, `dst` sur `size64`
    /// octets inscriptibles.
    pub unsafe fn to64(&self, src: usize, dst: *mut u8) {
        let s = src as *const u8;
        for f in self.fields {
            match *f {
                F::D(a, b) => (dst.add(b as usize) as *mut u32)
                    .write_unaligned((s.add(a as usize) as *const u32).read_unaligned()),
                F::W(a, b) => (dst.add(b as usize) as *mut u16)
                    .write_unaligned((s.add(a as usize) as *const u16).read_unaligned()),
                F::B(a, b) => *dst.add(b as usize) = *s.add(a as usize),
                F::P(a, b) => (dst.add(b as usize) as *mut u64)
                    .write_unaligned(widen((s.add(a as usize) as *const u32).read_unaligned())),
                F::N(a, b, n) => std::ptr::copy_nonoverlapping(
                    s.add(a as usize),
                    dst.add(b as usize),
                    n as usize,
                ),
                F::Size(_, b) => {
                    (dst.add(b as usize) as *mut u32).write_unaligned(self.size64 as u32)
                }
            }
        }
    }

    /// Recopie le résultat 64 bits `src` dans la structure x86 `dst`.
    ///
    /// # Safety
    /// Comme `to64`, dans l'autre sens.
    pub unsafe fn to32(&self, src: *const u8, dst: usize) {
        let d = dst as *mut u8;
        for f in self.fields {
            match *f {
                F::D(a, b) => (d.add(a as usize) as *mut u32)
                    .write_unaligned((src.add(b as usize) as *const u32).read_unaligned()),
                F::W(a, b) => (d.add(a as usize) as *mut u16)
                    .write_unaligned((src.add(b as usize) as *const u16).read_unaligned()),
                F::B(a, b) => *d.add(a as usize) = *src.add(b as usize),
                F::P(a, b) => (d.add(a as usize) as *mut u32)
                    .write_unaligned((src.add(b as usize) as *const u64).read_unaligned() as u32),
                F::N(a, b, n) => std::ptr::copy_nonoverlapping(
                    src.add(b as usize),
                    d.add(a as usize),
                    n as usize,
                ),
                F::Size(..) => {}
            }
        }
    }

    /// Tampon 64 bits rempli depuis la structure x86 (aligné sur 8).
    ///
    /// # Safety
    /// Comme `to64`.
    pub unsafe fn boxed64(&self, src: usize) -> Vec<u64> {
        let mut v = vec![0u64; self.size64.div_ceil(8) + 1];
        self.to64(src, v.as_mut_ptr() as *mut u8);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_avec_pointeurs_et_constantes() {
        static L: Layout = Layout {
            size32: 16,
            size64: 32,
            fields: &[
                F::Size(0, 0),
                F::P(4, 8),
                F::D(8, 16),
                F::W(12, 20),
                F::P(14, 24),
            ],
        };
        let mut x86 = [0u8; 16];
        x86[0..4].copy_from_slice(&16u32.to_le_bytes());
        x86[4..8].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        x86[8..12].copy_from_slice(&7u32.to_le_bytes());
        x86[12..14].copy_from_slice(&3u16.to_le_bytes());
        x86[14..16].copy_from_slice(&[0, 0]);
        let mut src = [0u8; 20];
        src[..16].copy_from_slice(&x86);
        src[14..18].copy_from_slice(&0xffff_0002u32.to_le_bytes());
        let v = unsafe { L.boxed64(src.as_ptr() as usize) };
        let b = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, 32) };
        assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 32);
        assert_eq!(
            u64::from_le_bytes(b[8..16].try_into().unwrap()),
            0x1234_5678
        );
        assert_eq!(u32::from_le_bytes(b[16..20].try_into().unwrap()), 7);
        assert_eq!(
            u64::from_le_bytes(b[24..32].try_into().unwrap()),
            0xffff_ffff_ffff_0002
        );
        let mut back = [0u8; 20];
        back[0..4].copy_from_slice(&16u32.to_le_bytes());
        unsafe { L.to32(v.as_ptr() as *const u8, back.as_mut_ptr() as usize) };
        assert_eq!(&back[0..4], &16u32.to_le_bytes());
        assert_eq!(&back[4..14], &src[4..14]);
        assert_eq!(&back[14..18], &0xffff_0002u32.to_le_bytes());
    }
}
