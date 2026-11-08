//! Fenêtres des programmes x86 : user32 et gdi32 (#232, étape 5).
//!
//! Les passerelles ordinaires de `mod.rs` suffisent aux API qui ne prennent
//! que des entiers et des HANDLE (les HWND, HDC et objets GDI de Winlux sont
//! de petits nombres). Restent ici les structures qui changent de taille en
//! 64 bits parce qu'elles portent un pointeur, un HANDLE ou un WPARAM :
//! WNDCLASS, MSG, PAINTSTRUCT, BITMAP, LOGBRUSH, TRACKMOUSEEVENT…, et les
//! procédures de fenêtre.
//!
//! - **WNDPROC x86.** Chaque procédure x86 reçoit une passerelle 64 → 32
//!   (`wndproc64`) : c'est elle que retient la fenêtre, et le runtime
//!   l'appelle comme n'importe quelle procédure win64.
//! - **WNDPROC 64 bits vue du x86.** GetWindowLong(GWL_WNDPROC) d'une
//!   fenêtre du runtime (contrôle intégré, DefWindowProc) rend une
//!   passerelle x86 (`wndproc32`), que le programme peut rappeler par
//!   CallWindowProc ou appeler directement.
//! - **Messages à structures.** Tout le runtime parle la disposition
//!   64 bits. Un message qui part d'un programme x86 (SendMessage,
//!   DefWindowProc, CallWindowProc vers du 64 bits) est converti en 64 bits
//!   avant l'appel puis recopié au retour ; un message qui arrive à une
//!   procédure x86 l'est en sens inverse. Messages traités ici :
//!   WM_NCCREATE/WM_CREATE (CREATESTRUCT), WM_WINDOWPOSCHANGING/CHANGED
//!   (WINDOWPOS), WM_NCCALCSIZE (NCCALCSIZE_PARAMS), WM_COPYDATA.
//!   Les contrôles communs (WM_NOTIFY, LVITEM…) viendront de `comctl`.

use super::crt::{Fix, Override};
use super::layout::{Layout, F};
use super::{call32, rd, wr, Desc};
use std::collections::HashMap;
use std::sync::Mutex;

const WM_CREATE: u32 = 0x0001;
const WM_NCCREATE: u32 = 0x0081;
const WM_WINDOWPOSCHANGING: u32 = 0x0046;
const WM_WINDOWPOSCHANGED: u32 = 0x0047;
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_COPYDATA: u32 = 0x004A;

const GWL_WNDPROC: i32 = -4;
const GCL_WNDPROC: i32 = -24;
/// Index des données d'une boîte de dialogue : DWL_DLGPROC et DWL_USER
/// valent 4 et 8 en x86, DWLP_DLGPROC et DWLP_USER 8 et 16 en x64.
const DWL_DLGPROC: i32 = 4;
const DWL_USER: i32 = 8;

/// Argument n° `i` de la pile x86.
fn arg(args: u32, i: u32) -> u32 {
    rd(args + i * 4)
}

/// Valeur x86 étendue à 64 bits : signe gardé pour -1, -2… (HWND_TOPMOST).
fn ext(v: u32) -> usize {
    if v >= 0xffff_0000 {
        v as i32 as isize as usize
    } else {
        v as usize
    }
}

fn user(name: &str) -> Option<usize> {
    crate::dlls::lookup("user32", name)
}

/// Appel win64 d'une fonction du runtime avec ces cases.
fn call64(target: usize, slots: &[u64]) -> u64 {
    let mut s = slots.to_vec();
    let n = s.len();
    while s.len() < 4 {
        s.push(0);
    }
    let mut xmm0 = 0u64;
    unsafe { super::winlux_wow_call64(target, s.as_ptr(), n, &mut xmm0) }
}

fn raw(f: extern "win64" fn(u32, usize) -> u64, aux: usize, nargs: u16) -> Override {
    Override::Raw {
        target: f as *const () as usize,
        aux,
        pop: Some(nargs * 4),
    }
}

/// Fonction 64 bits, nombre d'arguments x86 et rang d'un argument
/// particulier (MSG, procédure…).
struct Spec {
    target: usize,
    nargs: u32,
    at: u32,
}

fn spec(f: extern "win64" fn(u32, usize) -> u64, target: usize, nargs: u32, at: u32) -> Override {
    let s = Box::leak(Box::new(Spec { target, nargs, at }));
    raw(f, s as *const Spec as usize, nargs as u16)
}

