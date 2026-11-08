//! `comdlg32` : boîtes non modales « Rechercher » (`FindText`) et
//! « Remplacer » (`ReplaceText`) (#712).
//!
//! La boîte vit dans la boucle de messages du programme. À chaque bouton,
//! elle recopie le texte et les options dans la structure FINDREPLACE du
//! programme, puis envoie le message enregistré « commdlg_FindReplace » à
//! la fenêtre propriétaire, avec cette structure en lParam. C'est le
//! programme qui cherche et remplace dans son propre texte. À la fermeture,
//! il reçoit FR_DIALOGTERM puis la boîte est détruite.
//!
//! Les programmes x86 gardent leur propre structure (40 octets) : la boîte
//! lit et écrit directement ses champs, et le message reçu par le programme
//! pointe sur elle. La procédure crochet (`FR_ENABLEHOOK`) est appelée avant
//! le traitement de la boîte, comme sous Windows. Un modèle de boîte
//! personnalisé (`FR_ENABLETEMPLATE`) n'est pas repris : la boîte de Winlux
//! est utilisée.

use super::comdlg32_ui::{cancel_label, get_text, send, set_error, set_text, tr, wide};
use super::{dialogs, user32};
use std::collections::HashMap;
use std::sync::Mutex;

pub(crate) const FR_DOWN: u32 = 0x1;
pub(crate) const FR_WHOLEWORD: u32 = 0x2;
pub(crate) const FR_MATCHCASE: u32 = 0x4;
pub(crate) const FR_FINDNEXT: u32 = 0x8;
pub(crate) const FR_REPLACE: u32 = 0x10;
pub(crate) const FR_REPLACEALL: u32 = 0x20;
pub(crate) const FR_DIALOGTERM: u32 = 0x40;
pub(crate) const FR_ENABLEHOOK: u32 = 0x100;
pub(crate) const FR_ENABLETEMPLATE: u32 = 0x200;
const FR_NOUPDOWN: u32 = 0x400;
const FR_NOMATCHCASE: u32 = 0x800;
const FR_NOWHOLEWORD: u32 = 0x1000;
const FR_ENABLETEMPLATEHANDLE: u32 = 0x2000;
const FR_HIDEUPDOWN: u32 = 0x4000;
const FR_HIDEMATCHCASE: u32 = 0x8000;
const FR_HIDEWHOLEWORD: u32 = 0x10000;

const CDERR_INITIALIZATION: u32 = 0x0002;
const CDERR_STRUCTSIZE: u32 = 0x0001;
const CDERR_DIALOGFAILURE: u32 = 0xFFFF;
const FRERR_BUFFERLENGTHZERO: u32 = 0x4001;

/// Nom public du message envoyé au propriétaire (FINDMSGSTRING).
pub(crate) const FINDMSGSTRING: &str = "commdlg_FindReplace";

// Identifiants des contrôles, ceux de dlgs.h : les programmes les lisent
// parfois avec GetDlgItem depuis leur procédure crochet.
const ID_FIND_NEXT: usize = 1; // IDOK
const ID_CANCEL: usize = 2; // IDCANCEL
const PSH_REPLACE: usize = 0x400;
const PSH_REPLACE_ALL: usize = 0x401;
const CHX_WHOLE_WORD: usize = 0x410;
const CHX_MATCH_CASE: usize = 0x411;
const RAD_UP: usize = 0x420;
const RAD_DOWN: usize = 0x421;
const GRP_DIRECTION: usize = 0x430;
const EDT_FIND: usize = 0x480;
const EDT_REPLACE: usize = 0x481;

const WM_INITDIALOG: u32 = 0x0110;
const WM_COMMAND: u32 = 0x0111;
const WM_DESTROY: u32 = 0x0002;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const EN_CHANGE: usize = 0x0300;

/// Position des champs de FINDREPLACE (64 bits : 80 octets, x86 : 40).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub size: u32,
    pub owner: usize,
    pub flags: usize,
    pub find: usize,
    pub replace: usize,
    pub find_len: usize,
    pub replace_len: usize,
    pub hook: usize,
    /// Taille d'un pointeur : 8 en 64 bits, 4 en x86.
    pub ptr: usize,
}

pub(crate) const LAYOUT64: Layout = Layout {
    size: 80,
    owner: 8,
    flags: 24,
    find: 32,
    replace: 40,
    find_len: 48,
    replace_len: 50,
    hook: 64,
    ptr: 8,
};

