//! Informations système Winlux (fenêtre autonome, comme msinfo32.exe)
#![no_std]
#![no_main]
#![allow(static_mut_refs, dead_code)]

use winlux_testexe::*;

#[path = "../shell/admin_ui.rs"]
mod admin_ui;
#[path = "../shell/gfx.rs"]
mod gfx;
#[path = "../shell/gpuinfo.rs"]
mod gpuinfo;
#[path = "../shell/locale.rs"]
mod locale;
#[path = "../shell/sysinfo.rs"]
mod sysinfo;
#[path = "../shell/wm_standalone.rs"]
mod wm;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let class = WNDCLASSA {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleA(core::ptr::null()),
            hIcon: core::ptr::null_mut(),
            hCursor: LoadCursorA(core::ptr::null_mut(), 32512),
            hbrBackground: 6,
            lpszMenuName: core::ptr::null(),
            lpszClassName: c"WinluxSysInfo".as_ptr(),
        };
        RegisterClassA(&class);

        let hwnd = CreateWindowExA(
            0,
            c"WinluxSysInfo".as_ptr(),
            tr4(&[
                b"Informations syst\xE8me\0",
                b"System Information\0",
                b"Informaci\xF3n del sistema\0",
                b"Systeminformationen\0",
            ])
            .as_ptr()
            .cast(),
            0x00CF0000,
            80,
            60,
            1060,
            700,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            GetModuleHandleA(core::ptr::null()),
            core::ptr::null_mut(),
        );

        ShowWindow(hwnd, 1);
        UpdateWindow(hwnd);
        SetTimer(hwnd, 1, 1000, 0);

        let mut msg: MSG = core::mem::zeroed();
        while GetMessageA(&mut msg, core::ptr::null_mut(), 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }

        ExitProcess(msg.wparam as u32)
    }
}

unsafe fn mouse(hwnd: Handle, ev: wm::Mouse, lparam: isize) {
    let x = (lparam & 0xFFFF) as i16 as i32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
    let frame = wm::Frame::new(hwnd);
    if sysinfo::mouse(ev, x, y, &frame) {
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

unsafe extern "system" fn wndproc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        0x0002 => {
            PostQuitMessage(0);
            0
        }
        0x000F => {
            let mut ps: PAINTSTRUCT = core::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let fonts = gfx::Fonts::new();
            let mut brushes = gfx::Brushes::new();
            SetBkMode(hdc, 1);
            let frame = wm::Frame::new(hwnd);
            sysinfo::draw(
                &mut wm::Gfx {
                    hdc,
                    b: &mut brushes,
                    f: &fonts,
                },
                &frame,
            );
            brushes.release();
            fonts.release();
            EndPaint(hwnd, &ps);
            0
        }
        0x0201 => {
            mouse(hwnd, wm::Mouse::Down, lparam);
            0
        }
        0x0200 => {
            mouse(hwnd, wm::Mouse::Move, lparam);
            0
        }
        0x020A => {
            // La molette donne des coordonnées d'écran : on garde la
            // dernière position connue, sysinfo n'en a pas besoin.
            let delta = ((wparam >> 16) as i16 as i32) / 120;
            let frame = wm::Frame::new(hwnd);
            if sysinfo::mouse(wm::Mouse::Wheel(delta), 0, 0, &frame) {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
            0
        }
        0x0100 => {
            // WM_KEYDOWN
            if sysinfo::key(wparam as u32, 0) {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
            0
        }
        0x0102 => {
            // WM_CHAR
            if sysinfo::key(0, wparam as u16) {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
            0
        }
        0x0113 => {
            // WM_TIMER
            let frame = wm::Frame::new(hwnd);
            if sysinfo::tick(&frame) {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}