/// Remplacements x86 de user32 et gdi32 (`n` : nom normalisé de la DLL).
pub(super) fn override_for(n: &str, func: &str) -> Option<Override> {
    if n == "gdi32" {
        let t = crate::dlls::lookup(n, func);
        return Some(match func {
            "GetObjectA" | "GetObjectW" => raw(get_object, t?, 3),
            "CreateDIBSection" => raw(create_dib_section, t?, 6),
            "CreateBrushIndirect" => raw(create_brush_indirect, t?, 1),
            _ => return None,
        });
    }
    if n != "user32" {
        return None;
    }
    let t = user(func);
    let wide = func.ends_with('W') as usize;
    Some(match func {
        "RegisterClassA" | "RegisterClassW" => raw(register_class, t?, 1),
        "RegisterClassExA" | "RegisterClassExW" => raw(register_class_ex, t?, 1),
        "GetClassInfoA" | "GetClassInfoW" | "GetClassInfoExA" | "GetClassInfoExW" => {
            let ex = func.contains("Ex");
            let target = user(if wide == 1 {
                "GetClassInfoExW"
            } else {
                "GetClassInfoExA"
            })?;
            spec(get_class_info, target, 3, ex as u32)
        }
        "GetMessageA" | "GetMessageW" => spec(get_message, t?, 4, 0),
        "PeekMessageA" | "PeekMessageW" => spec(get_message, t?, 5, 0),
        "TranslateMessage" | "DispatchMessageA" | "DispatchMessageW" => spec(msg_in, t?, 1, 0),
        "IsDialogMessageA" | "IsDialogMessageW" | "IsDialogMessage" | "TranslateMDISysAccel" => {
            spec(msg_in, t?, 2, 1)
        }
        "TranslateAcceleratorA" | "TranslateAcceleratorW" | "TranslateAccelerator" => {
            spec(msg_in, t?, 3, 2)
        }
        // MENUITEMINFO : 48 octets en x86, 80 en x64 (#752, 7zFM).
        "GetMenuItemInfoA" | "GetMenuItemInfoW" => spec(menu_item_info, t?, 4, 1),
        "SetMenuItemInfoA" | "SetMenuItemInfoW" | "InsertMenuItemA" | "InsertMenuItemW" => {
            spec(menu_item_info, t?, 4, 0)
        }
        // ICONINFO : 20 octets en x86 (deux HBITMAP de 4), 32 en x64.
        "GetIconInfo" => raw(get_icon_info, t?, 2),
        "CreateIconIndirect" => raw(create_icon_indirect, t?, 1),
        "BeginPaint" => raw(begin_paint, t?, 2),
        "EndPaint" => raw(end_paint, t?, 2),
        "GetWindowLongA" | "GetWindowLongW" => raw(get_window_long, t?, 2),
        "SetWindowLongA" | "SetWindowLongW" => raw(set_window_long, t?, 3),
        "GetClassLongA" | "GetClassLongW" => raw(get_class_long, t?, 2),
        "SetClassLongA" | "SetClassLongW" => raw(set_class_long, t?, 3),
        "CallWindowProcA" | "CallWindowProcW" => raw(call_window_proc, 0, 5),
        // Messages qui peuvent porter une structure : conversion 32 → 64.
        "SendMessageA" | "SendMessageW" | "SendNotifyMessageA" | "SendNotifyMessageW" => {
            spec(send_message, t?, 4, TO_WINDOW)
        }
        "DefWindowProcA" | "DefWindowProcW" | "DefDlgProcA" | "DefDlgProcW" | "DefFrameProcA"
        | "DefFrameProcW" | "DefMDIChildProcA" | "DefMDIChildProcW" => {
            let (nargs, at) = if func.starts_with("DefFrameProc") {
                (5, 1)
            } else {
                (4, 0)
            };
            spec(send_message, t?, nargs, at)
        }
        "SendDlgItemMessageA" | "SendDlgItemMessageW" => raw(send_dlg_item_message, t?, 5),
        "SendMessageTimeoutA" | "SendMessageTimeoutW" => raw(send_message_timeout, t?, 7),
        // Procédures de boîtes de dialogue : même passerelle que les WNDPROC.
        "DialogBoxParamA"
        | "DialogBoxParamW"
        | "CreateDialogParamA"
        | "CreateDialogParamW"
        | "DialogBoxIndirectParamA"
        | "DialogBoxIndirectParamW"
        | "CreateDialogIndirectParamA"
        | "CreateDialogIndirectParamW" => spec(with_proc, t?, 5, 3),
        "TrackMouseEvent" | "_TrackMouseEvent" => raw(track_mouse_event, t?, 1),
        // Une procédure de crochet x86 ne peut pas être appelée en win64 :
        // handle factice, sans enregistrement, comme avant #938.
        "SetWindowsHookExA" | "SetWindowsHookExW" => raw(inert_hook, 0, 4),
        _ => return None,
    })
}

// ---------------------------------------------------------------------
// Procédures de fenêtre
// ---------------------------------------------------------------------

// Entrée des passerelles WNDPROC 64 → 32 : r10d = procédure x86, passée en
// cinquième argument de `wndproc_dispatch`.
std::arch::global_asm!(
    r#"
    .text
    .globl winlux_wow_wndproc64
winlux_wow_wndproc64:
    push rbp
    mov rbp, rsp
    sub rsp, 48
    mov qword ptr [rsp + 32], r10
    call {dispatch}
    leave
    ret
"#,
    dispatch = sym wndproc_dispatch,
);

extern "C" {
    fn winlux_wow_wndproc64();
}

#[derive(Default)]
struct Procs {
    /// Procédure x86 → passerelle 64 bits, et l'inverse.
    to64: HashMap<u32, usize>,
    from64: HashMap<usize, u32>,
    /// Procédure 64 bits → passerelle x86, et l'inverse.
    to32: HashMap<usize, u32>,
    from32: HashMap<u32, usize>,
}

static PROCS: Mutex<Option<Procs>> = Mutex::new(None);

fn with_procs<T>(f: impl FnOnce(&mut Procs) -> T) -> T {
    let mut g = PROCS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Procs::default))
}

/// Procédure appelable en win64 pour la procédure x86 `proc32`.
pub(super) fn wndproc64(proc32: u32) -> usize {
    if proc32 == 0 {
        return 0;
    }
    with_procs(|p| {
        if let Some(&native) = p.from32.get(&proc32) {
            return native;
        }
        if let Some(&s) = p.to64.get(&proc32) {
            return s;
        }
        let mut code = Vec::with_capacity(20);
        code.extend_from_slice(&[0x41, 0xba]); // mov r10d, imm32
        code.extend_from_slice(&proc32.to_le_bytes());
        code.extend_from_slice(&[0x48, 0xb8]); // mov rax, imm64
        code.extend_from_slice(&(winlux_wow_wndproc64 as *const () as usize as u64).to_le_bytes());
        code.extend_from_slice(&[0xff, 0xe0]); // jmp rax
        let at = super::alloc_code(&code) as usize;
        p.to64.insert(proc32, at);
        p.from64.insert(at, proc32);
        at
    })
}

/// Procédure appelable par le code x86 pour la procédure 64 bits `native`.
pub(super) fn wndproc32(native: usize) -> u32 {
    if native == 0 {
        return 0;
    }
    if let Some(x86) = with_procs(|p| p.from64.get(&native).copied()) {
        return x86;
    }
    if let Some(s) = with_procs(|p| p.to32.get(&native).copied()) {
        return s;
    }
    let stub = super::make_stub(
        Desc {
            ret: b'i',
            supported: true,
            target: native_from32 as *const () as usize,
            kinds: Vec::new(),
            label: format!("WNDPROC 0x{native:x}"),
            fix: Fix::None,
            raw: Some(native),
            callbacks: 0,
        },
        Some(16),
    ) as u32;
    with_procs(|p| {
        p.to32.insert(native, stub);
        p.from32.insert(stub, native);
    });
    stub
}

/// La procédure 64 bits derrière une passerelle x86 de `wndproc32`.
fn native_of(proc32: u32) -> Option<usize> {
    with_procs(|p| p.from32.get(&proc32).copied())
}

