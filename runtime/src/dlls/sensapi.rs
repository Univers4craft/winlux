//! sensapi.dll : état du réseau (Notepad++ s'en sert pour ses mises à jour).

/// NETWORK_ALIVE_LAN : une interface autre que la boucle locale est active.
const NETWORK_ALIVE_LAN: u32 = 1;

fn network_up() -> bool {
    std::fs::read_dir("/sys/class/net")
        .map(|dir| {
            dir.flatten().any(|e| {
                e.file_name() != "lo"
                    && std::fs::read_to_string(e.path().join("operstate"))
                        .is_ok_and(|s| s.trim() == "up")
            })
        })
        .unwrap_or(false)
}

pub unsafe extern "win64" fn is_network_alive(flags: *mut u32) -> i32 {
    let up = network_up();
    if !flags.is_null() {
        *flags = if up { NETWORK_ALIVE_LAN } else { 0 };
    }
    if !up {
        crate::process::set_last_error(0);
    }
    up as i32
}

pub unsafe extern "win64" fn is_destination_reachable(_dest: usize, qoc: *mut u32) -> i32 {
    if !qoc.is_null() && *qoc >= 4 {
        *qoc.add(1) = if network_up() { NETWORK_ALIVE_LAN } else { 0 };
    }
    network_up() as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "IsNetworkAlive" => fn is_network_alive,
        "IsDestinationReachableA" => fn is_destination_reachable,
        "IsDestinationReachableW" => fn is_destination_reachable,
    }
}
