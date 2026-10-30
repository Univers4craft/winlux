//! Gestionnaire des tâches Winlux (fenêtre autonome)
#![no_std]
#![no_main]

use winlux_testexe::*;

#[path = "../shell/taskmgr.rs"]
mod taskmgr;
#[path = "../shell/gfx.rs"]
mod gfx;
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
            lpszClassName: c"WinluxTaskMgr".as_ptr(),
        };
        RegisterClassA(&class);

        let hwnd = CreateWindowExA(
            0,
            c"WinluxTaskMgr".as_ptr(),
            c"Gestionnaire des t\xE2ches".as_ptr(),
            0x00CF0000,
            80,
            60,
            1000,
            650,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            GetModuleHandleA(core::ptr::null()),
            core::ptr::null_mut(),
        );

        ShowWindow(hwnd, 1);
        UpdateWindow(hwnd);

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
            PostQuitMessage(0);
            0
        }
        0x000F => {
            // WM_PAINT
            let mut ps: PAINTSTRUCT = core::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut outer = Rect { left: 0, top: 0, right: 1000, bottom: 650 };
            GetClientRect(hwnd, &mut outer);
            let frame = wm::Frame {
                outer,
                title: Rect { left: 0, top: 0, right: outer.right, bottom: 0 },
                client: outer,
            };
            let fonts = gfx::Fonts::new();
            let mut brushes = gfx::Brushes::new();
            SetBkMode(hdc, 1);
            let mut g = wm::Gfx { hdc, b: &mut brushes, f: &fonts };
            taskmgr::draw(&mut g, &frame);
            brushes.release();
            fonts.release();
            EndPaint(hwnd, &ps);
            0
        }
        0x0201 => {
            // WM_LBUTTONDOWN
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut outer = Rect { left: 0, top: 0, right: 1000, bottom: 650 };
            GetClientRect(hwnd, &mut outer);
            let frame = wm::Frame {
                outer,
                title: Rect { left: 0, top: 0, right: outer.right, bottom: 0 },
                client: outer,
            };
            if taskmgr::mouse(wm::Mouse::Down, x, y, &frame) {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
            0
        }
        0x0200 => {
            // WM_MOUSEMOVE
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut outer = Rect { left: 0, top: 0, right: 1000, bottom: 650 };
            GetClientRect(hwnd, &mut outer);
            let frame = wm::Frame {
                outer,
                title: Rect { left: 0, top: 0, right: outer.right, bottom: 0 },
                client: outer,
            };
            if taskmgr::mouse(wm::Mouse::Move, x, y, &frame) {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
            0
        }
        0x020A => {
            // WM_MOUSEWHEEL
            let delta = ((wparam >> 16) as i16 as i32) / 120;
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut outer = Rect { left: 0, top: 0, right: 1000, bottom: 650 };
            GetClientRect(hwnd, &mut outer);
            let frame = wm::Frame {
                outer,
                title: Rect { left: 0, top: 0, right: outer.right, bottom: 0 },
                client: outer,
            };
            if taskmgr::mouse(wm::Mouse::Wheel(delta), x, y, &frame) {
                InvalidateRect(hwnd, core::ptr::null(), 0);
            }
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}
