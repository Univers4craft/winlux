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

const SF_RTF: u32 = 0x2;
const SFF_SELECTION: u32 = 0x8000;
const SF_UNICODE: u32 = 0x10;
const SF_USECODEPAGE: u32 = 0x20;

/// EDITSTREAMCALLBACK(dwCookie, pbBuff, cb, pcb) -> 0 pour continuer.
type StreamCallback = unsafe extern "win64" fn(usize, *mut u8, i32, *mut i32) -> u32;

/// EDITSTREAM en 64 bits : dwCookie (8), dwError (4 + 4 de remplissage),
/// pfnCallback (8). Les programmes 32 bits ne sont pas encore servis.
/// EDITSTREAM est déclarée sous `pack(4)` dans richedit.h : en x64,
/// dwCookie (8 octets) à 0, dwError (4) à 8 et pfnCallback à 12, sans
/// remplissage (20 octets). Lire le rappel à 16 donnait la moitié haute du
/// pointeur, et Paint.NET sautait à 0x7288 (#1454).
const EDITSTREAM_CALLBACK: usize = 12;

unsafe fn edit_stream(lp: isize) -> Option<(usize, StreamCallback)> {
    if lp == 0 || crate::wow32::pointer_size() != 8 {
        return None;
    }
    let p = lp as *mut u8;
    let cookie = (p as *const usize).read_unaligned();
    let cb = (p.add(EDITSTREAM_CALLBACK) as *const usize).read_unaligned();
    if cb == 0 {
        return None;
    }
    (p.add(8) as *mut u32).write_unaligned(0);
    Some((cookie, std::mem::transmute::<usize, StreamCallback>(cb)))
}

unsafe fn set_stream_error(lp: isize, err: u32) {
    ((lp as *mut u8).add(8) as *mut u32).write_unaligned(err);
}

/// EM_STREAMIN : le programme fournit le texte par son rappel, en texte
/// brut (ANSI, page de code ou UTF-16) ou en RTF, réduit ici à son texte.
/// WinForms (RichTextBox.Text) passe par là : sans lui, la zone de détails
/// de la boîte de plantage de Paint.NET restait vide (#1433).
unsafe fn stream_in(hwnd: usize, flags: u32, lp: isize) -> isize {
    let Some((cookie, cb)) = edit_stream(lp) else {
        return 0;
    };
    let mut bytes = Vec::new();
    let mut buf = vec![0u8; 4096];
    loop {
        let mut got = 0i32;
        let err = cb(cookie, buf.as_mut_ptr(), buf.len() as i32, &mut got);
        if err != 0 {
            set_stream_error(lp, err);
            break;
        }
        if got <= 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..(got as usize).min(buf.len())]);
    }
    let text = if flags & SF_RTF != 0 || bytes.starts_with(b"{\\rtf") {
        rtf_to_text(&bytes)
    } else if flags & SF_UNICODE != 0 {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else if flags & SF_USECODEPAGE != 0 && flags >> 16 == 65001 {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        crate::dlls::decode_ansi(&bytes)
    };
    let text = text.trim_end_matches('\0').to_string();
    let n = text.chars().count();
    if flags & SFF_SELECTION != 0 {
        edit_replace(hwnd, &text, false);
    } else {
        // Le flux remplace tout le texte, sans la limite de saisie.
        with_ctl(hwnd, |c| c.edit.limit = c.edit.limit.max(n));
        control_proc_impl(Kind::Edit, hwnd, 0x00B1, 0, -1);
        edit_replace(hwnd, &text, false);
        with_ctl(hwnd, |c| {
            c.edit.anchor = 0;
            c.edit.caret = 0;
            c.edit.top_line = 0;
            c.edit.modified = false;
        });
    }
    n as isize
}

/// EM_STREAMOUT : rend le texte (ou la sélection) par le rappel, en texte
/// brut ; le RTF demandé est un RTF minimal autour du même texte.
unsafe fn stream_out(hwnd: usize, flags: u32, lp: isize) -> isize {
    let Some((cookie, cb)) = edit_stream(lp) else {
        return 0;
    };
    let text = if flags & SFF_SELECTION != 0 {
        selected_text(hwnd)
    } else {
        text_of(hwnd)
    };
    let mut bytes = if flags & SF_RTF != 0 {
        text_to_rtf(&text).into_bytes()
    } else if flags & SF_UNICODE != 0 {
        text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    } else if flags & SF_USECODEPAGE != 0 && flags >> 16 == 65001 {
        text.clone().into_bytes()
    } else {
        crate::dlls::encode_ansi(&text)
    };
    let total = bytes.len();
    let mut off = 0;
    while off < total {
        let chunk = (total - off).min(4096);
        let mut done = 0i32;
        let err = cb(cookie, bytes[off..].as_mut_ptr(), chunk as i32, &mut done);
        if err != 0 {
            set_stream_error(lp, err);
            break;
        }
        if done <= 0 {
            break;
        }
        off += (done as usize).min(chunk);
    }
    text.chars().count() as isize
}