pub(crate) const LAYOUT32: Layout = Layout {
    size: 40,
    owner: 4,
    flags: 12,
    find: 16,
    replace: 20,
    find_len: 24,
    replace_len: 26,
    hook: 32,
    ptr: 4,
};

impl Layout {
    unsafe fn u32_at(&self, fr: usize, off: usize) -> u32 {
        std::ptr::read_unaligned((fr + off) as *const u32)
    }
    unsafe fn u16_at(&self, fr: usize, off: usize) -> u16 {
        std::ptr::read_unaligned((fr + off) as *const u16)
    }
    pub(crate) unsafe fn ptr_at(&self, fr: usize, off: usize) -> usize {
        if self.ptr == 4 {
            self.u32_at(fr, off) as usize
        } else {
            std::ptr::read_unaligned((fr + off) as *const usize)
        }
    }
    pub(crate) unsafe fn flags(&self, fr: usize) -> u32 {
        self.u32_at(fr, self.flags)
    }
    unsafe fn set_flags(&self, fr: usize, v: u32) {
        std::ptr::write_unaligned((fr + self.flags) as *mut u32, v)
    }
}

/// Une boîte ouverte.
#[derive(Clone, Copy, Debug)]
struct Find {
    /// FINDREPLACE du programme (adresse telle qu'il la voit).
    fr: usize,
    layout: Layout,
    wide: bool,
    replace: bool,
    owner: usize,
    msg: u32,
    /// Procédure crochet, appelable en 64 bits (passerelle pour le x86).
    hook: usize,
}

fn boxes() -> &'static Mutex<HashMap<usize, Find>> {
    static B: std::sync::OnceLock<Mutex<HashMap<usize, Find>>> = std::sync::OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get(dlg: usize) -> Option<Find> {
    crate::dlls::lock(boxes()).get(&dlg).copied()
}

/// Lit une chaîne du programme (au plus `max` caractères).
unsafe fn read_str(ptr: usize, max: usize, wide: bool) -> String {
    if ptr == 0 || max == 0 {
        return String::new();
    }
    if wide {
        let p = ptr as *const u16;
        let mut v = Vec::new();
        while v.len() < max {
            let c = *p.add(v.len());
            if c == 0 {
                break;
            }
            v.push(c);
        }
        String::from_utf16_lossy(&v)
    } else {
        let p = ptr as *const u8;
        let mut v = Vec::new();
        while v.len() < max {
            let c = *p.add(v.len());
            if c == 0 {
                break;
            }
            v.push(c);
        }
        crate::dlls::decode_ansi(&v)
    }
}

/// Écrit `text` dans le tampon du programme de `cap` caractères, coupé et
/// terminé par un zéro.
unsafe fn write_str(ptr: usize, cap: usize, wide: bool, text: &str) {
    if ptr == 0 || cap == 0 {
        return;
    }
    if wide {
        let units: Vec<u16> = text.encode_utf16().take(cap - 1).collect();
        let p = ptr as *mut u16;
        std::ptr::copy_nonoverlapping(units.as_ptr(), p, units.len());
        *p.add(units.len()) = 0;
    } else {
        let bytes = crate::dlls::encode_ansi(text);
        let n = bytes.len().min(cap - 1);
        let p = ptr as *mut u8;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, n);
        *p.add(n) = 0;
    }
}

fn checked(dlg: usize, id: usize) -> bool {
    let item = user32::get_dlg_item(dlg, id as i32);
    item != 0 && send(item, BM_GETCHECK, 0, 0) == 1
}

fn set_check(dlg: usize, id: usize, on: bool) {
    let item = user32::get_dlg_item(dlg, id as i32);
    if item != 0 {
        send(item, BM_SETCHECK, on as usize, 0);
    }
}

/// Suivant, Remplacer et Remplacer tout ne servent à rien sans texte.
unsafe fn update_buttons(dlg: usize) {
    let has = !get_text(user32::get_dlg_item(dlg, EDT_FIND as i32)).is_empty();
    for id in [ID_FIND_NEXT, PSH_REPLACE, PSH_REPLACE_ALL] {
        let item = user32::get_dlg_item(dlg, id as i32);
        if item != 0 {
            user32::enable_window(item, has as i32);
        }
    }
}

