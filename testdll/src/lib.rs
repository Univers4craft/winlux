#![no_std]

static ATTACH_KIND: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[no_mangle]
pub extern "system" fn DllMain(_: usize, reason: u32, reserved: usize) -> i32 {
    if reason == 1 {
        ATTACH_KIND.store(
            if reserved == 0 { 2 } else { 1 },
            core::sync::atomic::Ordering::SeqCst,
        );
    }
    1
}

#[no_mangle]
pub extern "system" fn winlux_attach_kind() -> u32 {
    ATTACH_KIND.load(core::sync::atomic::Ordering::SeqCst)
}

#[no_mangle]
pub extern "system" fn winlux_add(a: u32, b: u32) -> u32 {
    a.wrapping_add(b)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
