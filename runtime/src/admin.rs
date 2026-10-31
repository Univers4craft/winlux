//! Outils communs aux services d'administration Winlux (planificateur de
//! tâches, pare-feu) : racine du système de fichiers, fichiers texte lisibles
//! par le bureau Win32 et heure locale.
//!
//! Les fichiers échangés avec le bureau sont en Latin-1 : le shell les lit et
//! les écrit avec les API « A » (TextOutA, CreateFileA). On accepte aussi
//! l'UTF-8 en lecture pour les fichiers édités à la main.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Racine du système (`/` en production, un dossier temporaire en test).
pub fn root() -> PathBuf {
    match std::env::var_os("WINLUX_ADMIN_ROOT") {
        Some(r) if !r.is_empty() => PathBuf::from(r),
        _ => PathBuf::from("/"),
    }
}

/// Chemin absolu `rel` sous la racine (sans `/` initial).
pub fn path(rel: &str) -> PathBuf {
    root().join(rel.trim_start_matches('/'))
}

/// Décode un fichier : UTF-8 s'il est valide, sinon Latin-1.
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Encode en Latin-1 ; les caractères hors Latin-1 deviennent `?`.
pub fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' })
        .collect()
}

pub fn read_text(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| decode(&b))
}

/// Écrit `text` de façon atomique (fichier temporaire puis renommage).
pub fn write_text(p: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, encode(text))?;
    std::fs::rename(&tmp, p)
}

/// Ajoute une ligne à un journal ; au-delà de `max` octets on garde la
/// seconde moitié pour borner la taille.
pub fn append_line(p: &Path, line: &str, max: u64) {
    use std::io::Write;
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(p).map(|m| m.len() > max).unwrap_or(false) {
        if let Ok(data) = std::fs::read(p) {
            let half = data.len() / 2;
            let cut = data[half..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(data.len(), |i| half + i + 1);
            let _ = std::fs::write(p, &data[cut..]);
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let mut bytes = encode(line);
        bytes.push(b'\n');
        let _ = f.write_all(&bytes);
    }
}

/// Valeur `clé=valeur` d'un texte de configuration.
pub fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        l.strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
            .map(str::trim_end)
    })
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Heure locale décomposée.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Local {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// 0 = dimanche … 6 = samedi.
    pub weekday: u32,
    /// Décalage par rapport à UTC, en secondes.
    pub offset: i64,
}

pub fn local(epoch: i64) -> Local {
    // SAFETY: localtime_r écrit dans `tm`, une structure locale initialisée.
    unsafe {
        let t = epoch as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return Local::default();
        }
        Local {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            minute: tm.tm_min as u32,
            second: tm.tm_sec as u32,
            weekday: tm.tm_wday as u32,
            offset: tm.tm_gmtoff,
        }
    }
}

/// Instant correspondant à une date locale ; `mktime` normalise les
/// débordements (jour 32 → mois suivant).
pub fn epoch_of(year: i32, month: u32, day: i64, hour: u32, minute: u32) -> i64 {
    // SAFETY: mktime lit et normalise une structure locale initialisée.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = year - 1900;
        tm.tm_mon = month as i32 - 1;
        tm.tm_mday = day as i32;
        tm.tm_hour = hour as i32;
        tm.tm_min = minute as i32;
        tm.tm_isdst = -1;
        libc::mktime(&mut tm) as i64
    }
}

/// « 31/10/2026 14:05 ».
pub fn format_time(epoch: i64) -> String {
    let l = local(epoch);
    format!(
        "{:02}/{:02}/{:04} {:02}:{:02}",
        l.day, l.month, l.year, l.hour, l.minute
    )
}

