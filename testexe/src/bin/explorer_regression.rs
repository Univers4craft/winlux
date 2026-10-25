//! Régressions des opérations et des raccourcis du vrai module Explorer.
#![no_std]
#![no_main]
#![allow(static_mut_refs, dead_code)]

use winlux_testexe::*;
#[path = "../shell/desktop.rs"]
mod desktop;
#[path = "../shell/explorer.rs"]
mod explorer;
#[path = "../shell/gfx.rs"]
mod gfx;
#[path = "../shell/settings.rs"]
mod settings;
#[path = "../shell/wm.rs"]
mod wm;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        explorer::regression_checks();
    }
    finish()
}
