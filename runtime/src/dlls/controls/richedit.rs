//! Contrôle RichEdit (RICHEDIT50W de msftedit.dll, RichEdit20W/A de
//! riched20.dll, RICHEDIT de riched32.dll) : un EDIT multiligne de Winlux
//! qui répond aussi aux messages propres à RichEdit (sélection étendue,
//! format de caractères, masque d'événements, longueur et plages de texte).
//!
//! Le texte reste en une seule mise en forme : EM_SETCHARFORMAT est accepté
//! mais les couleurs et attributs par plage ne sont pas encore dessinés.
//! Sans ce contrôle, wxWidgets affichait « Impossible to create a rich edit
//! control » et le journal de FileZilla retombait sur un EDIT simple (#795).
//!
//! Références : documentation publique Win32 « Rich Edit Messages ».

use super::*;

/// Classes RichEdit reconnues (en minuscules).
pub(super) fn is_rich_class(lower: &str) -> bool {
    matches!(
        lower,
        "richedit50w" | "richedit60w" | "richedit20w" | "richedit20a" | "richedit"
    )
}

/// Vrai si la fenêtre est un RichEdit (pour les comportements qui diffèrent
/// de l'EDIT : défilement borné à la dernière page).
pub(super) fn is_rich(hwnd: usize) -> bool {
    gui::window_class(hwnd).is_some_and(|c| is_rich_class(&c.to_ascii_lowercase()))
}

pub(super) unsafe extern "win64" fn rich_edit_proc(
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    if let Some(r) = rich_message(hwnd, msg, wp, lp) {
        return r;
    }
    let r = control_proc_impl(Kind::Edit, hwnd, msg, wp, lp);
    if msg == WM_NCCREATE {
        with_ctl(hwnd, |c| c.edit.limit = 32767);
        lock(rich_state()).insert(hwnd, RichState::default());
    } else if msg == WM_NCDESTROY {
        lock(rich_state()).remove(&hwnd);
    }
    r
}

#[derive(Clone, Copy, Default)]
struct RichState {
    event_mask: u32,
    options: u32,
    text_mode: u32,
    lang_options: u32,
    edit_style: u32,
    bk_color: u32,
}

fn rich_state() -> &'static Mutex<HashMap<usize, RichState>> {
    static S: OnceLock<Mutex<HashMap<usize, RichState>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn with_rich<R>(hwnd: usize, f: impl FnOnce(&mut RichState) -> R) -> R {
    let mut map = lock(rich_state());
    f(map.entry(hwnd).or_default())
}

const EM_EXGETSEL: u32 = 0x0434;
const EM_EXLIMITTEXT: u32 = 0x0435;
const EM_EXLINEFROMCHAR: u32 = 0x0436;
const EM_EXSETSEL: u32 = 0x0437;
const EM_GETCHARFORMAT: u32 = 0x043A;
const EM_GETEVENTMASK: u32 = 0x043B;
const EM_GETPARAFORMAT: u32 = 0x043D;
const EM_HIDESELECTION: u32 = 0x043F;
const EM_SETBKGNDCOLOR: u32 = 0x0443;
const EM_SETCHARFORMAT: u32 = 0x0444;
const EM_SETEVENTMASK: u32 = 0x0445;
const EM_SETPARAFORMAT: u32 = 0x0447;
const EM_SETTARGETDEVICE: u32 = 0x0448;
const EM_STREAMIN: u32 = 0x0449;
const EM_STREAMOUT: u32 = 0x044A;
const EM_GETTEXTRANGE: u32 = 0x044B;
const EM_SETOPTIONS: u32 = 0x044D;
const EM_GETOPTIONS: u32 = 0x044E;
const EM_SETUNDOLIMIT: u32 = 0x0452;
const EM_SETTEXTMODE: u32 = 0x0459;
const EM_GETTEXTMODE: u32 = 0x045A;
const EM_AUTOURLDETECT: u32 = 0x045B;
const EM_GETTEXTEX: u32 = 0x045E;
const EM_GETTEXTLENGTHEX: u32 = 0x045F;
const EM_SETTEXTEX: u32 = 0x0461;
const EM_SETLANGOPTIONS: u32 = 0x0478;
const EM_GETLANGOPTIONS: u32 = 0x0479;
const EM_SETEDITSTYLE: u32 = 0x04CC;
const EM_GETEDITSTYLE: u32 = 0x04CD;
const EM_GETSCROLLPOS: u32 = 0x04DD;
const EM_SETSCROLLPOS: u32 = 0x04DE;
const EM_SETZOOM: u32 = 0x04E1;
const EM_GETZOOM: u32 = 0x04E0;

/// TM_PLAINTEXT | TM_MULTILEVELUNDO | TM_MULTICODEPAGE : ce que le contrôle
/// sait faire réellement.
const TEXT_MODE: u32 = 0x1 | 0x4 | 0x20;