/// Vrai si une route par défaut existe (IPv4 ou IPv6).
pub fn network_available() -> bool {
    if let Some(routes) = read_text(&path("proc/net/route")) {
        if routes.lines().skip(1).any(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.len() > 2 && f[1] == "00000000" && f[7.min(f.len() - 1)] == "00000000"
        }) {
            return true;
        }
    }
    if let Some(routes) = read_text(&path("proc/net/ipv6_route")) {
        return routes.lines().any(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.len() >= 10
                && f[0] == "00000000000000000000000000000000"
                && f[1] == "00"
                && f[9] != "lo"
        });
    }
    false
}

/// Vrai sur secteur, ou si la machine n'a pas de batterie.
pub fn on_ac_power() -> bool {
    let Ok(entries) = std::fs::read_dir(path("sys/class/power_supply")) else {
        return true;
    };
    let mut battery_discharging = false;
    let mut has_battery = false;
    let mut mains_online = false;
    let mut has_mains = false;
    for e in entries.flatten() {
        let dir = e.path();
        let kind = read_text(&dir.join("type")).unwrap_or_default();
        match kind.trim() {
            "Battery" => {
                has_battery = true;
                let status = read_text(&dir.join("status")).unwrap_or_default();
                battery_discharging |= status.trim() == "Discharging";
            }
            "Mains" | "USB" | "USB_C" | "USB_PD" => {
                has_mains = true;
                mains_online |= read_text(&dir.join("online")).unwrap_or_default().trim() == "1";
            }
            _ => {}
        }
    }
    if !has_battery {
        return true;
    }
    if has_mains {
        mains_online
    } else {
        !battery_discharging
    }
}

#[cfg(test)]
pub(crate) mod test_root {
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    /// Racine temporaire exclusive : les tests qui touchent `WINLUX_ADMIN_ROOT`
    /// s'exécutent l'un après l'autre.
    pub struct Root {
        pub dir: std::path::PathBuf,
        _guard: MutexGuard<'static, ()>,
    }

    impl Root {
        pub fn new(name: &str) -> Root {
            let guard = LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let dir = std::env::temp_dir().join(format!(
                "winlux-admin-{name}-{}-{}",
                std::process::id(),
                super::now()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::env::set_var("WINLUX_ADMIN_ROOT", &dir);
            Root { dir, _guard: guard }
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            std::env::remove_var("WINLUX_ADMIN_ROOT");
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_round_trip_and_utf8_input() {
        assert_eq!(decode(b"t\xE2che"), "tâche");
        assert_eq!(decode("tâche".as_bytes()), "tâche");
        assert_eq!(encode("tâche ✓"), b"t\xE2che ?");
    }

    #[test]
    fn mktime_normalises_and_matches_localtime() {
        let e = epoch_of(2026, 10, 32, 9, 30);
        let l = local(e);
        assert_eq!(
            (l.year, l.month, l.day, l.hour, l.minute),
            (2026, 11, 1, 9, 30)
        );
        assert_eq!(l.weekday, 0, "le 1er novembre 2026 est un dimanche");
    }

    #[test]
    fn power_and_network_read_the_fake_root() {
        let r = test_root::Root::new("power");
        let bat = r.dir.join("sys/class/power_supply/BAT0");
        let ac = r.dir.join("sys/class/power_supply/AC");
        std::fs::create_dir_all(&bat).unwrap();
        std::fs::create_dir_all(&ac).unwrap();
        std::fs::write(bat.join("type"), "Battery\n").unwrap();
        std::fs::write(bat.join("status"), "Discharging\n").unwrap();
        std::fs::write(ac.join("type"), "Mains\n").unwrap();
        std::fs::write(ac.join("online"), "0\n").unwrap();
        assert!(!on_ac_power());
        std::fs::write(ac.join("online"), "1\n").unwrap();
        assert!(on_ac_power());
        assert!(!network_available());
        std::fs::create_dir_all(r.dir.join("proc/net")).unwrap();
        std::fs::write(
            r.dir.join("proc/net/route"),
            "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n\
             eth0\t00000000\t0202000A\t0003\t0\t0\t0\t00000000\n",
        )
        .unwrap();
        assert!(network_available());
    }
}