/// Procédure 64 bits appelée depuis le code x86 (passerelle `wndproc32`).
extern "win64" fn native_from32(args: u32, native: usize) -> u64 {
    let hwnd = ext(arg(args, 0));
    let msg = arg(args, 1);
    send_conv(
        hwnd,
        msg,
        arg(args, 2),
        arg(args, 3),
        &mut |wp, lp| unsafe { crate::dlls::gui::call_wndproc(native, hwnd, msg, wp, lp) },
    ) as u64
}

/// Procédure x86 appelée par le runtime (passerelle `wndproc64`).
extern "win64" fn wndproc_dispatch(
    hwnd: usize,
    msg: u32,
    wparam: usize,
    lparam: isize,
    proc32: usize,
) -> isize {
    // Thread du runtime sans TEB 32 bits : il en reçoit un.
    if super::teb32_addr() == 0 {
        crate::process::init_thread();
    }
    let proc32 = proc32 as u32;
    recv_conv(hwnd, msg, wparam, lparam, &mut |wp, lp| {
        call32(proc32 as usize, &[hwnd as u32, msg, wp, lp]) as u32
    }) as i32 as isize
}

// ---------------------------------------------------------------------
// Messages à structures
// ---------------------------------------------------------------------

/// Champ de 32 bits à `off` d'une structure 64 bits.
fn r64(p: usize, off: usize) -> u32 {
    unsafe { ((p + off) as *const u32).read_unaligned() }
}

fn q64(p: usize, off: usize) -> u64 {
    unsafe { ((p + off) as *const u64).read_unaligned() }
}

fn w64(p: usize, off: usize, v: u32) {
    unsafe { ((p + off) as *mut u32).write_unaligned(v) }
}

fn wq64(p: usize, off: usize, v: u64) {
    unsafe { ((p + off) as *mut u64).write_unaligned(v) }
}

/// Pointeur 64 bits qui doit être visible du code x86.
fn low(v: u64, what: &str) -> u32 {
    if v > u32::MAX as u64 {
        crate::trace!("wow32 : {what} 0x{v:x} au-dessus de 4 Go, invisible du programme x86");
    }
    v as u32
}

/// lParam 64 bits pour le x86 : valeur signée (-1…) ou pointeur sous 4 Go.
fn lparam32(lp: i64, what: &str) -> u32 {
    if lp < i32::MIN as i64 || lp > u32::MAX as i64 {
        crate::trace!("wow32 : {what} 0x{lp:x} au-dessus de 4 Go, invisible du programme x86");
    }
    lp as u32
}

// CREATESTRUCT : 12 champs de 4 octets en x86 ; en x64, pointeurs et
// HANDLE de 8 octets.
const CS64: [(usize, bool); 12] = [
    (0, true),
    (8, true),
    (16, true),
    (24, true),
    (32, false),
    (36, false),
    (40, false),
    (44, false),
    (48, false),
    (56, true),
    (64, true),
    (72, false),
];

fn create_to32(p64: usize, out: &mut [u32; 12]) {
    for (i, &(off, wide)) in CS64.iter().enumerate() {
        out[i] = if wide {
            low(q64(p64, off), "CREATESTRUCT")
        } else {
            r64(p64, off)
        };
    }
}

fn create_to64(p32: u32, out: &mut [u64; 10]) {
    let p = out.as_mut_ptr() as usize;
    for (i, &(off, wide)) in CS64.iter().enumerate() {
        let v = rd(p32 + i as u32 * 4);
        if wide {
            wq64(p, off, ext(v) as u64);
        } else {
            w64(p, off, v);
        }
    }
}

// WINDOWPOS : hwnd, hwndInsertAfter, x, y, cx, cy, flags.
fn pos_to32(p64: usize, p32: usize) {
    w64(p32, 0, low(q64(p64, 0), "WINDOWPOS"));
    w64(p32, 4, q64(p64, 8) as u32);
    for i in 0..5 {
        w64(p32, 8 + i * 4, r64(p64, 16 + i * 4));
    }
}

fn pos_to64(p32: usize, p64: usize) {
    wq64(p64, 0, ext(r64(p32, 0)) as u64);
    wq64(p64, 8, ext(r64(p32, 4)) as u64);
    for i in 0..5 {
        w64(p64, 16 + i * 4, r64(p32, 8 + i * 4));
    }
}

/// Message parti du code x86 vers une procédure 64 bits : structure en
/// disposition 64 bits pendant l'appel, recopiée au retour.
fn send_conv(
    hwnd: usize,
    msg: u32,
    wp: u32,
    lp: u32,
    call: &mut dyn FnMut(usize, isize) -> isize,
) -> u32 {
    send_conv_with(hwnd, msg, wp, lp, true, call)
}

/// `comctl` faux : la structure d'un contrôle commun reste en x86 (la
/// procédure qui la reçoit est du code x86).
fn send_conv_with(
    hwnd: usize,
    msg: u32,
    wp: u32,
    lp: u32,
    comctl: bool,
    call: &mut dyn FnMut(usize, isize) -> isize,
) -> u32 {
    // Contrôles communs (LVITEM, TVINSERTSTRUCT, TBBUTTON…) : comctl.rs.
    let converted = if comctl {
        super::comctl::convert_message(hwnd, msg, wp, lp)
    } else {
        None
    };
    if let Some(c) = converted {
        let r = call(c.wparam, c.lparam);
        return match c.done {
            Some(done) => done(r),
            None => r,
        } as u32;
    }
    let wp64 = ext(wp);
    if lp == 0 {
        return call(wp64, 0) as u32;
    }
    match msg {
        WM_NCCREATE | WM_CREATE => {
            let mut cs = [0u64; 10];
            create_to64(lp, &mut cs);
            call(wp64, cs.as_ptr() as isize) as u32
        }
        WM_WINDOWPOSCHANGING | WM_WINDOWPOSCHANGED => {
            let mut pos = [0u64; 5];
            let p = pos.as_mut_ptr() as usize;
            pos_to64(lp as usize, p);
            let r = call(wp64, p as isize);
            pos_to32(p, lp as usize);
            r as u32
        }
        WM_NCCALCSIZE if wp != 0 => {
            // NCCALCSIZE_PARAMS : trois RECT puis un pointeur de WINDOWPOS.
            let mut params = [0u64; 7];
            let mut pos = [0u64; 5];
            let p = params.as_mut_ptr() as usize;
            let pp = pos.as_mut_ptr() as usize;
            unsafe { std::ptr::copy_nonoverlapping(lp as usize as *const u8, p as *mut u8, 48) };
            let lppos = rd(lp + 48);
            if lppos != 0 {
                pos_to64(lppos as usize, pp);
                wq64(p, 48, pp as u64);
            }
            let r = call(wp64, p as isize);
            unsafe { std::ptr::copy_nonoverlapping(p as *const u8, lp as usize as *mut u8, 48) };
            if lppos != 0 {
                pos_to32(pp, lppos as usize);
            }
            r as u32
        }
        WM_COPYDATA => {
            // COPYDATASTRUCT : dwData (ULONG_PTR), cbData, lpData.
            let mut cds = [0u64; 3];
            let p = cds.as_mut_ptr() as usize;
            wq64(p, 0, rd(lp) as u64);
            w64(p, 8, rd(lp + 4));
            wq64(p, 16, rd(lp + 8) as u64);
            call(wp64, p as isize) as u32
        }
        _ => call(wp64, ext(lp) as isize) as u32,
    }
}

