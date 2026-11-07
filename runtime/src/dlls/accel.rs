//! Tables d'accélérateurs (user32) : CreateAcceleratorTable, LoadAccelerators,
//! CopyAcceleratorTable, DestroyAcceleratorTable et TranslateAccelerator.
//!
//! Écrit d'après la documentation publique (structure ACCEL, ressource
//! RT_ACCELERATOR, drapeaux FVIRTKEY/FSHIFT/FCONTROL/FALT). Une touche qui
//! correspond envoie WM_COMMAND (HIWORD = 1) à la fenêtre donnée : Ctrl+S
//! enregistre dans Notepad++ comme sous Windows.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const FVIRTKEY: u8 = 0x01;
const FSHIFT: u8 = 0x04;
const FCONTROL: u8 = 0x08;
const FALT: u8 = 0x10;

const WM_KEYDOWN: u32 = 0x0100;
const WM_CHAR: u32 = 0x0102;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSCHAR: u32 = 0x0106;
const WM_COMMAND: u32 = 0x0111;

const HANDLE_BASE: usize = 0x00AC_0000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Accel {
    pub virt: u8,
    pub key: u16,
    pub cmd: u16,
}

fn tables() -> &'static Mutex<HashMap<usize, Vec<Accel>>> {
    static T: OnceLock<Mutex<HashMap<usize, Vec<Accel>>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

fn new_table(entries: Vec<Accel>) -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
    let h = HANDLE_BASE + NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) * 4;
    super::lock(tables()).insert(h, entries);
    h
}

fn table(h: usize) -> Option<Vec<Accel>> {
    super::lock(tables()).get(&h).cloned()
}

/// ACCEL : fVirt (BYTE), key (WORD), cmd (WORD), soit 6 octets.
pub unsafe extern "win64" fn create_accelerator_table_w(accel: *const u8, count: i32) -> usize {
    if accel.is_null() || count <= 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let entries = (0..count as usize)
        .map(|i| {
            let p = accel.add(i * 6);
            Accel {
                virt: *p,
                key: std::ptr::read_unaligned(p.add(2) as *const u16),
                cmd: std::ptr::read_unaligned(p.add(4) as *const u16),
            }
        })
        .collect();
    new_table(entries)
}

pub unsafe extern "win64" fn create_accelerator_table_a(accel: *const u8, count: i32) -> usize {
    create_accelerator_table_w(accel, count)
}

/// Ressource RT_ACCELERATOR : entrées de 8 octets (fFlags, wAscii, wId,
/// remplissage) ; 0x80 dans fFlags marque la dernière.
unsafe fn load(hinst: usize, name: usize) -> usize {
    let hrsrc = super::resource::find_resource_w(hinst, name as *const u16, 9 as *const u16);
    if hrsrc == 0 {
        return 0;
    }
    let data = super::resource::load_resource(hinst, hrsrc) as *const u8;
    let size = super::resource::sizeof_resource(hinst, hrsrc) as usize;
    if data.is_null() {
        return 0;
    }
    let mut entries = Vec::new();
    for i in 0..size / 8 {
        let p = data.add(i * 8);
        let flags = std::ptr::read_unaligned(p as *const u16);
        entries.push(Accel {
            virt: (flags & 0x7F) as u8,
            key: std::ptr::read_unaligned(p.add(2) as *const u16),
            cmd: std::ptr::read_unaligned(p.add(4) as *const u16),
        });
        if flags & 0x80 != 0 {
            break;
        }
    }
    new_table(entries)
}

pub unsafe extern "win64" fn load_accelerators_w(hinst: usize, name: *const u16) -> usize {
    load(hinst, name as usize)
}

pub unsafe extern "win64" fn load_accelerators_a(hinst: usize, name: *const u8) -> usize {
    if (name as usize) >> 16 == 0 {
        return load(hinst, name as usize);
    }
    let w: Vec<u16> = super::cstr(name).encode_utf16().chain([0]).collect();
    load(hinst, w.as_ptr() as usize)
}

pub extern "win64" fn destroy_accelerator_table(h: usize) -> i32 {
    super::lock(tables()).remove(&h).is_some() as i32
}

/// CopyAcceleratorTable : nombre d'entrées si `dst` est nul, sinon copie.
pub unsafe extern "win64" fn copy_accelerator_table(h: usize, dst: *mut u8, count: i32) -> i32 {
    let Some(t) = table(h) else { return 0 };
    if dst.is_null() {
        return t.len() as i32;
    }
    let n = t.len().min(count.max(0) as usize);
    for (i, a) in t.iter().take(n).enumerate() {
        let p = dst.add(i * 6);
        *p = a.virt;
        *p.add(1) = 0;
        std::ptr::write_unaligned(p.add(2) as *mut u16, a.key);
        std::ptr::write_unaligned(p.add(4) as *mut u16, a.cmd);
    }
    n as i32
}

