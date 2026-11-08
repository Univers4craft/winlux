//! Boîtes de dialogue communes pour les programmes x86 (#232, étape 6).
//!
//! GetOpenFileName, ChooseColor, ChooseFont, PrintDlg… reçoivent une
//! structure pleine de pointeurs et de handles : sa version x86 est
//! recopiée dans une structure 64 bits, la boîte du runtime est ouverte,
//! puis le résultat (chemin choisi, couleur, police, handles DEVMODE…) est
//! recopié dans la structure du programme.
//!
//! Dans commdlg.h, les structures x86 sont compactées sur 1 octet
//! (`pshpack1.h`) : PRINTDLG, PAGESETUPDLG et PRINTDLGEX ont donc des
//! champs non alignés côté x86. Les procédures crochets (`lpfnHook`) ne
//! sont pas transmises : les boîtes du runtime ne les appellent pas.

use super::layout::{Layout, F};
use super::rd;

use F::{Size, D, N, P, W};

static OPENFILENAME: Layout = Layout {
    size32: 88,
    size64: 152,
    fields: &[
        Size(0, 0),
        P(4, 8),
        P(8, 16),
        P(12, 24),
        P(16, 32),
        D(20, 40),
        D(24, 44),
        P(28, 48),
        D(32, 56),
        P(36, 64),
        D(40, 72),
        P(44, 80),
        P(48, 88),
        D(52, 96),
        W(56, 100),
        W(58, 102),
        P(60, 104),
        P(64, 112),
        P(72, 128),
        D(80, 144),
        D(84, 148),
    ],
};

static CHOOSECOLOR: Layout = Layout {
    size32: 36,
    size64: 72,
    fields: &[
        Size(0, 0),
        P(4, 8),
        P(8, 16),
        D(12, 24),
        P(16, 32),
        D(20, 40),
        P(24, 48),
        P(32, 64),
    ],
};

static CHOOSEFONT: Layout = Layout {
    size32: 60,
    size64: 104,
    fields: &[
        Size(0, 0),
        P(4, 8),
        P(8, 16),
        P(12, 24),
        D(16, 32),
        D(20, 36),
        D(24, 40),
        P(28, 48),
        P(36, 64),
        P(40, 72),
        P(44, 80),
        W(48, 88),
        D(52, 92),
        D(56, 96),
    ],
};

static PRINTDLG: Layout = Layout {
    size32: 66,
    size64: 120,
    fields: &[
        Size(0, 0),
        P(4, 8),
        P(8, 16),
        P(12, 24),
        P(16, 32),
        D(20, 40),
        W(24, 44),
        W(26, 46),
        W(28, 48),
        W(30, 50),
        W(32, 52),
        P(34, 56),
        P(38, 64),
        P(50, 88),
        P(54, 96),
        P(58, 104),
        P(62, 112),
    ],
};

static PAGESETUPDLG: Layout = Layout {
    size32: 84,
    size64: 128,
    fields: &[
        Size(0, 0),
        P(4, 8),
        P(8, 16),
        P(12, 24),
        D(16, 32),
        // ptPaperSize, rtMinMargin, rtMargin : 40 octets sans pointeur.
        N(20, 36, 40),
        P(60, 80),
        P(64, 88),
        P(76, 112),
        P(80, 120),
    ],
};

/// PRINTDLGEX sans le rappel IPrintDialogCallback ni les pages de
/// propriétés supplémentaires (handles 64 bits du runtime).
static PRINTDLGEX: Layout = Layout {
    size32: 84,
    size64: 136,
    fields: &[
        Size(0, 0),
        P(4, 8),
        P(8, 16),
        P(12, 24),
        P(16, 32),
        D(20, 40),
        D(24, 44),
        D(28, 48),
        D(32, 52),
        D(36, 56),
        P(40, 64),
        D(44, 72),
        D(48, 76),
        D(52, 80),
        P(56, 88),
        P(60, 96),
        D(76, 128),
        D(80, 132),
    ],
};

fn layout_of(func: &str) -> Option<&'static Layout> {
    Some(match func {
        "GetOpenFileNameA" | "GetOpenFileNameW" | "GetSaveFileNameA" | "GetSaveFileNameW" => {
            &OPENFILENAME
        }
        "ChooseColorA" | "ChooseColorW" => &CHOOSECOLOR,
        "ChooseFontA" | "ChooseFontW" => &CHOOSEFONT,
        "PrintDlgA" | "PrintDlgW" => &PRINTDLG,
        "PageSetupDlgA" | "PageSetupDlgW" => &PAGESETUPDLG,
        "PrintDlgExA" | "PrintDlgExW" => &PRINTDLGEX,
        _ => return None,
    })
}