/// Message du runtime vers une procédure x86 : structure en disposition
/// x86 pendant l'appel, recopiée au retour.
fn recv_conv(
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
    call: &mut dyn FnMut(u32, u32) -> u32,
) -> u32 {
    // Notifications des contrôles communs (WM_NOTIFY, WM_DRAWITEM…) : comctl.rs.
    if let Some(c) = super::comctl::convert_to_x86(hwnd, msg, wp, lp) {
        let r = call(c.wparam as u32, c.lparam as u32) as i32 as isize;
        return match c.done {
            Some(done) => done(r),
            None => r,
        } as u32;
    }
    let wp32 = wp as u32;
    let p64 = lp as usize;
    if lp == 0 {
        return call(wp32, 0);
    }
    match msg {
        WM_NCCREATE | WM_CREATE => {
            let mut cs = [0u32; 12];
            create_to32(p64, &mut cs);
            call(wp32, cs.as_ptr() as usize as u32)
        }
        WM_WINDOWPOSCHANGING | WM_WINDOWPOSCHANGED => {
            let mut pos = [0u32; 7];
            let p = pos.as_mut_ptr() as usize;
            pos_to32(p64, p);
            let r = call(wp32, p as u32);
            pos_to64(p, p64);
            r
        }
        WM_NCCALCSIZE if wp != 0 => {
            let mut params = [0u32; 13];
            let mut pos = [0u32; 7];
            let p = params.as_mut_ptr() as usize;
            let pp = pos.as_mut_ptr() as usize;
            unsafe { std::ptr::copy_nonoverlapping(p64 as *const u8, p as *mut u8, 48) };
            let lppos = q64(p64, 48) as usize;
            if lppos != 0 {
                pos_to32(lppos, pp);
                w64(p, 48, pp as u32);
            }
            let r = call(wp32, p as u32);
            unsafe { std::ptr::copy_nonoverlapping(p as *const u8, p64 as *mut u8, 48) };
            if lppos != 0 {
                pos_to64(pp, lppos);
            }
            r
        }
        WM_COPYDATA => {
            let cds = [
                q64(p64, 0) as u32,
                r64(p64, 8),
                low(q64(p64, 16), "COPYDATASTRUCT"),
            ];
            call(wp32, cds.as_ptr() as usize as u32)
        }
        _ => call(wp32, lparam32(lp as i64, "lParam")),
    }
}

// ---------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------

/// WNDCLASS x86 (40 octets) → WNDCLASS x64 (72 octets).
fn class_to64(c: u32, out: usize) {
    w64(out, 0, rd(c)); // style
    wq64(out, 8, wndproc64(rd(c + 4)) as u64);
    w64(out, 16, rd(c + 8)); // cbClsExtra
    w64(out, 20, rd(c + 12)); // cbWndExtra
    for i in 0..6 {
        // hInstance, hIcon, hCursor, hbrBackground, lpszMenuName, lpszClassName
        wq64(out, 24 + i * 8, ext(rd(c + 16 + i as u32 * 4)) as u64);
    }
}

extern "win64" fn register_class(args: u32, target: usize) -> u64 {
    let wc = arg(args, 0);
    if wc == 0 {
        return call64(target, &[0]);
    }
    let mut c = [0u64; 9];
    class_to64(wc, c.as_mut_ptr() as usize);
    call64(target, &[c.as_ptr() as u64])
}

extern "win64" fn register_class_ex(args: u32, target: usize) -> u64 {
    let wc = arg(args, 0);
    if wc == 0 {
        return call64(target, &[0]);
    }
    // WNDCLASSEX = cbSize, puis WNDCLASS (style à 4, le reste à 8 en x64),
    // puis hIconSm.
    let mut fixed = [0u64; 10];
    let f = fixed.as_mut_ptr() as usize;
    w64(f, 0, 80);
    w64(f, 4, rd(wc + 4));
    wq64(f, 8, wndproc64(rd(wc + 8)) as u64);
    w64(f, 16, rd(wc + 12));
    w64(f, 20, rd(wc + 16));
    for i in 0..7u32 {
        // hInstance … lpszClassName, hIconSm
        wq64(f, 24 + i as usize * 8, ext(rd(wc + 20 + i * 4)) as u64);
    }
    call64(target, &[f as u64])
}

/// GetClassInfo(Ex)A/W ; `s.at` : 1 pour la forme Ex.
extern "win64" fn get_class_info(args: u32, s: usize) -> u64 {
    let s = unsafe { &*(s as *const Spec) };
    let out = arg(args, 2);
    if out == 0 {
        return 0;
    }
    let mut c = [0u64; 10];
    let p = c.as_mut_ptr() as usize;
    w64(p, 0, 80);
    let r = call64(
        s.target,
        &[ext(arg(args, 0)) as u64, ext(arg(args, 1)) as u64, p as u64],
    );
    if r as u32 != 0 {
        if s.at == 1 {
            wr(out, 48);
            wr(out + 4, r64(p, 4));
            wr(out + 8, wndproc32(q64(p, 8) as usize));
            wr(out + 12, r64(p, 16));
            wr(out + 16, r64(p, 20));
            for i in 0..7u32 {
                wr(out + 20 + i * 4, q64(p, 24 + i as usize * 8) as u32);
            }
        } else {
            wr(out, r64(p, 4));
            wr(out + 4, wndproc32(q64(p, 8) as usize));
            wr(out + 8, r64(p, 16));
            wr(out + 12, r64(p, 20));
            for i in 0..6u32 {
                wr(out + 16 + i * 4, q64(p, 24 + i as usize * 8) as u32);
            }
        }
    }
    r
}