/// Recopie le texte et les options dans la structure du programme, puis
/// le prévient avec `action` (FR_FINDNEXT, FR_REPLACE, FR_REPLACEALL ou
/// FR_DIALOGTERM).
unsafe fn notify(dlg: usize, action: u32) {
    let Some(f) = get(dlg) else {
        return;
    };
    let l = f.layout;
    let mut flags = l.flags(f.fr) & !(FR_FINDNEXT | FR_REPLACE | FR_REPLACEALL | FR_DIALOGTERM);
    if action != FR_DIALOGTERM {
        flags &= !(FR_DOWN | FR_WHOLEWORD | FR_MATCHCASE);
        // La boîte Remplacer n'a pas de sens : elle cherche vers le bas.
        if f.replace || checked(dlg, RAD_DOWN) {
            flags |= FR_DOWN;
        }
        if checked(dlg, CHX_WHOLE_WORD) {
            flags |= FR_WHOLEWORD;
        }
        if checked(dlg, CHX_MATCH_CASE) {
            flags |= FR_MATCHCASE;
        }
        let find = get_text(user32::get_dlg_item(dlg, EDT_FIND as i32));
        let cap = l.u16_at(f.fr, l.find_len) as usize;
        write_str(l.ptr_at(f.fr, l.find), cap, f.wide, &find);
        if f.replace {
            let with = get_text(user32::get_dlg_item(dlg, EDT_REPLACE as i32));
            let cap = l.u16_at(f.fr, l.replace_len) as usize;
            write_str(l.ptr_at(f.fr, l.replace), cap, f.wide, &with);
        }
    }
    l.set_flags(f.fr, flags | action);
    if f.owner != 0 && f.msg != 0 {
        send(f.owner, f.msg, 0, f.fr as isize);
    }
}

unsafe fn init(dlg: usize, f: &Find) {
    let l = f.layout;
    let flags = l.flags(f.fr);
    let label = |text: &str, rect| {
        dialogs::add_control(dlg, "STATIC", text, 0xFFFF, 0, 0, rect);
    };
    let button = |text: &str, id: usize, style: u32, y: i32| {
        dialogs::add_control(
            dlg,
            "BUTTON",
            text,
            id,
            0x0001_0000 | style,
            0,
            (366, y, 112, 28),
        );
    };
    let edit = |id: usize, y: i32| {
        // WS_TABSTOP | ES_AUTOHSCROLL, bord creux.
        dialogs::add_control(
            dlg,
            "EDIT",
            "",
            id,
            0x0001_0000 | 0x80,
            0x200,
            (124, y, 230, 26),
        )
    };
    label(
        tr("&Rechercher :", "Fi&nd what:", "Su&chen nach:", "&Buscar:"),
        (12, 16, 110, 20),
    );
    let find = edit(EDT_FIND, 12);
    let mut y = 50;
    if f.replace {
        label(
            tr(
                "Rem&placer par :",
                "Re&place with:",
                "Ersetzen &durch:",
                "Ree&mplazar con:",
            ),
            (12, 52, 110, 20),
        );
        edit(EDT_REPLACE, 48);
        y = 90;
    }
    // Options.
    let check_w = if f.replace { 340 } else { 210 };
    let options = [
        (
            CHX_WHOLE_WORD,
            tr(
                "Mot &entier uniquement",
                "Match &whole word only",
                "Nur &ganzes Wort",
                "Solo palabras &completas",
            ),
            FR_WHOLEWORD,
            FR_HIDEWHOLEWORD,
            FR_NOWHOLEWORD,
        ),
        (
            CHX_MATCH_CASE,
            tr(
                "Respecter la &casse",
                "Match &case",
                "&Groß-/Kleinschreibung",
                "&Mayúsculas/minúsculas",
            ),
            FR_MATCHCASE,
            FR_HIDEMATCHCASE,
            FR_NOMATCHCASE,
        ),
    ];
    let mut oy = y;
    for (id, text, on, hide, disable) in options {
        if flags & hide != 0 {
            continue;
        }
        // BS_AUTOCHECKBOX | WS_TABSTOP.
        let item = dialogs::add_control(
            dlg,
            "BUTTON",
            text,
            id,
            0x3 | 0x0001_0000,
            0,
            (12, oy, check_w, 22),
        );
        set_check(dlg, id, flags & on != 0);
        if flags & disable != 0 {
            user32::enable_window(item, 0);
        }
        oy += 26;
    }
    // Direction (Rechercher seulement).
    if !f.replace && flags & FR_HIDEUPDOWN == 0 {
        dialogs::add_control(
            dlg,
            "BUTTON",
            tr("Direction", "Direction", "Richtung", "Dirección"),
            GRP_DIRECTION,
            0x7, // BS_GROUPBOX
            0,
            (230, 46, 124, 74),
        );
        let up = dialogs::add_control(
            dlg,
            "BUTTON",
            tr("&Haut", "&Up", "&Aufwärts", "&Arriba"),
            RAD_UP,
            0x9 | 0x0002_0000 | 0x0001_0000, // BS_AUTORADIOBUTTON | WS_GROUP | WS_TABSTOP
            0,
            (240, 68, 108, 20),
        );
        let down = dialogs::add_control(
            dlg,
            "BUTTON",
            tr("&Bas", "&Down", "A&bwärts", "Abaj&o"),
            RAD_DOWN,
            0x9,
            0,
            (240, 92, 108, 20),
        );
        set_check(dlg, RAD_DOWN, flags & FR_DOWN != 0);
        set_check(dlg, RAD_UP, flags & FR_DOWN == 0);
        if flags & FR_NOUPDOWN != 0 {
            user32::enable_window(up, 0);
            user32::enable_window(down, 0);
        }
    }
    // Boutons, à droite.
    button(
        tr(
            "&Suivant",
            "&Find Next",
            "&Weitersuchen",
            "Buscar &siguiente",
        ),
        ID_FIND_NEXT,
        0x1, // BS_DEFPUSHBUTTON
        12,
    );
    let mut by = 46;
    if f.replace {
        button(
            tr("&Remplacer", "&Replace", "&Ersetzen", "&Reemplazar"),
            PSH_REPLACE,
            0,
            46,
        );
        button(
            tr(
                "Remplacer &tout",
                "Replace &All",
                "&Alle ersetzen",
                "Reemplazar &todo",
            ),
            PSH_REPLACE_ALL,
            0,
            80,
        );
        by = 114;
    }
    button(cancel_label(), ID_CANCEL, 0, by);
    // Texte de départ : celui que le programme a mis dans son tampon.
    let cap = l.u16_at(f.fr, l.find_len) as usize;
    set_text(find, &read_str(l.ptr_at(f.fr, l.find), cap, f.wide));
    if f.replace {
        let cap = l.u16_at(f.fr, l.replace_len) as usize;
        let with = read_str(l.ptr_at(f.fr, l.replace), cap, f.wide);
        set_text(user32::get_dlg_item(dlg, EDT_REPLACE as i32), &with);
    }
    update_buttons(dlg);
    user32::set_focus(find);
    send(find, 0x00B1, 0, -1); // EM_SETSEL : tout le texte sélectionné
}

