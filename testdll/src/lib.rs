#![no_std]

static ATTACH_KIND: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
/// Nombre de DLL_PROCESS_ATTACH reçus par cette image (#24) : une DLL
/// vraiment déchargée puis rechargée repart d'une image neuve, donc de 1.
static ATTACHES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
/// Marque posée par le programme ; une image neuve la remet à 0 (#24).
static MARK: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[no_mangle]
pub extern "system" fn DllMain(_: usize, reason: u32, reserved: usize) -> i32 {
    if reason == 1 {
        ATTACHES.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
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
pub extern "system" fn winlux_attach_count() -> u32 {
    ATTACHES.load(core::sync::atomic::Ordering::SeqCst)
}

#[no_mangle]
pub extern "system" fn winlux_set_mark(v: u32) {
    MARK.store(v, core::sync::atomic::Ordering::SeqCst);
}

#[no_mangle]
pub extern "system" fn winlux_mark() -> u32 {
    MARK.load(core::sync::atomic::Ordering::SeqCst)
}

#[no_mangle]
pub extern "system" fn winlux_add(a: u32, b: u32) -> u32 {
    a.wrapping_add(b)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