// ---------------------------------------------------------------------
// Boucle de messages
// ---------------------------------------------------------------------

// MSG : x86 hwnd, message, wParam, lParam, time, pt (28 octets) ;
// x64 hwnd 0, message 8, wParam 16, lParam 24, time 32, pt 36 (48 octets).
fn msg_to64(m: u32, out: usize) {
    wq64(out, 0, ext(rd(m)) as u64);
    w64(out, 8, rd(m + 4));
    wq64(out, 16, rd(m + 8) as u64);
    wq64(out, 24, rd(m + 12) as i32 as i64 as u64);
    w64(out, 32, rd(m + 16));
    w64(out, 36, rd(m + 20));
    w64(out, 40, rd(m + 24));
}

fn msg_to32(m: usize, out: u32) {
    wr(out, q64(m, 0) as u32);
    wr(out + 4, r64(m, 8));
    wr(out + 8, q64(m, 16) as u32);
    wr(out + 12, lparam32(q64(m, 24) as i64, "lParam de MSG"));
    wr(out + 16, r64(m, 32));
    wr(out + 20, r64(m, 36));
    wr(out + 24, r64(m, 40));
}

/// GetMessage et PeekMessage : MSG x64 recopié en x86.
extern "win64" fn get_message(args: u32, s: usize) -> u64 {
    let s = unsafe { &*(s as *const Spec) };
    let out = arg(args, 0);
    let mut m = [0u64; 6];
    let p = m.as_mut_ptr() as usize;
    let mut slots: Vec<u64> = (0..s.nargs).map(|i| ext(arg(args, i)) as u64).collect();
    slots[0] = if out == 0 { 0 } else { p as u64 };
    let r = call64(s.target, &slots);
    let peek = s.nargs == 5;
    if out != 0 && ((peek && r as u32 != 0) || (!peek && r as i32 != -1)) {
        msg_to32(p, out);
    }
    r
}

/// Fonctions qui lisent un MSG (rang `s.at`).
extern "win64" fn msg_in(args: u32, s: usize) -> u64 {
    let s = unsafe { &*(s as *const Spec) };
    let mut m = [0u64; 6];
    let p = m.as_mut_ptr() as usize;
    let mut slots: Vec<u64> = (0..s.nargs).map(|i| ext(arg(args, i)) as u64).collect();
    let m32 = arg(args, s.at);
    if m32 != 0 {
        msg_to64(m32, p);
        slots[s.at as usize] = p as u64;
    }
    call64(s.target, &slots)
}

// ---------------------------------------------------------------------
// Peinture
// ---------------------------------------------------------------------

// PAINTSTRUCT : x86 hdc, fErase, rcPaint, fRestore, fIncUpdate, réservé[32]
// (64 octets) ; x64 hdc sur 8 octets, le reste décalé de 4 (72 octets).
extern "win64" fn begin_paint(args: u32, target: usize) -> u64 {
    let out = arg(args, 1);
    let mut ps = [0u64; 9];
    let p = ps.as_mut_ptr() as usize;
    let r = call64(
        target,
        &[
            ext(arg(args, 0)) as u64,
            if out == 0 { 0 } else { p as u64 },
        ],
    );
    if out != 0 && r != 0 {
        wr(out, q64(p, 0) as u32);
        unsafe {
            std::ptr::copy_nonoverlapping((p + 8) as *const u8, (out + 4) as usize as *mut u8, 60)
        };
    }
    r
}

extern "win64" fn end_paint(args: u32, target: usize) -> u64 {
    let ps32 = arg(args, 1);
    let mut ps = [0u64; 9];
    let p = ps.as_mut_ptr() as usize;
    if ps32 != 0 {
        wq64(p, 0, rd(ps32) as u64);
        unsafe {
            std::ptr::copy_nonoverlapping((ps32 + 4) as usize as *const u8, (p + 8) as *mut u8, 60)
        };
    }
    call64(
        target,
        &[
            ext(arg(args, 0)) as u64,
            if ps32 == 0 { 0 } else { p as u64 },
        ],
    )
}

// ---------------------------------------------------------------------
// Données de fenêtre et de classe
// ---------------------------------------------------------------------

fn get_ptr(hwnd: usize, index: i32) -> usize {
    user("GetWindowLongPtrW").map_or(0, |f| {
        call64(f, &[hwnd as u64, index as i64 as u64]) as usize
    })
}

fn set_ptr(hwnd: usize, index: i32, value: usize) -> usize {
    user("SetWindowLongPtrW").map_or(0, |f| {
        call64(f, &[hwnd as u64, index as i64 as u64, value as u64]) as usize
    })
}

fn is_dialog(hwnd: usize) -> bool {
    crate::dlls::dialogs::is_dialog(hwnd)
}

extern "win64" fn get_window_long(args: u32, target: usize) -> u64 {
    let hwnd = ext(arg(args, 0));
    let index = arg(args, 1) as i32;
    let dialog = index > 0 && is_dialog(hwnd);
    match index {
        GWL_WNDPROC => wndproc32(get_ptr(hwnd, GWL_WNDPROC)) as u64,
        DWL_DLGPROC if dialog => wndproc32(get_ptr(hwnd, 8)) as u64,
        DWL_USER if dialog => get_ptr(hwnd, 16) as u32 as u64,
        _ => call64(target, &[hwnd as u64, index as i64 as u64]) as u32 as u64,
    }
}

