//! Test PE d'un contrôle enfant mis en hauteur 0, comme le libellé de
//! Paint.NET qui le faisait planter (#2992).
//!
//! Sous Windows, SetWindowPos accepte une taille nulle et GetClientRect la
//! relit telle quelle (Learn, « SetWindowPos », « GetClientRect ») ; une
//! fenêtre dont la zone client est vide n'a rien à repeindre. Une fenêtre X
//! ne descend pas sous 1x1 : la suite vérifie que cette contrainte du
//! serveur ne remonte pas jusqu'au programme, et qu'un écho en retard d'une
//! taille précédente ne la remplace pas.

#![no_std]
#![no_main]

use winlux_testexe::*;

const WS_CHILD: u32 = 0x4000_0000;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;

static mut CHILD: Handle = core::ptr::null_mut();
static mut CHILD_PAINTS: u32 = 0;
static mut EMPTY_PAINTS: u32 = 0;

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_PAINT && hwnd == CHILD {
        CHILD_PAINTS += 1;
        let mut c: Rect = core::mem::zeroed();
        GetClientRect(hwnd, &mut c);
        if c.right <= 0 || c.bottom <= 0 {
            EMPTY_PAINTS += 1;
        }
        let mut ps: PaintStruct = core::mem::zeroed();
        BeginPaint(hwnd, &mut ps);
        EndPaint(hwnd, &ps);
        return 0;
    }
    DefWindowProcA(hwnd, msg, wparam, lparam)
}

unsafe fn pump(ms: u32) {
    let start = GetTickCount();
    let mut msg: Msg = core::mem::zeroed();
    while GetTickCount().wrapping_sub(start) < ms {
        while PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, 1) != 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
        Sleep(10);
    }
}

fn out_num(v: i32) {
    let mut buf = [0u8; 12];
    let mut i = buf.len();
    let mut n = (v as i64).unsigned_abs();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if v < 0 {
        i -= 1;
        buf[i] = b'-';
    }
    out(&buf[i..]);
}

unsafe fn client(hwnd: Handle) -> Rect {
    let mut c: Rect = core::mem::zeroed();
    GetClientRect(hwnd, &mut c);
    out(b"client ");
    out_num(c.right);
    out(b"x");
    out_num(c.bottom);
    out(b", WM_PAINT ");
    out_num(CHILD_PAINTS as i32);
    out(b" dont zone vide ");
    out_num(EMPTY_PAINTS as i32);
    out(b"\n");
    c
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let class = c"WinluxLibelleVide";
        let wc = WndClassExA {
            size: core::mem::size_of::<WndClassExA>() as u32,
            style: 0,
            wnd_proc: wnd_proc as *const () as usize,
            cls_extra: 0,
            wnd_extra: 0,
            instance: core::ptr::null_mut(),
            icon: core::ptr::null_mut(),
            cursor: core::ptr::null_mut(),
            background: GetSysColorBrush(15),
            menu_name: core::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: core::ptr::null_mut(),
        };
        check!(RegisterClassExA(&wc) != 0, "RegisterClassExA");
        let main = CreateWindowExA(
            0,
            class.as_ptr(),
            c"Principale".as_ptr(),
            0x00CF_0000 | WS_VISIBLE,
            40,
            40,
            400,
            300,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!main.is_null(), "fenetre principale");
        let child = CreateWindowExA(
            0,
            class.as_ptr(),
            c"".as_ptr(),
            WS_CHILD | WS_VISIBLE,
            0,
            32,
            150,
            0,
            main,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!child.is_null(), "libelle cree en hauteur 0");
        CHILD = child;
        // Comme Paint.NET : deux placements rapprochés, toujours en hauteur 0.
        SetWindowPos(
            child,
            core::ptr::null_mut(),
            0,
            43,
            150,
            0,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SetWindowPos(
            child,
            core::ptr::null_mut(),
            0,
            43,
            196,
            0,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        pump(800);
        let c = client(child);
        check!(
            c.right == 196 && c.bottom == 0,
            "GetClientRect : la hauteur 0 demandee reste 0 (#2992)"
        );
        check!(
            CHILD_PAINTS == 0 && EMPTY_PAINTS == 0,
            "aucun WM_PAINT tant que la zone client est vide (#2992)"
        );
        // Agrandi ensuite : la taille demandée, pas un écho périmé.
        SetWindowPos(
            child,
            core::ptr::null_mut(),
            0,
            43,
            196,
            20,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        pump(500);
        let c = client(child);
        check!(
            c.right == 196 && c.bottom == 20,
            "GetClientRect : 196x20 apres agrandissement (#2992)"
        );
        check!(
            EMPTY_PAINTS == 0,
            "toujours aucun WM_PAINT a zone vide (#2992)"
        );
        ExitProcess(if FAILURES == 0 { 0 } else { 1 });
    }
}