struct Dlg {
    target: usize,
    layout: &'static Layout,
}

/// Convertit la structure x86 (seul argument), appelle la boîte 64 bits
/// et recopie le résultat.
pub(super) fn call_with_struct(target: usize, layout: &Layout, ptr: u32) -> u64 {
    let f: extern "win64" fn(*mut u8) -> u64 = unsafe { std::mem::transmute(target) };
    if ptr == 0 {
        return f(std::ptr::null_mut());
    }
    // Une structure plus courte (ancienne version, OPENFILENAME de 76
    // octets) est complétée par des zéros.
    let given = (rd(ptr) as usize).min(layout.size32);
    let mut x86 = vec![0u8; layout.size32 + 8];
    unsafe {
        std::ptr::copy_nonoverlapping(ptr as usize as *const u8, x86.as_mut_ptr(), given);
        let mut wide = layout.boxed64(x86.as_ptr() as usize);
        let r = f(wide.as_mut_ptr() as *mut u8);
        layout.to32(wide.as_ptr() as *const u8, x86.as_mut_ptr() as usize);
        std::ptr::copy_nonoverlapping(x86.as_ptr(), ptr as usize as *mut u8, given);
        r
    }
}

extern "win64" fn dlg32(args: u32, aux: usize) -> u64 {
    let d = unsafe { &*(aux as *const Dlg) };
    call_with_struct(d.target, d.layout, rd(args))
}

/// FindText / ReplaceText (#712) : la boîte est non modale et prévient le
/// programme avec un pointeur sur SA structure. Pas de copie 64 bits : la
/// boîte lit et écrit directement la FINDREPLACE x86 (40 octets), et la
/// procédure crochet x86 passe par une passerelle 64 bits.
extern "win64" fn find32(args: u32, aux: usize) -> u64 {
    use crate::dlls::comdlg32_find::{open, FR_ENABLEHOOK, LAYOUT32};
    let (wide, replace) = (aux & 1 != 0, aux & 2 != 0);
    let fr = rd(args) as usize;
    unsafe {
        let hook = if fr != 0 && LAYOUT32.flags(fr) & FR_ENABLEHOOK != 0 {
            super::user::wndproc64(LAYOUT32.ptr_at(fr, LAYOUT32.hook) as u32)
        } else {
            0
        };
        open(fr, LAYOUT32, wide, replace, hook) as u64
    }
}

/// Passerelle x86 des boîtes communes de comdlg32.
pub(super) fn override_for(dll: &str, func: &str) -> Option<super::crt::Override> {
    if dll != "comdlg32" {
        return None;
    }
    let find = match func {
        "FindTextA" => Some(0),
        "FindTextW" => Some(1),
        "ReplaceTextA" => Some(2),
        "ReplaceTextW" => Some(3),
        _ => None,
    };
    if let Some(aux) = find {
        return Some(super::crt::Override::Raw {
            target: find32 as *const () as usize,
            aux,
            pop: Some(4),
        });
    }
    let layout = layout_of(func)?;
    let target = crate::dlls::lookup(dll, func)?;
    let d = Box::leak(Box::new(Dlg { target, layout }));
    Some(super::crt::Override::Raw {
        target: dlg32 as *const () as usize,
        aux: d as *const Dlg as usize,
        pop: Some(4),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailles_des_structures() {
        // Dernier champ de chaque structure + sa taille = taille x86/x64.
        for l in [
            &OPENFILENAME,
            &CHOOSECOLOR,
            &CHOOSEFONT,
            &PRINTDLG,
            &PAGESETUPDLG,
            &PRINTDLGEX,
        ] {
            let mut end32 = 0;
            let mut end64 = 0;
            for f in l.fields {
                let (a, b, n32, n64) = match *f {
                    F::D(a, b) | F::Size(a, b) => (a, b, 4, 4),
                    F::W(a, b) => (a, b, 2, 2),
                    F::B(a, b) => (a, b, 1, 1),
                    F::P(a, b) => (a, b, 4, 8),
                    F::N(a, b, n) => (a, b, n, n),
                };
                end32 = end32.max(a as usize + n32 as usize);
                end64 = end64.max(b as usize + n64 as usize);
            }
            assert!(end32 <= l.size32 && end64 <= l.size64);
        }
        assert!(layout_of("GetSaveFileNameW").is_some());
        assert!(layout_of("CommDlgExtendedError").is_none());
    }
}