extern "win64" fn set_window_long(args: u32, target: usize) -> u64 {
    let hwnd = ext(arg(args, 0));
    let index = arg(args, 1) as i32;
    let value = arg(args, 2);
    let dialog = index > 0 && is_dialog(hwnd);
    match index {
        GWL_WNDPROC => wndproc32(set_ptr(hwnd, GWL_WNDPROC, wndproc64(value))) as u64,
        DWL_DLGPROC if dialog => wndproc32(set_ptr(hwnd, 8, wndproc64(value))) as u64,
        DWL_USER if dialog => set_ptr(hwnd, 16, ext(value)) as u32 as u64,
        _ => call64(
            target,
            &[hwnd as u64, index as i64 as u64, value as i32 as i64 as u64],
        ) as u32 as u64,
    }
}

extern "win64" fn get_class_long(args: u32, target: usize) -> u64 {
    let hwnd = ext(arg(args, 0));
    let index = arg(args, 1) as i32;
    if index == GCL_WNDPROC {
        let native = user("GetClassLongPtrW").map_or(0, |f| {
            call64(f, &[hwnd as u64, index as i64 as u64]) as usize
        });
        return wndproc32(native) as u64;
    }
    call64(target, &[hwnd as u64, index as i64 as u64]) as u32 as u64
}

extern "win64" fn set_class_long(args: u32, target: usize) -> u64 {
    let hwnd = ext(arg(args, 0));
    let index = arg(args, 1) as i32;
    let value = arg(args, 2);
    if index == GCL_WNDPROC {
        let f = user("SetClassLongPtrW").unwrap_or(target);
        let old = call64(
            f,
            &[hwnd as u64, index as i64 as u64, wndproc64(value) as u64],
        );
        return wndproc32(old as usize) as u64;
    }
    call64(
        target,
        &[hwnd as u64, index as i64 as u64, value as i32 as i64 as u64],
    ) as u32 as u64
}

// ---------------------------------------------------------------------
// Envoi de messages
// ---------------------------------------------------------------------

/// CallWindowProc : procédure x86 appelée telle quelle (le message est
/// déjà en disposition x86), passerelle d'une procédure 64 bits appelée
/// directement avec conversion.
extern "win64" fn call_window_proc(args: u32, _aux: usize) -> u64 {
    let proc32 = arg(args, 0);
    let (hwnd, msg, wp, lp) = (arg(args, 1), arg(args, 2), arg(args, 3), arg(args, 4));
    if proc32 == 0 {
        return 0;
    }
    match native_of(proc32) {
        Some(native) => send_conv(ext(hwnd), msg, wp, lp, &mut |w, l| unsafe {
            crate::dlls::gui::call_wndproc(native, ext(hwnd), msg, w, l)
        }) as u64,
        None => call32(proc32 as usize, &[hwnd, msg, wp, lp]) as u32 as u64,
    }
}

/// `Spec.at` de SendMessage : le message va à la procédure de la fenêtre,
/// qui peut être du code x86 (sous-classement).
const TO_WINDOW: u32 = 0x100;

/// Fenêtre dont la procédure est du code x86 : un contrôle du runtime
/// sous-classé par le programme (SetWindowLong GWL_WNDPROC). Elle doit
/// recevoir les structures des contrôles communs en disposition x86 ; elle
/// les passe ensuite à la procédure d'origine par CallWindowProc, qui
/// convertit (#762 : 7zFM, liste et barre d'adresse sans texte, les
/// structures étaient converties deux fois).
fn x86_window(hwnd: usize) -> bool {
    let proc_ = get_ptr(hwnd, GWL_WNDPROC);
    proc_ != 0 && with_procs(|p| p.from64.contains_key(&proc_))
}

/// SendMessage, DefWindowProc… : hwnd au rang `s.at`, puis message,
/// wParam, lParam.
extern "win64" fn send_message(args: u32, s: usize) -> u64 {
    let s = unsafe { &*(s as *const Spec) };
    let mut slots: Vec<u64> = (0..s.nargs).map(|i| ext(arg(args, i)) as u64).collect();
    let to_window = s.at & TO_WINDOW != 0;
    let at = s.at & !TO_WINDOW;
    let msg = arg(args, at + 1);
    let hwnd = ext(arg(args, at));
    let comctl = !(to_window && x86_window(hwnd));
    send_conv_with(
        hwnd,
        msg,
        arg(args, at + 2),
        arg(args, at + 3),
        comctl,
        &mut |wp, lp| {
            slots[at as usize + 2] = wp as u64;
            slots[at as usize + 3] = lp as u64;
            call64(s.target, &slots) as isize
        },
    ) as u64
}

extern "win64" fn send_dlg_item_message(args: u32, target: usize) -> u64 {
    let (dlg, id, msg) = (ext(arg(args, 0)), arg(args, 1), arg(args, 2));
    let item = crate::dlls::user32::get_dlg_item(dlg, id as i32);
    let comctl = !x86_window(item);
    send_conv_with(
        item,
        msg,
        arg(args, 3),
        arg(args, 4),
        comctl,
        &mut |wp, lp| {
            call64(
                target,
                &[dlg as u64, id as u64, msg as u64, wp as u64, lp as u64],
            ) as isize
        },
    ) as u64
}

/// SendMessageTimeout : le résultat (DWORD_PTR) est écrit sur 8 octets en x64.
extern "win64" fn send_message_timeout(args: u32, target: usize) -> u64 {
    let (hwnd, msg) = (ext(arg(args, 0)), arg(args, 1));
    let (flags, timeout, out) = (arg(args, 4), arg(args, 5), arg(args, 6));
    let mut result = 0u64;
    let mut ok = 0u64;
    let comctl = !x86_window(hwnd);
    send_conv_with(
        hwnd,
        msg,
        arg(args, 2),
        arg(args, 3),
        comctl,
        &mut |wp, lp| {
            ok = call64(
                target,
                &[
                    hwnd as u64,
                    msg as u64,
                    wp as u64,
                    lp as u64,
                    flags as u64,
                    timeout as u64,
                    &mut result as *mut u64 as u64,
                ],
            );
            result as isize
        },
    );
    if out != 0 {
        wr(out, result as u32);
    }
    ok
}

/// DialogBoxParam, CreateDialogParam… : la procédure x86 au rang `s.at`.
extern "win64" fn with_proc(args: u32, s: usize) -> u64 {
    let s = unsafe { &*(s as *const Spec) };
    let mut slots: Vec<u64> = (0..s.nargs).map(|i| ext(arg(args, i)) as u64).collect();
    slots[s.at as usize] = wndproc64(arg(args, s.at)) as u64;
    call64(s.target, &slots)
}

