//! Écrans multiples côté Win32 : `EnumDisplayMonitors`, `GetMonitorInfo`,
//! `MonitorFrom*` et les métriques d'écran virtuel, d'après les moniteurs
//! RandR du serveur X (voir `runtime/src/display` pour leur disposition).
//!
//! Les HMONITOR valent 1..=N, l'écran principal en premier : les programmes
//! qui supposaient un seul moniteur (handle 1) restent corrects.
//! Coordonnées : celles de l'écran X en pixels logiques. Sous Windows le
//! principal est en (0, 0) ; ici il peut être décalé quand un écran est à
//! sa gauche, ce que les programmes gèrent via `rcMonitor`.

use super::gui::{self, Rect};

pub const MONITOR_DEFAULTTONULL: u32 = 0;
pub const MONITOR_DEFAULTTONEAREST: u32 = 2;
const MONITORINFOF_PRIMARY: u32 = 1;

/// Écrans, principal en premier ; jamais vide.
pub fn list() -> Vec<gui::Monitor> {
    let mut v = gui::monitors();
    if v.is_empty() {
        let (w, h) = gui::screen_size();
        v.push(gui::Monitor {
            rect: Rect {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            primary: true,
        });
    }
    v.sort_by_key(|m| (!m.primary, m.rect.left, m.rect.top));
    if !v.iter().any(|m| m.primary) {
        v[0].primary = true;
    }
    v
}

fn distance(r: &Rect, x: i32, y: i32) -> i64 {
    let dx = i64::from((r.left - x).max(0).max(x - (r.right - 1)));
    let dy = i64::from((r.top - y).max(0).max(y - (r.bottom - 1)));
    dx * dx + dy * dy
}

fn overlap(a: &Rect, b: &Rect) -> i64 {
    let w = i64::from(a.right.min(b.right) - a.left.max(b.left)).max(0);
    let h = i64::from(a.bottom.min(b.bottom) - a.top.max(b.top)).max(0);
    w * h
}

/// Handle du moniteur contenant le point (ou le plus proche / le principal
/// selon `flags`).
pub fn from_point_in(list: &[gui::Monitor], x: i32, y: i32, flags: u32) -> usize {
    if let Some(i) = list.iter().position(|m| distance(&m.rect, x, y) == 0) {
        return i + 1;
    }
    match flags {
        MONITOR_DEFAULTTONULL => 0,
        MONITOR_DEFAULTTONEAREST => list
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| distance(&m.rect, x, y))
            .map_or(1, |(i, _)| i + 1),
        // MONITOR_DEFAULTTOPRIMARY (1) et valeurs inconnues.
        _ => 1,
    }
}

/// Moniteur qui recouvre le plus le rectangle.
pub fn from_rect_in(list: &[gui::Monitor], r: &Rect, flags: u32) -> usize {
    let best = list
        .iter()
        .enumerate()
        .map(|(i, m)| (i, overlap(&m.rect, r)))
        .max_by_key(|(_, a)| *a);
    match best {
        Some((i, a)) if a > 0 => i + 1,
        _ => from_point_in(list, (r.left + r.right) / 2, (r.top + r.bottom) / 2, flags),
    }
}

/// Rectangle englobant tous les écrans (SM_*VIRTUALSCREEN).
pub fn virtual_rect_in(list: &[gui::Monitor]) -> Rect {
    let mut r = list[0].rect;
    for m in &list[1..] {
        r.left = r.left.min(m.rect.left);
        r.top = r.top.min(m.rect.top);
        r.right = r.right.max(m.rect.right);
        r.bottom = r.bottom.max(m.rect.bottom);
    }
    r
}

pub fn virtual_rect() -> Rect {
    virtual_rect_in(&list())
}

pub fn count() -> i32 {
    list().len() as i32
}

/// MonitorFromPoint reçoit un POINT par valeur : sur x64 les deux i32 sont
/// regroupés dans un seul registre.
pub extern "win64" fn monitor_from_point_packed(pt: u64, flags: u32) -> usize {
    from_point_in(&list(), pt as u32 as i32, (pt >> 32) as u32 as i32, flags)
}

pub unsafe extern "win64" fn monitor_from_rect(rect: *const Rect, flags: u32) -> usize {
    if rect.is_null() {
        return if flags == MONITOR_DEFAULTTONULL { 0 } else { 1 };
    }
    from_rect_in(&list(), &*rect, flags)
}

pub extern "win64" fn monitor_from_window(hwnd: usize, flags: u32) -> usize {
    match gui::window_rect(hwnd) {
        Some(r) => from_rect_in(&list(), &r, flags),
        None if flags == MONITOR_DEFAULTTONULL => 0,
        None => 1,
    }
}

