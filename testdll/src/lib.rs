#![no_std]

#[no_mangle]
pub extern "system" fn winlux_add(a: u32, b: u32) -> u32 { a.wrapping_add(b) }

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
