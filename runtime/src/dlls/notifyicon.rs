//! Zone de notification : `Shell_NotifyIconA/W`.
//!
//! Chaque icône déclarée par le programme est affichée dans la zone de
//! notification du panneau par un petit assistant GTK (`winlux-notify-icon`,
//! une icône XEmbed par entrée), lancé à la première icône visible. Les clics
//! reviennent par son flux de sortie et sont transmis à la fenêtre du programme
//! avec son `uCallbackMessage`, au format de la version de l'icône (classique
//! ou `NOTIFYICON_VERSION_4`). Sans affichage ni assistant, l'appel réussit
//! quand même : le programme garde le même comportement que sous Windows.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};

pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIM_SETFOCUS: u32 = 3;
pub const NIM_SETVERSION: u32 = 4;

const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;
const NIF_STATE: u32 = 0x08;
const NIF_INFO: u32 = 0x10;
const NIS_HIDDEN: u32 = 0x01;

const WM_CONTEXTMENU: u32 = 0x007B;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const NIN_SELECT: u32 = 0x0400;
const NIN_BALLOONSHOW: u32 = 0x0402;
const NIN_BALLOONUSERCLICK: u32 = 0x0405;

/// Champs utiles d'un `NOTIFYICONDATA` (A ou W), déjà décodés.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IconData {
    pub hwnd: usize,
    pub id: u32,
    pub flags: u32,
    pub callback: u32,
    pub icon: usize,
    pub tip: Option<String>,
    pub state: u32,
    pub state_mask: u32,
    pub info: Option<(String, String, u32)>,
    pub version: u32,
}

#[derive(Clone, Debug, Default)]
struct Icon {
    callback: u32,
    tip: String,
    hidden: bool,
    version: u32,
    /// Dernière position du pointeur sur l'icône (coordonnées écran),
    /// pour Shell_NotifyIconGetRect.
    pos: Option<(i32, i32)>,
}