/// TRACKMOUSEEVENT : cbSize, dwFlags, hwndTrack, dwHoverTime (16 octets en
/// x86, 24 en x64).
extern "win64" fn inert_hook(_args: u32, _aux: usize) -> u64 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x00F8_0000);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u64
}

extern "win64" fn track_mouse_event(args: u32, target: usize) -> u64 {
    let t = arg(args, 0);
    if t == 0 {
        return call64(target, &[0]);
    }
    let mut tme = [0u64; 3];
    let p = tme.as_mut_ptr() as usize;
    w64(p, 0, 24);
    w64(p, 4, rd(t + 4));
    wq64(p, 8, ext(rd(t + 8)) as u64);
    w64(p, 16, rd(t + 12));
    let r = call64(target, &[p as u64]);
    wr(t + 4, r64(p, 4));
    wr(t + 8, q64(p, 8) as u32);
    wr(t + 12, r64(p, 16));
    r
}

// ---------------------------------------------------------------------
// gdi32
// ---------------------------------------------------------------------

const OBJ_BRUSH: u32 = 2;
const OBJ_BITMAP: u32 = 7;

/// GetObject : BITMAP (bmBits) et LOGBRUSH (lbHatch) sont plus courts en
/// x86 ; les autres descriptions (LOGFONT, LOGPEN…) sont identiques.
/// Taille d'un DIBSECTION en x86.
const DIBSECTION32: u64 = 84;

