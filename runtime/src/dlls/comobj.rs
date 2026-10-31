//! Petits outils partagés par les objets COM écrits en Rust (DirectInput,
//! Media Foundation) : GUID, tables virtuelles et chaînes à taille fixe.

/// GUID dans sa représentation mémoire Windows (Data1..3 petit-boutistes).
pub type Guid = [u8; 16];

pub const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    let a = d1.to_le_bytes();
    let b = d2.to_le_bytes();
    let c = d3.to_le_bytes();
    [
        a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d4[0], d4[1], d4[2], d4[3], d4[4], d4[5],
        d4[6], d4[7],
    ]
}

pub const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

pub const S_OK: i32 = 0;
pub const S_FALSE: i32 = 1;
pub const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
pub const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
pub const E_POINTER: i32 = 0x8000_4003u32 as i32;
pub const E_FAIL: i32 = 0x8000_4005u32 as i32;
pub const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
pub const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;

/// Lit un GUID passé par pointeur (`REFGUID`) ; `None` si nul.
///
/// # Safety
/// `p` doit être nul ou pointer sur 16 octets lisibles.
pub unsafe fn read_guid(p: *const u8) -> Option<Guid> {
    if p.is_null() {
        None
    } else {
        Some(std::ptr::read_unaligned(p as *const Guid))
    }
}

/// Table virtuelle statique : pointeurs de fonctions de signatures variées.
#[repr(transparent)]
pub struct VTable<const N: usize>(pub [*const (); N]);
unsafe impl<const N: usize> Sync for VTable<N> {}

/// Écrit `s` dans un tampon de `chars` caractères, large ou ANSI, terminé
/// par un zéro (tronqué si besoin).
///
/// # Safety
/// `dst` doit pouvoir recevoir `chars` caractères du type choisi.
pub unsafe fn write_fixed_str(dst: *mut u8, chars: usize, s: &str, wide: bool) {
    if chars == 0 {
        return;
    }
    if wide {
        let w: Vec<u16> = s.encode_utf16().take(chars - 1).collect();
        let d = dst as *mut u16;
        for (i, c) in w.iter().enumerate() {
            d.add(i).write_unaligned(*c);
        }
        d.add(w.len()).write_unaligned(0);
    } else {
        let a = super::encode_ansi(s);
        let n = a.len().min(chars - 1);
        std::ptr::copy_nonoverlapping(a.as_ptr(), dst, n);
        *dst.add(n) = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_en_memoire_windows() {
        // {6F1D2B61-D5A0-11CF-BFC7-444553540000} = GUID_SysKeyboard
        let g = guid(
            0x6F1D2B61,
            0xD5A0,
            0x11CF,
            [0xBF, 0xC7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00],
        );
        assert_eq!(&g[..4], &[0x61, 0x2B, 0x1D, 0x6F]);
        assert_eq!(&g[4..8], &[0xA0, 0xD5, 0xCF, 0x11]);
        assert_eq!(g[8], 0xBF);
    }

    #[test]
    fn chaines_fixes_tronquees() {
        let mut w = [0xFFFFu16; 4];
        unsafe { write_fixed_str(w.as_mut_ptr() as *mut u8, 4, "Manette", true) };
        assert_eq!(w, [b'M' as u16, b'a' as u16, b'n' as u16, 0]);
        let mut a = [0xFFu8; 8];
        unsafe { write_fixed_str(a.as_mut_ptr(), 8, "Volant", false) };
        assert_eq!(&a[..7], b"Volant\0");
    }
}