/// Texte d'un document RTF : groupes de destination ignorés (tables de
/// polices et de couleurs, images, `\\*`), `\\par` et `\\line` en fin de
/// ligne, `\\tab`, `\\'hh` (page de code 1252) et `\\uN` (avec `\\ucN`).
pub(super) fn rtf_to_text(rtf: &[u8]) -> String {
    const SKIP: &[&str] = &[
        "fonttbl",
        "colortbl",
        "stylesheet",
        "info",
        "pict",
        "object",
        "header",
        "footer",
        "listtable",
        "listoverridetable",
        "rsidtbl",
        "generator",
        "themedata",
        "colorschememapping",
        "latentstyles",
        "datastore",
        "xmlnstbl",
    ];
    let mut out = String::new();
    // Pile : (ignorer ce groupe, caractères à sauter après \u).
    let mut stack: Vec<(bool, usize)> = vec![(false, 1)];
    let mut skip_after_u = 0usize;
    let mut i = 0;
    let mut pending_ansi: Vec<u8> = Vec::new();
    let flush = |out: &mut String, pending: &mut Vec<u8>| {
        if !pending.is_empty() {
            out.push_str(&crate::dlls::decode_ansi(pending));
            pending.clear();
        }
    };
    while i < rtf.len() {
        let c = rtf[i];
        let (skipping, uc) = *stack.last().unwrap_or(&(false, 1));
        match c {
            b'{' => {
                stack.push((skipping, uc));
                i += 1;
            }
            b'}' => {
                if stack.len() > 1 {
                    stack.pop();
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&n) = rtf.get(i) else { break };
                if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < rtf.len() && rtf[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&rtf[start..i]).unwrap_or("");
                    let nstart = i;
                    if i < rtf.len() && (rtf[i] == b'-' || rtf[i].is_ascii_digit()) {
                        i += 1;
                        while i < rtf.len() && rtf[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let param: Option<i32> = std::str::from_utf8(&rtf[nstart..i])
                        .ok()
                        .and_then(|t| t.parse().ok());
                    if i < rtf.len() && rtf[i] == b' ' {
                        i += 1;
                    }
                    if SKIP.contains(&word) {
                        if let Some(top) = stack.last_mut() {
                            top.0 = true;
                        }
                        continue;
                    }
                    if skipping {
                        continue;
                    }
                    if skip_after_u > 0 {
                        skip_after_u -= 1;
                        continue;
                    }
                    match word {
                        "par" | "line" => {
                            flush(&mut out, &mut pending_ansi);
                            out.push_str("\r\n");
                        }
                        "tab" => {
                            flush(&mut out, &mut pending_ansi);
                            out.push('\t');
                        }
                        "uc" => {
                            if let Some(top) = stack.last_mut() {
                                top.1 = param.unwrap_or(1).max(0) as usize;
                            }
                        }
                        "u" => {
                            flush(&mut out, &mut pending_ansi);
                            let v = param.unwrap_or(0);
                            let v = if v < 0 { v + 65536 } else { v } as u32;
                            out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
                            skip_after_u = uc;
                        }
                        _ => {}
                    }
                } else {
                    i += 1;
                    match n {
                        b'*' => {
                            if let Some(top) = stack.last_mut() {
                                top.0 = true;
                            }
                        }
                        b'\'' => {
                            let hex = rtf.get(i..i + 2).and_then(|h| std::str::from_utf8(h).ok());
                            i += 2;
                            if skipping {
                                continue;
                            }
                            if skip_after_u > 0 {
                                skip_after_u -= 1;
                                continue;
                            }
                            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                                pending_ansi.push(b);
                            }
                        }
                        b'\\' | b'{' | b'}' if !skipping => {
                            flush(&mut out, &mut pending_ansi);
                            out.push(n as char);
                        }
                        b'~' if !skipping => {
                            flush(&mut out, &mut pending_ansi);
                            out.push('\u{A0}');
                        }
                        b'\n' | b'\r' if !skipping => {
                            flush(&mut out, &mut pending_ansi);
                            out.push_str("\r\n");
                        }
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                i += 1;
                if skipping {
                    continue;
                }
                if skip_after_u > 0 {
                    skip_after_u -= 1;
                    continue;
                }
                pending_ansi.push(c);
            }
        }
    }
    flush(&mut out, &mut pending_ansi);
    out
}

/// RTF minimal qui porte `text` (EM_STREAMOUT avec SF_RTF).
fn text_to_rtf(text: &str) -> String {
    let mut s = String::from("{\\rtf1\\ansi\\deff0 ");
    for ch in text.chars() {
        match ch {
            '\\' | '{' | '}' => {
                s.push('\\');
                s.push(ch);
            }
            '\r' => {}
            '\n' => s.push_str("\\par\n"),
            '\t' => s.push_str("\\tab "),
            c if (c as u32) < 0x80 => s.push(c),
            c => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    s.push_str(&format!("\\u{}?", *u as i16));
                }
            }
        }
    }
    s.push('}');
    s
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
        EM_STREAMIN => stream_in(hwnd, wp as u32, lp),
        EM_STREAMOUT => stream_out(hwnd, wp as u32, lp),
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