extern "win64" fn get_object(args: u32, target: usize) -> u64 {
    let (obj, size, out) = (ext(arg(args, 0)), arg(args, 1) as i32, arg(args, 2));
    let kind = crate::dlls::lookup("gdi32", "GetObjectType")
        .map_or(0, |f| call64(f, &[obj as u64]) as u32);
    let size32 = match kind {
        OBJ_BITMAP => 24,
        OBJ_BRUSH => 12,
        _ => return call64(target, &[obj as u64, size as i64 as u64, out as u64]),
    };
    if out == 0 {
        return size32;
    }
    if size < size32 as i32 {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut buf = [0u64; 16];
    let p = buf.as_mut_ptr() as usize;
    let r = call64(target, &[obj as u64, 128, p as u64]);
    if r == 0 {
        return 0;
    }
    match kind {
        // DIBSECTION x64 (104 octets) → x86 (84) quand l'appelant a prévu la
        // place : BITMAP (24), BITMAPINFOHEADER (40), dsBitfields (12),
        // dshSection (4), dsOffset (4). La VCL (TBitmap, TPngImageList de
        // WinSCP) recopie dsBmih pour recréer la DIB (#905).
        OBJ_BITMAP if r == 104 && size >= DIBSECTION32 as i32 => {
            unsafe {
                std::ptr::copy_nonoverlapping(p as *const u8, out as usize as *mut u8, 20);
                std::ptr::copy_nonoverlapping(
                    (p + 32) as *const u8,
                    (out + 24) as usize as *mut u8,
                    52,
                );
            }
            wr(out + 20, low(q64(p, 24), "bmBits"));
            wr(out + 76, q64(p, 88) as u32);
            wr(out + 80, r64(p, 96));
            return DIBSECTION32;
        }
        OBJ_BITMAP => {
            // bmType, bmWidth, bmHeight, bmWidthBytes, bmPlanes et
            // bmBitsPixel (16 + 4 octets), puis bmBits.
            unsafe { std::ptr::copy_nonoverlapping(p as *const u8, out as usize as *mut u8, 20) };
            wr(out + 20, low(q64(p, 24), "bmBits"));
        }
        _ => {
            wr(out, r64(p, 0));
            wr(out + 4, r64(p, 4));
            wr(out + 8, q64(p, 8) as u32);
        }
    }
    size32
}

/// CreateDIBSection : *ppvBits est un pointeur de 4 octets en x86.
extern "win64" fn create_dib_section(args: u32, target: usize) -> u64 {
    let ppv = arg(args, 3);
    let mut bits = 0u64;
    let r = call64(
        target,
        &[
            ext(arg(args, 0)) as u64,
            arg(args, 1) as u64,
            arg(args, 2) as u64,
            if ppv == 0 {
                0
            } else {
                &mut bits as *mut u64 as u64
            },
            ext(arg(args, 4)) as u64,
            arg(args, 5) as u64,
        ],
    );
    if ppv != 0 {
        wr(ppv, low(bits, "DIB section"));
    }
    r
}

/// CreateBrushIndirect : LOGBRUSH de 12 octets en x86, 16 en x64.
extern "win64" fn create_brush_indirect(args: u32, target: usize) -> u64 {
    let lb = arg(args, 0);
    if lb == 0 {
        return call64(target, &[0]);
    }
    let b = [rd(lb) as u64 | (rd(lb + 4) as u64) << 32, rd(lb + 8) as u64];
    call64(target, &[b.as_ptr() as u64])
}

/// MENUITEMINFO jusqu'à hbmpItem : hSubMenu, hbmpChecked, hbmpUnchecked,
/// dwItemData, dwTypeData et hbmpItem ont 8 octets en x64. cbSize vaut 44
/// (sans hbmpItem) ou 48 en x86.
static MENUITEMINFO: Layout = Layout {
    size32: 48,
    size64: 80,
    fields: &[
        F::Size(0, 0),
        F::D(4, 4),
        F::D(8, 8),
        F::D(12, 12),
        F::D(16, 16),
        F::P(20, 24),
        F::P(24, 32),
        F::P(28, 40),
        F::P(32, 48),
        F::P(36, 56),
        F::D(40, 64),
        F::P(44, 72),
    ],
};

/// Get/SetMenuItemInfo, InsertMenuItem : (menu, item, byPosition, info).
/// `spec.at` vaut 1 quand la structure est recopiée au retour.
extern "win64" fn menu_item_info(args: u32, s: usize) -> u64 {
    let s = unsafe { &*(s as *const Spec) };
    let info = arg(args, 3);
    let head = [
        ext(arg(args, 0)) as u64,
        arg(args, 1) as u64,
        arg(args, 2) as u64,
    ];
    if info == 0 {
        return call64(s.target, &[head[0], head[1], head[2], 0]);
    }
    let given = (rd(info) as usize).clamp(44, MENUITEMINFO.size32);
    let mut x86 = [0u8; 48];
    unsafe { std::ptr::copy_nonoverlapping(info as usize as *const u8, x86.as_mut_ptr(), given) };
    let mut wide = unsafe { MENUITEMINFO.boxed64(x86.as_ptr() as usize) };
    let r = call64(
        s.target,
        &[head[0], head[1], head[2], wide.as_mut_ptr() as u64],
    );
    if s.at == 1 && r != 0 {
        unsafe {
            MENUITEMINFO.to32(wide.as_ptr() as *const u8, x86.as_mut_ptr() as usize);
            std::ptr::copy_nonoverlapping(x86.as_ptr(), info as usize as *mut u8, given);
        }
    }
    r
}

/// ICONINFO x64 : fIcon, xHotspot, yHotspot, 4 octets de remplissage,
/// hbmMask (+16), hbmColor (+24). En x86 : hbmMask +12, hbmColor +16.
fn iconinfo_to_x86(p: usize, out: u32) {
    wr(out, r64(p, 0));
    wr(out + 4, r64(p, 4));
    wr(out + 8, r64(p, 8));
    wr(out + 12, low(q64(p, 16), "hbmMask"));
    wr(out + 16, low(q64(p, 24), "hbmColor"));
}

fn iconinfo_from_x86(info: u32) -> [u64; 4] {
    [
        rd(info) as u64 | (rd(info + 4) as u64) << 32,
        rd(info + 8) as u64,
        ext(rd(info + 12)) as u64,
        ext(rd(info + 16)) as u64,
    ]
}

/// GetIconInfo : le runtime écrirait 32 octets dans les 20 du programme
/// (WinSCP y perdait son cadre de pile : #719).
extern "win64" fn get_icon_info(args: u32, target: usize) -> u64 {
    let (icon, out) = (ext(arg(args, 0)), arg(args, 1));
    if out == 0 {
        return call64(target, &[icon as u64, 0]);
    }
    let mut buf = [0u64; 4];
    let r = call64(target, &[icon as u64, buf.as_mut_ptr() as u64]);
    if r != 0 {
        iconinfo_to_x86(buf.as_ptr() as usize, out);
    }
    r
}

extern "win64" fn create_icon_indirect(args: u32, target: usize) -> u64 {
    let info = arg(args, 0);
    if info == 0 {
        return call64(target, &[0]);
    }
    let b = iconinfo_from_x86(info);
    call64(target, &[b.as_ptr() as u64])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menuiteminfo_x86() {
        // 7zFM : MIIM_STRING|MIIM_SUBMENU, texte dans un tampon x86.
        let low = crate::wow32::alloc_low(4096, false);
        let x86 = unsafe { std::slice::from_raw_parts_mut(low as *mut u32, 12) };
        let buf = (low + 1024) as u32;
        x86.copy_from_slice(&[48, 0x44, 0, 0, 7, 0x1234, 0, 0, 0x99, buf, 64, 0]);
        let w = unsafe { MENUITEMINFO.boxed64(low) };
        let p = w.as_ptr() as usize;
        assert_eq!(r64(p, 0), 80, "cbSize x64");
        assert_eq!(q64(p, 24), 0x1234, "hSubMenu");
        assert_eq!(q64(p, 48), 0x99, "dwItemData");
        assert_eq!(q64(p, 56), buf as u64, "dwTypeData");
        assert_eq!(r64(p, 64), 64, "cch");
        let mut back = [0u32; 12];
        unsafe { MENUITEMINFO.to32(p as *const u8, back.as_mut_ptr() as usize) };
        assert_eq!(&back[1..], &x86[1..]);
    }

    #[test]
    fn iconinfo_x86() {
        let mut x64 = [0u64; 4];
        let p = x64.as_mut_ptr() as usize;
        w64(p, 0, 1);
        w64(p, 4, 16);
        w64(p, 8, 24);
        unsafe { ((p + 16) as *mut u64).write(0x51) };
        unsafe { ((p + 24) as *mut u64).write(0x52) };
        // Cinq mots x86 sous 4 Go puis une sentinelle qui ne doit pas bouger.
        let low = crate::wow32::alloc_low(4096, false);
        let out = unsafe { std::slice::from_raw_parts_mut(low as *mut u32, 6) };
        out.fill(0xAAAA_AAAA);
        iconinfo_to_x86(p, low as u32);
        assert_eq!(out, [1, 16, 24, 0x51, 0x52, 0xAAAA_AAAA]);
        let back = iconinfo_from_x86(low as u32);
        assert_eq!(back, [1 | 16 << 32, 24, 0x51, 0x52]);
    }

    #[test]
    fn createstruct_aller_retour() {
        let src: [u32; 12] = [
            0x1234,
            0x40_0000,
            7,
            0x40002,
            600,
            800,
            10,
            20,
            0x00cf_0000,
            0x5000,
            0x6000,
            0x200,
        ];
        // Structure x86 sous 4 Go, comme dans un vrai programme 32 bits.
        let low = crate::wow32::alloc_low(4096, false);
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), low as *mut u32, 12) };
        let mut wide = [0u64; 10];
        create_to64(low as u32, &mut wide);
        let p = wide.as_ptr() as usize;
        assert_eq!(q64(p, 0), 0x1234);
        assert_eq!(q64(p, 24), 0x40002);
        assert_eq!(r64(p, 32), 600);
        assert_eq!(r64(p, 48), 0x00cf_0000);
        assert_eq!(q64(p, 56), 0x5000);
        assert_eq!(q64(p, 64), 0x6000);
        assert_eq!(r64(p, 72), 0x200);
        let mut back = [0u32; 12];
        create_to32(p, &mut back);
        assert_eq!(back, src);
    }

    #[test]
    fn windowpos_aller_retour() {
        let src: [u32; 7] = [0x40001, u32::MAX, 1, 2, 3, 4, 0x14];
        let mut wide = [0u64; 5];
        let p = wide.as_mut_ptr() as usize;
        pos_to64(src.as_ptr() as usize, p);
        assert_eq!(q64(p, 8), u64::MAX, "HWND_TOPMOST garde son signe");
        let mut back = [0u32; 7];
        pos_to32(p, back.as_mut_ptr() as usize);
        assert_eq!(back, src);
    }
}
