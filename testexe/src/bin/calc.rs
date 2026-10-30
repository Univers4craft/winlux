//! Calculatrice avec Chronomètre, Minuteur et Réveil
#![no_std]
#![no_main]

use winlux_testexe::*;

#[path = "../shell/calc.rs"]
mod calc;
#[path = "../shell/gfx.rs"]
mod gfx;
#[path = "../shell/wm.rs"]
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
            lpszClassName: c"WinluxCalc".as_ptr(),
        };
        RegisterClassA(&class);
        
        let hwnd = CreateWindowExA(
            0,
            c"WinluxCalc".as_ptr(),
            c"Calculatrice Winlux".as_ptr(),
            0x00CF0000,
            100,
            100,
            400,
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
        
        ExitProcess(msg.wParam as u32)
    }
}

unsafe extern "system" fn wndproc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        0x0002 => { // WM_DESTROY
            PostQuitMessage(0);
            0
        }
        0x000F => { // WM_PAINT
            let mut ps: PAINTSTRUCT = core::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut frame = wm::Frame::new(hwnd);
            wm::resize_frame(&mut frame);
            let mut gfx = wm::Gfx::new(hdc, &frame);
            calc::draw(&mut gfx, &frame);
            gfx.release();
            EndPaint(hwnd, &ps);
            0
        }
        0x0201 => { // WM_LBUTTONDOWN
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let mut frame = wm::Frame::new(hwnd);
            wm::resize_frame(&mut frame);
            calc::mouse(wm::Mouse::LeftClick, x, y, &frame);
            InvalidateRect(hwnd, core::ptr::null_mut(), 1);
            0
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}