/// Remplit MONITORINFO (40 octets) ou MONITORINFOEX (A : 72, W : 104).
pub unsafe fn fill_info(monitor: usize, info: *mut u8, wide: bool) -> i32 {
    if info.is_null() {
        return 0;
    }
    let size = *(info as *const u32);
    if size < 40 {
        return 0;
    }
    let list = list();
    let Some(m) = monitor.checked_sub(1).and_then(|i| list.get(i)) else {
        return 0;
    };
    std::ptr::write_unaligned(info.add(4) as *mut Rect, m.rect);
    std::ptr::write_unaligned(info.add(20) as *mut Rect, m.rect);
    *(info.add(36) as *mut u32) = if m.primary { MONITORINFOF_PRIMARY } else { 0 };
    // szDevice : « \\.\DISPLAY1 », comme Windows.
    let name = format!("\\\\.\\DISPLAY{monitor}");
    if wide && size >= 104 {
        let dst = info.add(40) as *mut u16;
        for (i, c) in name.encode_utf16().chain(std::iter::once(0)).enumerate() {
            std::ptr::write_unaligned(dst.add(i), c);
        }
    } else if !wide && size >= 72 {
        let dst = info.add(40);
        for (i, b) in name.bytes().chain(std::iter::once(0)).enumerate() {
            *dst.add(i) = b;
        }
    }
    1
}

pub unsafe extern "win64" fn get_monitor_info_a(monitor: usize, info: *mut u8) -> i32 {
    fill_info(monitor, info, false)
}

pub unsafe extern "win64" fn get_monitor_info_w(monitor: usize, info: *mut u8) -> i32 {
    fill_info(monitor, info, true)
}

/// EnumDisplayMonitors : rappelle `callback(hmonitor, hdc, &rect, data)`
/// pour chaque écran qui coupe `clip` (coordonnées écran si `hdc` est nul).
pub unsafe extern "win64" fn enum_display_monitors(
    hdc: usize,
    clip: *const Rect,
    callback: usize,
    data: isize,
) -> i32 {
    if callback == 0 {
        return 0;
    }
    type Proc = unsafe extern "win64" fn(usize, usize, *mut Rect, isize) -> i32;
    let cb: Proc = std::mem::transmute(callback);
    for (i, m) in list().iter().enumerate() {
        let mut r = m.rect;
        if !clip.is_null() {
            let c = &*clip;
            if overlap(&r, c) == 0 {
                continue;
            }
            r = Rect {
                left: r.left.max(c.left),
                top: r.top.max(c.top),
                right: r.right.min(c.right),
                bottom: r.bottom.min(c.bottom),
            };
        }
        if cb(i + 1, hdc, &mut r, data) == 0 {
            break;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(l: i32, t: i32, r: i32, b: i32, primary: bool) -> gui::Monitor {
        gui::Monitor {
            rect: Rect {
                left: l,
                top: t,
                right: r,
                bottom: b,
            },
            primary,
        }
    }

    #[test]
    fn point_rect_and_virtual_screen() {
        // Portable principal à gauche, écran 2560x1440 à droite.
        let list = [mon(0, 0, 1920, 1080, true), mon(1920, 0, 4480, 1440, false)];
        assert_eq!(from_point_in(&list, 100, 100, 0), 1);
        assert_eq!(from_point_in(&list, 3000, 1200, 0), 2);
        // Zone morte sous le portable.
        assert_eq!(from_point_in(&list, 100, 1300, MONITOR_DEFAULTTONULL), 0);
        assert_eq!(from_point_in(&list, 100, 1300, 1), 1, "principal");
        assert_eq!(
            from_point_in(&list, 1900, 1300, MONITOR_DEFAULTTONEAREST),
            2
        );
        let window = Rect {
            left: 1800,
            top: 100,
            right: 2600,
            bottom: 600,
        };
        assert_eq!(from_rect_in(&list, &window, 0), 2, "majorité sur l'écran 2");
        let v = virtual_rect_in(&list);
        assert_eq!((v.left, v.top, v.right, v.bottom), (0, 0, 4480, 1440));
    }

    #[test]
    fn monitor_info_ex_names_the_display() {
        let mut buf = [0u8; 104];
        buf[..4].copy_from_slice(&104u32.to_ne_bytes());
        unsafe {
            assert_eq!(fill_info(1, buf.as_mut_ptr(), true), 1);
            assert_eq!(fill_info(9, buf.as_mut_ptr(), true), 0);
        }
        assert_eq!(u32::from_ne_bytes(buf[36..40].try_into().unwrap()), 1);
        let name: Vec<u16> = buf[40..64]
            .chunks(2)
            .map(|c| u16::from_ne_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        assert_eq!(String::from_utf16_lossy(&name), "\\\\.\\DISPLAY1");
    }
}
