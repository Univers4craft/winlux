//! Visionneuse d'images : `imageviewer.exe [image]`. Le bureau l'ouvre pour
//! toute image d'un format décodé par Winlux (ShellExecute, double-clic).
#![no_std]
#![no_main]

use winlux_testexe::*;

#[path = "../shell/gfx.rs"]
#[allow(dead_code)]
mod gfx;
#[path = "../shell/imageviewer.rs"]
mod imageviewer;
#[path = "../shell/locale.rs"]
#[allow(dead_code)]
mod locale;
#[path = "../shell/wm_standalone.rs"]
#[allow(dead_code)]
mod wm;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let class = WNDCLASSA {
            style: 0x0008, // CS_DBLCLKS
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleA(core::ptr::null()),
            hIcon: core::ptr::null_mut(),
            hCursor: LoadCursorA(core::ptr::null_mut(), 32512),
            hbrBackground: 0,
            lpszMenuName: core::ptr::null(),
            lpszClassName: c"WinluxImageViewer".as_ptr(),
        };
        RegisterClassA(&class);

        let hwnd = CreateWindowExA(
            0,
            c"WinluxImageViewer".as_ptr(),
            c"Visionneuse d'images".as_ptr(),
            0x00CF0000,
            80,
            60,
            1100,
            760,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            GetModuleHandleA(core::ptr::null()),
            core::ptr::null_mut(),
        );
        imageviewer::init(hwnd);

        ShowWindow(hwnd, 1);
        UpdateWindow(hwnd);

        let mut msg: MSG = core::mem::zeroed();
        while GetMessageA(&mut msg, core::ptr::null_mut(), 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
        imageviewer::shutdown();
        ExitProcess(msg.wparam as u32)
    }
}

fn xy(lparam: isize) -> (i32, i32) {
    (
        (lparam & 0xFFFF) as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as i16 as i32,
    )
}

unsafe extern "system" fn wndproc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        0x0002 => {
            PostQuitMessage(0);
            0
        }
        0x0005 => {
            InvalidateRect(hwnd, core::ptr::null(), 0);
            0
        }
        0x000F => {
            let mut ps: PAINTSTRUCT = core::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let fonts = gfx::Fonts::new();
            let mut brushes = gfx::Brushes::new();
            SetBkMode(hdc, 1);
            let frame = wm::Frame::new(hwnd);
            imageviewer::paint(
                &mut wm::Gfx {
                    hdc,
                    b: &mut brushes,
                    f: &fonts,
                },
                &frame.client,
            );
            brushes.release();
            fonts.release();
            EndPaint(hwnd, &ps);
            0
        }
        0x0014 => 1, // WM_ERASEBKGND : tout est repeint par paint()
        0x0100 => {
            imageviewer::key(wparam as u32);
            0
        }
        0x0113 => {
            imageviewer::timer(wparam);
            0
        }
        0x0200 => {
            let (x, y) = xy(lparam);
            imageviewer::mouse_move(x, y);
            0
        }
        0x0201 => {
            let (x, y) = xy(lparam);
            imageviewer::mouse_down(x, y);
            0
        }
        0x0202 => {
            imageviewer::mouse_up();
            0
        }
        0x0203 => {
            let (x, y) = xy(lparam);
            imageviewer::double_click(x, y);
            0
        }
        0x020A => {
            let (x, y) = xy(lparam);
            let mut pt = Point { x, y };
            ScreenToClient(hwnd, &mut pt);
            imageviewer::wheel(((wparam >> 16) & 0xFFFF) as i16 as i32, pt.x, pt.y);
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}
