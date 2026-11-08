//! DLL de test qui importe testdll.dll (#691). Un programme la charge par
//! LoadLibrary : testdll.dll arrive alors seulement comme dépendance, et
//! son DllMain doit tout de même être appelé avant celui-ci, comme sous
//! Windows.

#![no_std]

#[link(name = "testdll", kind = "raw-dylib")]
extern "system" {
    fn winlux_attach_kind() -> u32;
}

static DEP_AT_ATTACH: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(u32::MAX);

#[no_mangle]
pub extern "system" fn DllMain(_: usize, reason: u32, _: usize) -> i32 {
    if reason == 1 {
        // La dépendance est initialisée avant nous.
        DEP_AT_ATTACH.store(
            unsafe { winlux_attach_kind() },
            core::sync::atomic::Ordering::SeqCst,
        );
    }
    1
}

/// Ce que testdll.dll disait de son DllMain quand le nôtre a été appelé
/// (0 : jamais initialisée, 2 : chargement dynamique).
#[no_mangle]
pub extern "system" fn winlux_dep_attach_kind() -> u32 {
    DEP_AT_ATTACH.load(core::sync::atomic::Ordering::SeqCst)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
