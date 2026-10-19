use std::ffi::c_void;

pub unsafe extern "win64" fn message_box_a(_hwnd: usize, text: *const u8, caption: *const u8, _typ: u32) -> i32 {
    eprintln!("winlux: MessageBoxA [{}] {}", crate::dlls::cstr(caption), crate::dlls::cstr(text));
    1
}
pub extern "win64" fn get_system_metrics(index: i32) -> i32 {
    match index { 0 => 1920, 1 => 1080, 2 => 4, _ => 0 }
}
pub extern "win64" fn get_desktop_window() -> usize { 1 }
pub extern "win64" fn get_foreground_window() -> usize { 1 }
pub extern "win64" fn is_window(hwnd: usize) -> i32 { (hwnd != 0) as i32 }
pub extern "win64" fn show_window(_hwnd: usize, _cmd: i32) -> i32 { 1 }
pub extern "win64" fn post_quit_message(_code: i32) {}
pub unsafe extern "win64" fn peek_message_a(_msg: *mut c_void, _hwnd: usize, _min: u32, _max: u32, _remove: u32) -> i32 { 0 }
pub unsafe extern "win64" fn translate_message(_msg: *const c_void) -> i32 { 1 }
pub unsafe extern "win64" fn dispatch_message_a(_msg: *const c_void) -> usize { 0 }

pub fn exports() -> crate::dlls::Exports { crate::exports! {
    "MessageBoxA"=>fn message_box_a, "GetSystemMetrics"=>fn get_system_metrics,
    "GetDesktopWindow"=>fn get_desktop_window, "GetForegroundWindow"=>fn get_foreground_window,
    "IsWindow"=>fn is_window, "ShowWindow"=>fn show_window, "PostQuitMessage"=>fn post_quit_message,
    "PeekMessageA"=>fn peek_message_a, "TranslateMessage"=>fn translate_message, "DispatchMessageA"=>fn dispatch_message_a
} }
