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
        return;
    };
    if icon.callback == 0 {
        return;
    }
    for (wparam, lparam) in callback_messages(icon.version, id, event, x, y) {
        crate::dlls::gui::post_to_window(
            hwnd,
            crate::dlls::gui::Msg::new(hwnd, icon.callback, wparam, lparam),
        );
    }
}

/// Applique une commande `NIM_*` ; renvoie le BOOL de `Shell_NotifyIcon`.
pub fn apply(message: u32, data: &IconData) -> bool {
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