#[derive(Default)]
struct State {
    icons: HashMap<(usize, u32), Icon>,
    helper: Option<(Child, ChildStdin)>,
    helper_failed: bool,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(|| Mutex::new(State::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Lit une structure `NOTIFYICONDATA` de `cbSize` octets (x64).
///
/// # Safety
/// `data` doit pointer sur au moins `cbSize` octets lisibles.
pub unsafe fn read_icon_data(data: *const u8, wide: bool) -> Option<IconData> {
    if data.is_null() {
        return None;
    }
    let size = (data as *const u32).read_unaligned() as usize;
    if size < 40 {
        return None;
    }
    let u32_at = |offset: usize| (data.add(offset) as *const u32).read_unaligned();
    let usize_at = |offset: usize| (data.add(offset) as *const usize).read_unaligned();
    let char_size = if wide { 2 } else { 1 };
    let text = |offset: usize, chars: usize| -> String {
        if offset + chars * char_size > size {
            return String::new();
        }
        if wide {
            let units: Vec<u16> = (0..chars)
                .map(|i| (data.add(offset + i * 2) as *const u16).read_unaligned())
                .take_while(|&c| c != 0)
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            let bytes: Vec<u8> = (0..chars)
                .map(|i| *data.add(offset + i))
                .take_while(|&c| c != 0)
                .collect();
            bytes.iter().map(|&b| b as char).collect()
        }
    };
    let mut d = IconData {
        hwnd: usize_at(8),
        id: u32_at(16),
        flags: u32_at(20),
        callback: u32_at(24),
        icon: usize_at(32),
        ..IconData::default()
    };
    // Versions 1 (64 caractères d'infobulle) puis 2+ (128, état, bulle).
    let long_tip = 40 + 128 * char_size;
    let tip_chars = if size >= long_tip + 8 { 128 } else { 64 };
    if d.flags & NIF_TIP != 0 {
        d.tip = Some(text(40, tip_chars));
    }
    if size >= long_tip + 8 {
        d.state = u32_at(long_tip);
        d.state_mask = u32_at(long_tip + 4);
        let info = long_tip + 8;
        let timeout = info + 256 * char_size;
        let title = timeout + 4;
        let flags = title + 64 * char_size;
        if size >= flags + 4 {
            d.version = u32_at(timeout);
            if d.flags & NIF_INFO != 0 {
                d.info = Some((text(title, 64), text(info, 256), u32_at(flags)));
            }
        }
    }
    Some(d)
}

fn clean(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c == '\t' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn key(hwnd: usize, id: u32) -> String {
    format!("{hwnd:x}-{id}")
}

/// Ligne envoyée à l'assistant pour afficher ou mettre à jour une icône.
fn set_line(hwnd: usize, id: u32, icon: &Icon, source: &str) -> String {
    format!(
        "set\t{}\t{}\t{}\t{}\n",
        key(hwnd, id),
        clean(source),
        clean(&icon.tip),
        if icon.hidden { 0 } else { 1 }
    )
}

fn icon_source() -> String {
    crate::loader::main_module()
        .map(|m| m.path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn helper_program() -> Option<String> {
    if let Ok(path) = std::env::var("WINLUX_NOTIFY_ICON_HELPER") {
        return (!path.is_empty()).then_some(path);
    }
    std::env::var_os("DISPLAY")?;
    [
        "/usr/bin/winlux-notify-icon",
        "/usr/local/bin/winlux-notify-icon",
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).is_file())
    .map(str::to_string)
}

/// Envoie une ligne à l'assistant, lancé à la demande.
fn send(st: &mut State, line: &str) {
    if st.helper.is_none() && !st.helper_failed {
        st.helper_failed = true;
        if let Some(program) = helper_program() {
            let child = Command::new(program)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = child {
                if let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) {
                    std::thread::spawn(move || {
                        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                            deliver(&line);
                        }
                    });
                    st.helper = Some((child, stdin));
                    st.helper_failed = false;
                }
            }
        }
    }
    let failed = match st.helper.as_mut() {
        Some((_, stdin)) => stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.flush())
            .is_err(),
        None => false,
    };
    if failed {
        if let Some((mut child, _)) = st.helper.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Messages Win32 d'un événement de l'assistant (`ldown`, `rup`, `select`…).
pub fn callback_messages(
    version: u32,
    id: u32,
    event: &str,
    x: i32,
    y: i32,
) -> Vec<(usize, isize)> {
    let events: &[u32] = match event {
        "move" => &[WM_MOUSEMOVE],
        "ldown" => &[WM_LBUTTONDOWN],
        "lup" => &[WM_LBUTTONUP, NIN_SELECT],
        "ldbl" => &[WM_LBUTTONDBLCLK],
        "rdown" => &[WM_RBUTTONDOWN],
        "rup" => &[WM_RBUTTONUP, WM_CONTEXTMENU],
        "mdown" => &[WM_MBUTTONDOWN],
        "mup" => &[WM_MBUTTONUP],
        "balloon-show" => &[NIN_BALLOONSHOW],
        "balloon-click" => &[NIN_BALLOONUSERCLICK],
        _ => &[],
    };
    events
        .iter()
        // Les notifications NIN_SELECT / WM_CONTEXTMENU n'existent qu'à partir
        // de la version 3 (Windows 2000 les ajoute derrière NOTIFYICON_VERSION).
        .filter(|&&m| version >= 3 || (m != NIN_SELECT && m != WM_CONTEXTMENU))
        .map(|&m| {
            if version >= 4 {
                let point = ((y as u16 as usize) << 16) | (x as u16 as usize);
                (point, ((id as isize & 0xFFFF) << 16) | m as isize)
            } else {
                (id as usize, m as isize)
            }
        })
        .collect()
}

/// Identifiant de l'entrée « Ranger dans les icônes cachées » ajoutée au
/// menu du programme après un clic droit sur son icône (#927). Sous la plage
/// des commandes système (0xF000), loin des identifiants usuels.
pub const TRAY_HIDE_ID: u32 = 0xE927;

/// Clic droit sur une icône : le prochain menu surgissant du programme
/// (dans les 3 s) reçoit l'entrée « Ranger dans les icônes cachées ».
static TRAY_MENU: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

/// Nom de l'icône pour la zone de notification d'XFCE : celui que
/// `winlux-notify-icon` donne à son processus (nom du .exe, en minuscules).
pub fn overflow_name(source: &str) -> String {
    let file = source.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = match file.rfind('.') {
        Some(i) if i > 0 => &file[..i],
        _ => file,
    };
    let cleaned: String = stem
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '-' || c == '.');
    if cleaned.is_empty() {
        "winlux-programme".to_string()
    } else {
        cleaned.to_string()
    }
}

/// Entrée du menu, dans la langue de l'interface.
pub fn hide_label() -> &'static str {
    let lang = crate::intl::locale::ui_language_tag().unwrap_or_default();
    match lang.get(..2).unwrap_or("") {
        "en" => "Move to hidden icons",
        "es" => "Mover a iconos ocultos",
        "de" => "In ausgeblendete Symbole verschieben",
        _ => "Ranger dans les icônes cachées",
    }
}

/// Clic droit reçu sur l'icône `name` (aussi pour les tests du menu).
pub fn note_tray_menu(name: &str) {
    *TRAY_MENU.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((name.to_string(), std::time::Instant::now()));
}

/// Nom de l'icône dont le clic droit vient d'ouvrir un menu, s'il est récent.
pub fn take_tray_menu() -> Option<String> {
    let mut pending = TRAY_MENU.lock().unwrap_or_else(|e| e.into_inner());
    match pending.take() {
        Some((name, at)) if at.elapsed() < std::time::Duration::from_secs(3) => Some(name),
        _ => None,
    }
}

/// Range l'icône derrière la flèche (winlux-tray overflow hide), en direct.
pub fn hide_in_overflow(name: &str) {
    for tray in ["/usr/bin/winlux-tray", "/usr/local/bin/winlux-tray"] {
        if std::path::Path::new(tray).is_file() {
            let _ = Command::new(tray)
                .args(["overflow", "hide", name])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            return;
        }
    }
}

/// Ligne de l'assistant : `<clé>\t<événement>\t<x>\t<y>`.
fn deliver(line: &str) {
    let mut fields = line.split('\t');
    let (Some(target), Some(event)) = (fields.next(), fields.next()) else {
        return;
    };
    let x = fields.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let y = fields.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let Some((hwnd, id)) = target.split_once('-') else {
        return;
    };
    let (Ok(hwnd), Ok(id)) = (usize::from_str_radix(hwnd, 16), id.parse::<u32>()) else {
        return;
    };
    let Some(icon) = state().icons.get(&(hwnd, id)).cloned() else {
        crate::thunks::api_note(&format!(
            "zone de notification : {event} sur {target}, icône inconnue (#1222)"
        ));
        return;
    };
    if !event.starts_with("balloon") && (x, y) != (0, 0) {
        if let Some(entry) = state().icons.get_mut(&(hwnd, id)) {
            entry.pos = Some((x, y));
        }
    }
    if event == "rup" {
        if icon.callback == 0 {
            // Programme sans menu : l'assistant ouvre le sien (#927).
            let line = format!("menu\t{}\n", key(hwnd, id));
            send(&mut state(), &line);
            return;
        }
        note_tray_menu(&overflow_name(&icon_source()));
    }
    if icon.callback == 0 {
        crate::thunks::api_note(&format!(
            "zone de notification : {event} sur {target}, pas de message de rappel (#1222)"
        ));
        return;
    }
    for (wparam, lparam) in callback_messages(icon.version, id, event, x, y) {
        let posted = crate::dlls::gui::post_to_window(
            hwnd,
            crate::dlls::gui::Msg::new(hwnd, icon.callback, wparam, lparam),
        );
        crate::thunks::api_note(&format!(
            "zone de notification : {event} sur {target} → message {:#x} ({wparam:#x}, {lparam:#x}) {} (#1222)",
            icon.callback,
            if posted { "posté" } else { "refusé : fenêtre inconnue" }
        ));
    }
}

/// Applique une commande `NIM_*` ; renvoie le BOOL de `Shell_NotifyIcon`.
pub fn apply(message: u32, data: &IconData) -> bool {
    crate::thunks::api_note(&format!(
        "Shell_NotifyIcon({message}) : fenêtre {:#x}, id {}, drapeaux {:#x}, message {:#x}, version {} (#1222)",
        data.hwnd, data.id, data.flags, data.callback, data.version
    ));
    let mut st = state();
    let k = (data.hwnd, data.id);
    match message {
        NIM_ADD | NIM_MODIFY => {
            if message == NIM_ADD && st.icons.contains_key(&k) {
                return false;
            }
            if message == NIM_MODIFY && !st.icons.contains_key(&k) {
                return false;
            }
            let icon = st.icons.entry(k).or_default();
            if data.flags & NIF_MESSAGE != 0 {
                icon.callback = data.callback;
            }
            if let Some(tip) = &data.tip {
                icon.tip = tip.clone();
            }
            if data.flags & NIF_STATE != 0 && data.state_mask & NIS_HIDDEN != 0 {
                icon.hidden = data.state & NIS_HIDDEN != 0;
            }
            let icon = icon.clone();
            if data.flags & (NIF_ICON | NIF_TIP | NIF_STATE) != 0 || message == NIM_ADD {
                let line = set_line(data.hwnd, data.id, &icon, &icon_source());
                send(&mut st, &line);
            }
            if let Some((title, text, flags)) = &data.info {
                if !title.is_empty() || !text.is_empty() {
                    let line = format!(
                        "balloon\t{}\t{}\t{}\t{}\n",
                        key(data.hwnd, data.id),
                        clean(title),
                        clean(text),
                        flags
                    );
                    send(&mut st, &line);
                }
            }
            true
        }
        NIM_DELETE => {
            if st.icons.remove(&k).is_none() {
                return false;
            }
            let line = format!("delete\t{}\n", key(data.hwnd, data.id));
            send(&mut st, &line);
            true
        }
        NIM_SETVERSION => match st.icons.get_mut(&k) {
            Some(icon) => {
                icon.version = data.version;
                true
            }
            None => false,
        },
        NIM_SETFOCUS => st.icons.contains_key(&k),
        _ => false,
    }
}

/// Taille d'une icône de la zone de notification (unités logiques).
const TRAY_ICON_SIZE: i32 = 24;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

/// Rectangle écran d'une icône : centré sur le dernier passage du pointeur,
/// sinon dans le coin de la zone de travail, là où se trouve la zone de
/// notification du panneau.
fn icon_rect(hwnd: usize, id: u32) -> Option<crate::dlls::gui::Rect> {
    let pos = state().icons.get(&(hwnd, id))?.pos;
    let half = TRAY_ICON_SIZE / 2;
    let (cx, cy) = pos.unwrap_or_else(|| {
        let area = crate::dlls::gui::work_area();
        let (_, screen_bottom) = crate::dlls::gui::screen_size();
        // Barre en bas : l'icône est sous la zone de travail.
        let y = if area.bottom < screen_bottom {
            (area.bottom + screen_bottom) / 2
        } else {
            area.bottom - half
        };
        (area.right - TRAY_ICON_SIZE * 3, y)
    });
    Some(crate::dlls::gui::Rect {
        left: cx - half,
        top: cy - half,
        right: cx - half + TRAY_ICON_SIZE,
        bottom: cy - half + TRAY_ICON_SIZE,
    })
}

/// `Shell_NotifyIconGetRect(NOTIFYICONIDENTIFIER*, RECT*)` : Chromium la
/// cherche par GetProcAddress au clic droit sur l'icône et s'arrête net
/// (CHECK) si shell32 ne l'exporte pas (#1191).
pub unsafe extern "win64" fn shell_notify_icon_get_rect(ident: *const u8, out: *mut i32) -> i32 {
    if ident.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let u32_at = |offset: usize| (ident.add(offset) as *const u32).read_unaligned();
    // NOTIFYICONIDENTIFIER : cbSize, hWnd, uID, guidItem (x64 : 40 octets,
    // x86 : 28 octets).
    let (hwnd, id) = if crate::wow32::active() {
        (u32_at(4) as usize, u32_at(8))
    } else {
        ((ident.add(8) as *const usize).read_unaligned(), u32_at(16))
    };
    let Some(r) = icon_rect(hwnd, id) else {
        return E_FAIL;
    };
    for (i, v) in [r.left, r.top, r.right, r.bottom].into_iter().enumerate() {
        out.add(i).write_unaligned(v);
    }
    0
}

pub unsafe extern "win64" fn shell_notify_icon_a(message: u32, data: *const u8) -> i32 {
    match read_icon_data(data, false) {
        Some(d) => apply(message, &d) as i32,
        None => 0,
    }
}

pub unsafe extern "win64" fn shell_notify_icon_w(message: u32, data: *const u8) -> i32 {
    match read_icon_data(data, true) {
        Some(d) => apply(message, &d) as i32,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide_data(hwnd: usize, id: u32, flags: u32, tip: &str) -> Vec<u8> {
        let mut data = vec![0u8; 976];
        data[0..4].copy_from_slice(&976u32.to_le_bytes());
        data[8..16].copy_from_slice(&hwnd.to_le_bytes());
        data[16..20].copy_from_slice(&id.to_le_bytes());
        data[20..24].copy_from_slice(&flags.to_le_bytes());
        data[24..28].copy_from_slice(&0x8001u32.to_le_bytes());
        for (i, unit) in tip.encode_utf16().enumerate() {
            data[40 + i * 2..42 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        data
    }

    #[test]
    fn notify_icon_data_w_is_decoded_with_tip_state_and_balloon() {
        let mut data = wide_data(
            0x1234,
            7,
            NIF_MESSAGE | NIF_TIP | NIF_INFO,
            "Lecteur — prêt",
        );
        data[296..300].copy_from_slice(&NIS_HIDDEN.to_le_bytes());
        for (i, unit) in "Terminé".encode_utf16().enumerate() {
            data[304 + i * 2..306 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        data[816..820].copy_from_slice(&4u32.to_le_bytes());
        for (i, unit) in "Export".encode_utf16().enumerate() {
            data[820 + i * 2..822 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        let d = unsafe { read_icon_data(data.as_ptr(), true) }.unwrap();
        assert_eq!((d.hwnd, d.id, d.callback), (0x1234, 7, 0x8001));
        assert_eq!(d.tip.as_deref(), Some("Lecteur — prêt"));
        assert_eq!(d.state, NIS_HIDDEN);
        assert_eq!(d.version, 4);
        assert_eq!(d.info, Some(("Export".into(), "Terminé".into(), 0)));
    }

    #[test]
    fn overflow_name_matches_the_helper() {
        assert_eq!(overflow_name("/opt/apps/putty.exe"), "putty");
        assert_eq!(overflow_name("C:\\Programmes\\FileZilla.exe"), "filezilla");
        assert_eq!(overflow_name("Mon Appli (x86).exe"), "mon-appli--x86");
        assert_eq!(overflow_name(""), "winlux-programme");
    }

    #[test]
    fn old_ansi_structure_keeps_a_64_character_tip() {
        let mut data = vec![0u8; 104];
        data[0..4].copy_from_slice(&104u32.to_le_bytes());
        data[20..24].copy_from_slice(&NIF_TIP.to_le_bytes());
        data[40..46].copy_from_slice(b"Ancien");
        let d = unsafe { read_icon_data(data.as_ptr(), false) }.unwrap();
        assert_eq!(d.tip.as_deref(), Some("Ancien"));
        assert_eq!(d.info, None);
    }

    #[test]
    fn clicks_follow_the_icon_version() {
        // Classique : wParam = uID, lParam = message souris.
        assert_eq!(
            callback_messages(0, 3, "rup", 10, 20),
            vec![(3, WM_RBUTTONUP as isize)]
        );
        // Version 4 : lParam = MAKELONG(message, uID), wParam = position.
        assert_eq!(
            callback_messages(4, 3, "lup", 10, 20),
            vec![
                ((20 << 16) | 10, (3 << 16) | WM_LBUTTONUP as isize),
                ((20 << 16) | 10, (3 << 16) | NIN_SELECT as isize),
            ]
        );
        assert_eq!(
            callback_messages(3, 1, "rup", 0, 0)[1],
            (1, WM_CONTEXTMENU as isize)
        );
        assert!(callback_messages(0, 1, "inconnu", 0, 0).is_empty());
    }

    #[test]
    fn add_modify_delete_follow_win32_results_without_a_display() {
        std::env::set_var("WINLUX_NOTIFY_ICON_HELPER", "");
        let mut d = IconData {
            hwnd: 0x77,
            id: 1,
            flags: NIF_MESSAGE | NIF_TIP,
            callback: 0x8010,
            tip: Some("Sauvegarde".into()),
            ..IconData::default()
        };
        assert!(!apply(NIM_MODIFY, &d));
        assert!(apply(NIM_ADD, &d));
        assert!(!apply(NIM_ADD, &d));
        d.version = 4;
        assert!(apply(NIM_SETVERSION, &d));
        d.flags = NIF_STATE;
        d.state = NIS_HIDDEN;
        d.state_mask = NIS_HIDDEN;
        assert!(apply(NIM_MODIFY, &d));
        assert!(state().icons[&(0x77, 1)].hidden);
        assert!(apply(NIM_DELETE, &d));
        assert!(!apply(NIM_DELETE, &d));
    }

    #[test]
    fn rectangle_de_l_icone_pour_get_rect() {
        let mut d = IconData {
            hwnd: 0x1191,
            id: 3,
            flags: NIF_MESSAGE,
            callback: 0x8001,
            ..IconData::default()
        };
        assert!(apply(NIM_ADD, &d));
        let mut ident = [0u8; 40];
        ident[..4].copy_from_slice(&40u32.to_le_bytes());
        ident[8..16].copy_from_slice(&0x1191usize.to_le_bytes());
        ident[16..20].copy_from_slice(&3u32.to_le_bytes());
        let mut r = [0i32; 4];
        unsafe {
            assert_eq!(
                shell_notify_icon_get_rect(ident.as_ptr(), r.as_mut_ptr()),
                0
            );
            assert_eq!(r[2] - r[0], TRAY_ICON_SIZE);
            deliver("1191-3\tmove\t500\t700");
            assert_eq!(
                shell_notify_icon_get_rect(ident.as_ptr(), r.as_mut_ptr()),
                0
            );
            assert_eq!(r, [488, 688, 512, 712]);
            ident[16..20].copy_from_slice(&4u32.to_le_bytes());
            assert_eq!(
                shell_notify_icon_get_rect(ident.as_ptr(), r.as_mut_ptr()),
                E_FAIL
            );
        }
        d.flags = 0;
        assert!(apply(NIM_DELETE, &d));
    }

    #[test]
    fn helper_lines_never_carry_tabs_or_newlines() {
        let icon = Icon {
            tip: "a\tb\nc".into(),
            ..Icon::default()
        };
        assert_eq!(
            set_line(0x10, 2, &icon, "/x/app.exe"),
            "set\t10-2\t/x/app.exe\ta b c\t1\n"
        );
    }
}