/// Lit une CHARRANGE { cpMin, cpMax } (-1 pour cpMax = fin du texte).
unsafe fn char_range(lp: isize) -> (i32, i32) {
    let p = lp as *const i32;
    (p.read_unaligned(), p.add(1).read_unaligned())
}

/// Texte tel que RichEdit le compte : une fin de ligne vaut un caractère
/// (« \r »), alors que l'EDIT garde « \r\n ». Les positions de Winlux sont
/// déjà en caractères avec « \n » seul quand le programme écrit « \n ».
fn text_of(hwnd: usize) -> String {
    title(hwnd)
}

unsafe fn rich_message(hwnd: usize, msg: u32, wp: usize, lp: isize) -> Option<isize> {
    let wide = wide_for(hwnd);
    let r = match msg {
        EM_EXGETSEL => {
            let (a, b) = with_ctl(hwnd, |c| {
                (
                    c.edit.anchor.min(c.edit.caret),
                    c.edit.anchor.max(c.edit.caret),
                )
            })?;
            if lp != 0 {
                let p = lp as *mut i32;
                p.write_unaligned(a as i32);
                p.add(1).write_unaligned(b as i32);
            }
            0
        }
        EM_EXSETSEL => {
            if lp == 0 {
                return Some(0);
            }
            let (a, b) = char_range(lp);
            // cpMin = 0 et cpMax = -1 : tout ; cpMin = -1 : aucune sélection.
            control_proc_impl(Kind::Edit, hwnd, 0x00B1, a as isize as usize, b as isize);
            with_ctl(hwnd, |c| c.edit.anchor.max(c.edit.caret)).unwrap_or(0) as isize
        }
        EM_EXLIMITTEXT => {
            with_ctl(hwnd, |c| {
                c.edit.limit = if lp == 0 { 65536 } else { lp as usize };
            });
            0
        }
        EM_EXLINEFROMCHAR => control_proc_impl(Kind::Edit, hwnd, 0x00C9, lp as usize, 0),
        EM_GETCHARFORMAT => {
            write_char_format(lp, wide);
            // CFM_ALL : tous les attributs sont uniformes.
            0xF800_003F
        }
        EM_SETCHARFORMAT | EM_SETPARAFORMAT | EM_SETTARGETDEVICE => 1,
        EM_GETPARAFORMAT => 0,
        EM_SETEVENTMASK => {
            with_rich(hwnd, |s| std::mem::replace(&mut s.event_mask, lp as u32)) as isize
        }
        EM_GETEVENTMASK => with_rich(hwnd, |s| s.event_mask) as isize,
        EM_SETOPTIONS => with_rich(hwnd, |s| {
            // ECOOP_SET = 1, ECOOP_OR = 2, ECOOP_AND = 3, ECOOP_XOR = 4.
            s.options = match wp {
                2 => s.options | lp as u32,
                3 => s.options & lp as u32,
                4 => s.options ^ lp as u32,
                _ => lp as u32,
            };
            s.options
        }) as isize,
        EM_GETOPTIONS => with_rich(hwnd, |s| s.options) as isize,
        EM_SETTEXTMODE => {
            with_rich(hwnd, |s| s.text_mode = wp as u32);
            0
        }
        EM_GETTEXTMODE => with_rich(hwnd, |s| {
            if s.text_mode == 0 {
                TEXT_MODE
            } else {
                s.text_mode
            }
        }) as isize,
        EM_SETLANGOPTIONS => {
            with_rich(hwnd, |s| s.lang_options = lp as u32);
            1
        }
        EM_GETLANGOPTIONS => with_rich(hwnd, |s| s.lang_options) as isize,
        EM_SETEDITSTYLE => with_rich(hwnd, |s| {
            s.edit_style = (s.edit_style & !(lp as u32)) | (wp as u32 & lp as u32);
            s.edit_style
        }) as isize,
        EM_GETEDITSTYLE => with_rich(hwnd, |s| s.edit_style) as isize,
        EM_SETBKGNDCOLOR => {
            let old = with_rich(hwnd, |s| std::mem::replace(&mut s.bk_color, lp as u32));
            invalidate(hwnd);
            old as isize
        }
        EM_HIDESELECTION | EM_AUTOURLDETECT | EM_SETUNDOLIMIT => 0,
        EM_SETZOOM => 1,
        EM_GETZOOM => {
            if wp != 0 {
                *(wp as *mut i32) = 0;
            }
            if lp != 0 {
                *(lp as *mut i32) = 0;
            }
            1
        }
        // Les flux RTF ne sont pas lus : aucun caractère transféré.
        EM_STREAMIN | EM_STREAMOUT => 0,
        EM_GETTEXTLENGTHEX => {
            let text = text_of(hwnd);
            // GTL_NUMBYTES (0x10) en ANSI : octets ; sinon caractères UTF-16.
            let flags = if lp == 0 { 0 } else { *(lp as *const u32) };
            let codepage = if lp == 0 {
                1200
            } else {
                *(lp as *const u32).add(1)
            };
            if flags & 0x10 != 0 && codepage != 1200 {
                crate::dlls::encode_ansi(&text).len() as isize
            } else {
                text.encode_utf16().count() as isize
            }
        }
        EM_GETTEXTEX => {
            // GETTEXTEX { cb, flags, codepage, lpDefaultChar, lpUsedDefChar }.
            if wp == 0 || lp == 0 {
                return Some(0);
            }
            let cb = *(wp as *const u32);
            let codepage = *(wp as *const u32).add(2);
            let text = text_of(hwnd);
            if codepage == 1200 {
                crate::dlls::put_text_w(&text, lp as *mut u16, cb / 2) as isize
            } else {
                crate::dlls::put_text_a(&text, lp as *mut u8, cb) as isize
            }
        }
        EM_SETTEXTEX => {
            // SETTEXTEX { flags, codepage } ; ST_SELECTION = 2 remplace la
            // sélection, sinon tout le texte.
            let (flags, codepage) = if wp == 0 {
                (0, 1200)
            } else {
                (*(wp as *const u32), *(wp as *const u32).add(1))
            };
            let s = if lp == 0 {
                String::new()
            } else if codepage == 1200 {
                crate::dlls::wstr(lp as *const u16)
            } else {
                crate::dlls::cstr(lp as *const u8)
            };
            if flags & 0x2 != 0 {
                edit_replace(hwnd, &s, false);
            } else {
                control_proc_impl(Kind::Edit, hwnd, 0x00B1, 0, -1);
                edit_replace(hwnd, &s, false);
                with_ctl(hwnd, |c| {
                    c.edit.anchor = 0;
                    c.edit.caret = 0;
                    c.edit.top_line = 0;
                });
            }
            1
        }
        EM_GETTEXTRANGE => {
            // TEXTRANGE { CHARRANGE chrg; LPTSTR lpstrText } : le pointeur
            // suit la plage, aligné sur sa taille.
            if lp == 0 {
                return Some(0);
            }
            let (a, b) = char_range(lp);
            let out = crate::wow32::read_ptr(lp as usize + 8);
            let text: Vec<char> = text_of(hwnd).chars().collect();
            let end = if b < 0 {
                text.len()
            } else {
                (b as usize).min(text.len())
            };
            let start = (a.max(0) as usize).min(end);
            let part: String = text[start..end].iter().collect();
            if out == 0 {
                return Some(0);
            }
            if wide {
                crate::dlls::put_text_w(&part, out as *mut u16, u32::MAX / 4) as isize
            } else {
                crate::dlls::put_text_a(&part, out as *mut u8, u32::MAX / 4) as isize
            }
        }
        EM_GETSCROLLPOS => {
            if lp != 0 {
                let c = snapshot(hwnd)?;
                let lh = line_height(font_of(&c));
                let p = lp as *mut i32;
                p.write_unaligned(c.edit.scroll_x);
                p.add(1).write_unaligned(c.edit.top_line as i32 * lh);
            }
            1
        }
        EM_SETSCROLLPOS => {
            if lp != 0 {
                let c = snapshot(hwnd)?;
                let lh = line_height(font_of(&c)).max(1);
                let y = (lp as *const i32).add(1).read_unaligned().max(0);
                let target = y / lh;
                let now = c.edit.top_line as i32;
                control_proc_impl(Kind::Edit, hwnd, 0x00B6, 0, (target - now) as isize);
            }
            1
        }
        _ => return None,
    };
    Some(r)
}