unsafe fn call_hook(hook: usize, dlg: usize, msg: u32, wp: usize, lp: isize) -> isize {
    let f: extern "win64" fn(usize, u32, usize, isize) -> isize = std::mem::transmute(hook);
    f(dlg, msg, wp, lp)
}

unsafe extern "win64" fn find_proc(dlg: usize, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == WM_INITDIALOG {
        let f = *(lp as *const Find);
        crate::dlls::lock(boxes()).insert(dlg, f);
        init(dlg, &f);
        if f.hook != 0 {
            // Le crochet reçoit la structure du programme, comme sous Windows.
            call_hook(f.hook, dlg, msg, wp, f.fr as isize);
        }
        return 0; // le focus est déjà sur le champ de recherche
    }
    let Some(f) = get(dlg) else {
        return 0;
    };
    if f.hook != 0 {
        let r = call_hook(f.hook, dlg, msg, wp, lp);
        if r != 0 && msg != WM_DESTROY {
            return r;
        }
    }
    match msg {
        WM_COMMAND => {
            let id = wp & 0xFFFF;
            let code = (wp >> 16) & 0xFFFF;
            match (id, code) {
                (ID_FIND_NEXT, _) => notify(dlg, FR_FINDNEXT),
                (PSH_REPLACE, 0) => notify(dlg, FR_REPLACE),
                (PSH_REPLACE_ALL, 0) => notify(dlg, FR_REPLACEALL),
                (ID_CANCEL, _) => {
                    notify(dlg, FR_DIALOGTERM);
                    user32::destroy_window(dlg);
                }
                (RAD_UP | RAD_DOWN, 0) => {
                    set_check(dlg, RAD_UP, id == RAD_UP);
                    set_check(dlg, RAD_DOWN, id == RAD_DOWN);
                }
                (EDT_FIND, EN_CHANGE) => update_buttons(dlg),
                _ => {}
            }
            1
        }
        WM_DESTROY => {
            crate::dlls::lock(boxes()).remove(&dlg);
            0
        }
        _ => 0,
    }
}

