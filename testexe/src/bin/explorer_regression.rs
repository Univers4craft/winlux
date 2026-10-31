//! Régressions des opérations et des raccourcis du vrai module Explorer.
#![no_std]
#![no_main]
#![allow(static_mut_refs, dead_code)]

use winlux_testexe::*;
#[path = "../shell/bsod.rs"]
mod bsod;
#[path = "../shell/desktop.rs"]
mod desktop;
#[path = "../shell/diskcleaner.rs"]
mod diskcleaner;
#[path = "../shell/diskmgr.rs"]
mod diskmgr;
#[path = "../shell/eventviewer.rs"]
mod eventviewer;
#[path = "../shell/explorer.rs"]
mod explorer;
#[path = "../shell/gfx.rs"]
mod gfx;
#[path = "../shell/locale.rs"]
mod locale;
#[path = "../shell/login.rs"]
mod login;
#[path = "../shell/notepad.rs"]
mod notepad;
#[path = "../shell/notifications.rs"]
mod notifications;
#[path = "../shell/restore.rs"]
mod restore;
#[path = "../shell/settings.rs"]
mod settings;
#[path = "../shell/taskmgr.rs"]
mod taskmgr;
#[path = "../shell/terminal.rs"]
mod terminal;
#[path = "../shell/utilities.rs"]
mod utilities;
#[path = "../shell/weather.rs"]
mod weather;
#[path = "../shell/wm.rs"]
mod wm;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        explorer::regression_checks();
        desktop::regression_checks();
        settings::regression_checks();
    }
    finish()
}
