//! Lecteur PDF
#![no_std]
#![no_main]

use winlux_testexe::*;

#[path = "../shell/pdfviewer.rs"]
mod pdfviewer;
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
            lpszClassName: c"WinluxPDFViewer".as_ptr(),
        };
        RegisterClassA(&class);

        let hwnd = CreateWindowExA(
            0,
            c"WinluxPDFViewer".as_ptr(),
            c"Lecteur PDF".as_ptr(),
            0x00CF0000,
            100,
            100,
            600,
            500,
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
            let mut ps: PAINTSTRUCT = core::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let fonts = gfx::Fonts::new();
            let mut brushes = gfx::Brushes::new();
            SetBkMode(hdc, 1);
            let frame = wm::Frame::new(hwnd);
            pdfviewer::draw(&mut wm::Gfx { hdc, b: &mut brushes, f: &fonts }, &frame);
            brushes.release();
            fonts.release();
            EndPaint(hwnd, &ps);
            0
        }
        0x0201 => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let frame = wm::Frame::new(hwnd);
            pdfviewer::mouse(wm::Mouse::LeftClick, x, y, &frame);
            InvalidateRect(hwnd, core::ptr::null_mut(), 1);
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}