/// FindText / ReplaceText. `fr` est la structure telle que le programme la
/// voit ; `hook` est la procédure crochet déjà rendue appelable en 64 bits
/// (0 sans crochet).
pub(crate) unsafe fn open(
    fr: usize,
    layout: Layout,
    wide: bool,
    replace: bool,
    hook: usize,
) -> usize {
    set_error(0);
    if fr == 0 {
        set_error(CDERR_INITIALIZATION);
        return 0;
    }
    let l = layout;
    if l.u32_at(fr, 0) != l.size {
        set_error(CDERR_STRUCTSIZE);
        return 0;
    }
    let flags = l.flags(fr);
    if l.ptr_at(fr, l.find) == 0
        || l.u16_at(fr, l.find_len) == 0
        || (replace && (l.ptr_at(fr, l.replace) == 0 || l.u16_at(fr, l.replace_len) == 0))
    {
        set_error(FRERR_BUFFERLENGTHZERO);
        return 0;
    }
    let owner = l.ptr_at(fr, l.owner);
    if owner == 0 || user32::is_window(owner) == 0 {
        set_error(CDERR_DIALOGFAILURE);
        return 0;
    }
    if flags & (FR_ENABLETEMPLATE | FR_ENABLETEMPLATEHANDLE) != 0 {
        crate::trace!(
            "comdlg32 : FindText/ReplaceText, modèle personnalisé ignoré (boîte de Winlux)"
        );
    }
    let name = wide_name();
    let msg = user32::register_window_message_w(name.as_ptr());
    let f = Find {
        fr,
        layout,
        wide,
        replace,
        owner,
        msg,
        hook: if flags & FR_ENABLEHOOK != 0 { hook } else { 0 },
    };
    let (title, h) = if replace {
        (tr("Remplacer", "Replace", "Ersetzen", "Reemplazar"), 156)
    } else {
        (tr("Rechercher", "Find", "Suchen", "Buscar"), 132)
    };
    let dlg = dialogs::create_builtin(
        owner,
        title,
        490,
        h,
        find_proc as *const () as usize,
        &f as *const Find as isize,
    );
    if dlg == 0 {
        set_error(CDERR_DIALOGFAILURE);
    }
    dlg
}

fn wide_name() -> Vec<u16> {
    wide(FINDMSGSTRING)
}

unsafe fn open64(fr: *mut u8, wide: bool, replace: bool) -> usize {
    let fr = fr as usize;
    let hook = if fr != 0 {
        LAYOUT64.ptr_at(fr, LAYOUT64.hook)
    } else {
        0
    };
    open(fr, LAYOUT64, wide, replace, hook)
}

pub unsafe extern "win64" fn find_text_a(fr: *mut u8) -> usize {
    open64(fr, false, false)
}
pub unsafe extern "win64" fn find_text_w(fr: *mut u8) -> usize {
    open64(fr, true, false)
}
pub unsafe extern "win64" fn replace_text_a(fr: *mut u8) -> usize {
    open64(fr, false, true)
}
pub unsafe extern "win64" fn replace_text_w(fr: *mut u8) -> usize {
    open64(fr, true, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structures_et_chaines() {
        // FINDREPLACE : 80 octets en 64 bits, 40 en x86 (commdlg.h).
        assert_eq!(LAYOUT64.hook + 8 + 8, LAYOUT64.size as usize);
        assert_eq!(LAYOUT32.hook + 4 + 4, LAYOUT32.size as usize);
        let mut buf = [0u16; 4];
        unsafe {
            write_str(buf.as_mut_ptr() as usize, 4, true, "abcdef");
            assert_eq!(read_str(buf.as_ptr() as usize, 4, true), "abc");
        }
        let mut a = [0u8; 8];
        unsafe {
            write_str(a.as_mut_ptr() as usize, 8, false, "été");
            assert_eq!(read_str(a.as_ptr() as usize, 8, false), "été");
        }
    }

    #[test]
    fn erreurs_avant_la_boite() {
        unsafe {
            assert_eq!(find_text_w(std::ptr::null_mut()), 0);
            assert_eq!(
                super::super::comdlg32::comm_dlg_extended_error(),
                CDERR_INITIALIZATION
            );
            let mut fr = [0u8; 80];
            fr[0] = 40; // mauvaise taille
            assert_eq!(find_text_w(fr.as_mut_ptr()), 0);
            assert_eq!(
                super::super::comdlg32::comm_dlg_extended_error(),
                CDERR_STRUCTSIZE
            );
            fr[0] = 80; // tampon absent
            assert_eq!(replace_text_w(fr.as_mut_ptr()), 0);
            assert_eq!(
                super::super::comdlg32::comm_dlg_extended_error(),
                FRERR_BUFFERLENGTHZERO
            );
        }
    }
}