/// CHARFORMAT(2) de la police par défaut : Segoe UI 9 pt, couleur
/// automatique. Seuls les champs communs à CHARFORMAT et CHARFORMAT2 sont
/// écrits (la structure fournie fait au moins cette taille).
unsafe fn write_char_format(lp: isize, wide: bool) {
    if lp == 0 {
        return;
    }
    let p = lp as *mut u8;
    let cb = (p as *const u32).read_unaligned();
    let face_len = if wide { 64 } else { 32 };
    let base = 4 + 4 * 6 + 2;
    if (cb as usize) < base + face_len {
        return;
    }
    let put = |off: usize, v: u32| (p.add(off) as *mut u32).write_unaligned(v);
    put(4, 0xF800_003F); // dwMask = CFM_ALL
    put(8, 0x4000_0000); // dwEffects = CFE_AUTOCOLOR
    put(12, 180); // yHeight en twips (9 pt)
    put(16, 0); // yOffset
    put(20, 0); // crTextColor
    *p.add(24) = 1; // bCharSet = DEFAULT_CHARSET
    *p.add(25) = 0; // bPitchAndFamily
    let face = p.add(26);
    std::ptr::write_bytes(face, 0, face_len);
    if wide {
        for (i, u) in "Segoe UI".encode_utf16().enumerate() {
            (face as *mut u16).add(i).write_unaligned(u);
        }
    } else {
        std::ptr::copy_nonoverlapping(b"Segoe UI".as_ptr(), face, 8);
    }
}
