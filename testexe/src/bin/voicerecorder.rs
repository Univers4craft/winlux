//! Enregistreur vocal (binaire autonome)
#![no_std]
#![no_main]

use winlux_testexe::*;

#[path = "../shell/gfx.rs"]
mod gfx;

mod wm {
    use crate::gfx::{Brushes, Fonts};
    use winlux_testexe::{Handle, Rect};

    #[derive(Clone, Copy, PartialEq)]
    pub enum Mouse {
        LeftClick,
        LeftDown,
        LeftUp,
        LeftDrag,
        RightClick,
        Move,
        Up,
    }

    pub struct Frame {
        pub outer: Rect,
        pub title: Rect,
        pub client: Rect,
    }

    pub struct Gfx<'a> {
        pub hdc: Handle,
        pub b: &'a mut Brushes,
        pub f: &'a Fonts,
    }
}

#[path = "../shell/voicerecorder.rs"]
mod voicerecorder;

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
            lpszClassName: c"WinluxVoiceRecorder".as_ptr(),
        };
        RegisterClassA(&class);

        let hwnd = CreateWindowExA(
            0,
            c"WinluxVoiceRecorder".as_ptr(),
            c"Enregistreur vocal".as_ptr(),
            0x00CF0000,
            100,
            100,
            420,
            320,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            GetModuleHandleA(core::ptr::null()),
            core::ptr::null_mut(),
        );

        ShowWindow(hwnd, 1);
        UpdateWindow(hwnd);
        SetTimer(hwnd, 1, 100, 0);

        let mut msg: MSG = core::mem::zeroed();
        while GetMessageA(&mut msg, core::ptr::null_mut(), 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }

        ExitProcess(msg.wparam as u32)
    }
}

unsafe extern "system" fn wndproc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        0x0002 => {
            KillTimer(hwnd, 1);
            PostQuitMessage(0);
            0
        }
        0x000F => {
            let mut ps: PAINTSTRUCT = core::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let frame = make_frame(hwnd);
            let fonts = gfx::Fonts::new();
            let mut brushes = gfx::Brushes::new();
            SetBkMode(hdc, 1);
            let mut g = wm::Gfx {
                hdc,
                b: &mut brushes,
                f: &fonts,
            };
            voicerecorder::draw(&mut g, &frame);
            brushes.release();
            fonts.release();
            EndPaint(hwnd, &ps);
            0
        }
        0x0201 => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let frame = make_frame(hwnd);
            voicerecorder::mouse(wm::Mouse::LeftClick, x, y, &frame);
            InvalidateRect(hwnd, core::ptr::null_mut(), 1);
            0
        }
        0x0113 => {
            let frame = make_frame(hwnd);
            if voicerecorder::tick(&frame) {
                InvalidateRect(hwnd, core::ptr::null_mut(), 0);
            }
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}

unsafe fn make_frame(hwnd: Handle) -> wm::Frame {
    let mut r = Rect { left: 0, top: 0, right: 420, bottom: 320 };
    GetClientRect(hwnd, &mut r);
    wm::Frame {
        outer: r,
        title: Rect { left: 0, top: 0, right: r.right, bottom: 0 },
        client: r,
    }
}
