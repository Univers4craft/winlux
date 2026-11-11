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
pub const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

/// GUID sous sa forme lisible {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}.
pub fn guid_str(g: &Guid) -> String {
    let d1 = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
    let d2 = u16::from_le_bytes([g[4], g[5]]);
    let d3 = u16::from_le_bytes([g[6], g[7]]);
    format!(
        "{{{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15]
    )
}

/// Journalise une interface demandée mais non fournie (pour savoir laquelle
/// écrire ensuite).
pub fn log_no_interface<T>(object: &str, iid: *const T) {
    if iid.is_null() {
        return;
    }
    // SAFETY: un IID pointe toujours sur 16 octets.
    let g = unsafe { std::ptr::read_unaligned(iid as *const Guid) };
    // Un programme qui sonde la même interface sur chaque objet (Paint.NET :
    // plus de 900 000 refus) ne doit pas noyer le journal ni le ralentir :
    // les premiers refus de chaque IID sont notés, puis un rappel par
    // puissance de dix.
    let key = unsafe { std::ptr::read_unaligned(iid as *const [u8; 16]) };
    let n = {
        static SEEN: std::sync::Mutex<Vec<([u8; 16], u64)>> = std::sync::Mutex::new(Vec::new());
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        match seen.iter_mut().find(|(k, _)| *k == key) {
            Some((_, c)) => {
                *c += 1;
                *c
            }
            None => {
                seen.push((key, 1));
                1
            }
        }
    };
    if !refusal_worth_logging(n) {
        return;
    }
    if n <= REFUSALS_LOGGED {
        crate::log_warn!("com", "{object} : QueryInterface {} refusée", guid_str(&g));
    } else {
        crate::log_warn!(
            "com",
            "{object} : QueryInterface {} refusée ({n} fois)",
            guid_str(&g)
        );
    }
}

/// Refus notés en entier pour chaque IID avant le seul rappel périodique.
const REFUSALS_LOGGED: u64 = 4;

/// Vrai pour les premiers refus puis pour chaque puissance de dix.
fn refusal_worth_logging(n: u64) -> bool {
    if n <= REFUSALS_LOGGED {
        return true;
    }
    let mut p = 10;
    while p < n {
        p *= 10;
    }
    p == n
}

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
    fn refus_d_interface_notes_avec_parcimonie() {
        let notes: Vec<u64> = (1..=100_000)
            .filter(|&n| super::refusal_worth_logging(n))
            .collect();
        assert_eq!(notes, [1, 2, 3, 4, 10, 100, 1000, 10_000, 100_000]);
    }

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

/// Tampon d'octets aligné sur 8, pour bâtir un VARIANT, un VARDESC ou un
/// autre bloc que lisent des fonctions qui le déréférencent directement
/// (un `[u8; N]` sur la pile peut être mal aligné).
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct Aligned<const N: usize>(pub [u8; N]);

impl<const N: usize> Default for Aligned<N> {
    fn default() -> Self {
        Aligned([0; N])
    }
}

impl<const N: usize> std::ops::Deref for Aligned<N> {
    type Target = [u8; N];
    fn deref(&self) -> &[u8; N] {
        &self.0
    }
}

impl<const N: usize> std::ops::DerefMut for Aligned<N> {
    fn deref_mut(&mut self) -> &mut [u8; N] {
        &mut self.0
    }
}
