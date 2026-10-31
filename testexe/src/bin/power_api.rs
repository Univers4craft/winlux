//! API d'alimentation Win32 vues par un .exe : batterie, inhibition de la
//! veille et modes d'alimentation. `scripts/test-runtime.sh` fournit un faux
//! sysfs (WINLUX_POWER_ROOT) : batterie à 75 %, 30 Wh restants à 10 W.
#![no_std]
#![no_main]

use core::ptr::{null, null_mut};
use winlux_testexe::*;

const ES_CONTINUOUS: u32 = 0x8000_0000;
const ES_SYSTEM_REQUIRED: u32 = 1;
const ES_DISPLAY_REQUIRED: u32 = 2;
/// 381b4222-f694-41f0-9685-ff5bb260df2e
const BALANCED: [u8; 16] = [
    0x22, 0x42, 0x1b, 0x38, 0x94, 0xf6, 0xf0, 0x41, 0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e,
];
/// a1841308-3541-4fab-bc81-f71556f20b4a
const SAVER: [u8; 16] = [
    0x08, 0x13, 0x84, 0xa1, 0x41, 0x35, 0xab, 0x4f, 0xbc, 0x81, 0xf7, 0x15, 0x56, 0xf2, 0x0b, 0x4a,
];

unsafe fn check(ok: bool, what: &core::ffi::CStr, code: i32) {
    if !ok {
        printf(c"power_api: ECHEC %s\n".as_ptr(), what.as_ptr());
        exit(code);
    }
}

unsafe fn active() -> [u8; 16] {
    let mut guid: *mut [u8; 16] = null_mut();
    check(
        PowerGetActiveScheme(null_mut(), &mut guid) == 0 && !guid.is_null(),
        c"PowerGetActiveScheme",
        10,
    );
    let value = *guid;
    LocalFree(guid.cast());
    value
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let mut st = SystemPowerStatus::default();
        check(GetSystemPowerStatus(&mut st) != 0, c"GetSystemPowerStatus", 2);
        printf(
            c"secteur=%u drapeaux=%u batterie=%u%% autonomie=%u s\n".as_ptr(),
            st.ac_line_status as u32,
            st.battery_flag as u32,
            st.battery_life_percent as u32,
            st.battery_life_time,
        );
        check(st.ac_line_status == 0, c"ACLineStatus", 3);
        check(st.battery_life_percent == 75, c"BatteryLifePercent", 4);
        check(st.battery_flag == 1, c"BatteryFlag (haute)", 5);
        check(st.battery_life_time == 10_800, c"BatteryLifeTime", 6);
        check(st.battery_full_life_time == 14_400, c"BatteryFullLifeTime", 7);

        // Lecteur vidéo : garder l'écran et le système éveillés.
        let first = SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        check(first & ES_CONTINUOUS != 0, c"SetThreadExecutionState", 8);
        let previous = SetThreadExecutionState(ES_CONTINUOUS);
        check(
            previous & (ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) == ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED,
            c"SetThreadExecutionState (etat precedent)",
            9,
        );
        let request = PowerCreateRequest(null());
        check(
            !request.is_null()
                && PowerSetRequest(request, 1) != 0
                && PowerClearRequest(request, 1) != 0
                && PowerSetRequest(request, 99) == 0,
            c"PowerSetRequest",
            11,
        );

        check(active() == BALANCED, c"mode par defaut Equilibre", 12);
        check(PowerSetActiveScheme(null_mut(), &SAVER) == 0, c"PowerSetActiveScheme", 13);
        check(active() == SAVER, c"mode Economie applique", 14);
        let mut st = SystemPowerStatus::default();
        GetSystemPowerStatus(&mut st);
        check(st.system_status_flag == 1, c"SystemStatusFlag (economiseur)", 15);
        let unknown = [0u8; 16];
        check(PowerSetActiveScheme(null_mut(), &unknown) != 0, c"GUID inconnu refuse", 16);
        let mut size = 0u32;
        check(
            PowerReadFriendlyName(null_mut(), &SAVER, null(), null(), null_mut(), &mut size) == 0
                && size > 4,
            c"PowerReadFriendlyName",
            17,
        );
        check(PowerSetActiveScheme(null_mut(), &BALANCED) == 0, c"retour Equilibre", 18);
        printf(c"power_api: OK\n".as_ptr());
        exit(0)
    }
}