fn down(vk: usize) -> bool {
    super::gui::get_key_state(vk) < 0
}

/// Commande de la table qui répond à ce message clavier, s'il y en a une.
pub(crate) fn match_accel(
    t: &[Accel],
    message: u32,
    wparam: usize,
    mods: (bool, bool, bool),
) -> Option<u16> {
    let (shift, ctrl, alt) = mods;
    let virt_msg = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
    let char_msg = matches!(message, WM_CHAR | WM_SYSCHAR);
    t.iter()
        .find(|a| {
            if a.virt & FVIRTKEY != 0 {
                virt_msg
                    && a.key as usize == wparam & 0xFFFF
                    && (a.virt & FSHIFT != 0) == shift
                    && (a.virt & FCONTROL != 0) == ctrl
                    && (a.virt & FALT != 0) == alt
            } else {
                char_msg && a.key as usize == wparam & 0xFFFF && (a.virt & FALT != 0) == alt
            }
        })
        .map(|a| a.cmd)
}

pub unsafe extern "win64" fn translate_accelerator_w(
    hwnd: usize,
    h: usize,
    msg: *const super::gui::Msg,
) -> i32 {
    let Some(m) = msg.as_ref() else { return 0 };
    if hwnd == 0 {
        return 0;
    }
    let Some(t) = table(h) else { return 0 };
    let mods = (down(0x10), down(0x11), down(0x12));
    let Some(cmd) = match_accel(&t, m.message, m.wparam, mods) else {
        return 0;
    };
    super::user32::send_message_w(hwnd, WM_COMMAND, (1 << 16) | cmd as usize, 0);
    1
}

pub unsafe extern "win64" fn translate_accelerator_a(
    hwnd: usize,
    h: usize,
    msg: *const super::gui::Msg,
) -> i32 {
    translate_accelerator_w(hwnd, h, msg)
}

pub fn user32_overrides() -> crate::dlls::Exports {
    crate::exports! {
        "CreateAcceleratorTableW" => fn create_accelerator_table_w,
        "CreateAcceleratorTableA" => fn create_accelerator_table_a,
        "LoadAcceleratorsW" => fn load_accelerators_w,
        "LoadAcceleratorsA" => fn load_accelerators_a,
        "DestroyAcceleratorTable" => fn destroy_accelerator_table,
        "CopyAcceleratorTableW" => fn copy_accelerator_table,
        "CopyAcceleratorTableA" => fn copy_accelerator_table,
        "TranslateAcceleratorW" => fn translate_accelerator_w,
        "TranslateAcceleratorA" => fn translate_accelerator_a,
        "TranslateAccelerator" => fn translate_accelerator_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_s_et_caractere_alt() {
        // Ctrl+S (FVIRTKEY|FCONTROL, 'S') → 42 ; Suppr → 43 ; « a » → 44.
        let raw: [u8; 18] = [
            FVIRTKEY | FCONTROL,
            0,
            b'S',
            0,
            42,
            0, //
            FVIRTKEY,
            0,
            0x2E,
            0,
            43,
            0, //
            0,
            0,
            b'a',
            0,
            44,
            0,
        ];
        let h = unsafe { create_accelerator_table_w(raw.as_ptr(), 3) };
        assert!(h != 0);
        let t = table(h).unwrap();
        let none = (false, false, false);
        let ctrl = (false, true, false);
        assert_eq!(match_accel(&t, WM_KEYDOWN, b'S' as usize, ctrl), Some(42));
        assert_eq!(match_accel(&t, WM_KEYDOWN, b'S' as usize, none), None);
        assert_eq!(
            match_accel(&t, WM_KEYDOWN, b'S' as usize, (true, true, false)),
            None
        );
        assert_eq!(match_accel(&t, WM_KEYDOWN, 0x2E, none), Some(43));
        assert_eq!(match_accel(&t, WM_CHAR, b'a' as usize, none), Some(44));
        assert_eq!(match_accel(&t, WM_KEYDOWN, b'A' as usize, none), None);
        assert_eq!(
            unsafe { copy_accelerator_table(h, std::ptr::null_mut(), 0) },
            3
        );
        let mut back = [0u8; 18];
        assert_eq!(
            unsafe { copy_accelerator_table(h, back.as_mut_ptr(), 3) },
            3
        );
        assert_eq!(back, raw);
        assert_eq!(destroy_accelerator_table(h), 1);
        assert_eq!(destroy_accelerator_table(h), 0);
    }
}
