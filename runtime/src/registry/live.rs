//! Vue dynamique (#183) : des clés que Windows tient à jour lui-même et que
//! les .exe lisent directement au lieu de passer par l'API. Elles sont
//! recalculées à chaque chargement du registre d'après les réglages Linux,
//! comme le proxy et les clés réseau.
//!
//! - `HKLM\System\CurrentControlSet\Control\TimeZoneInformation` : fuseau.
//! - `HKLM\System\CurrentControlSet\Control\Nls\CodePage` : pages ACP/OEM.
//! - `HKCU\Keyboard Layout\Preload` : disposition du clavier.
//! - `HKLM\Software\Microsoft\Windows NT\CurrentVersion\Fonts` et son pendant
//!   HKCU : polices présentes dans `C:\Windows\Fonts` et le dossier de
//!   polices de l'utilisateur. Les valeurs écrites par les installateurs
//!   sont gardées ; Winlux n'ajoute que les fichiers qui manquent.

use super::{utf16z, Data, REG_BINARY, REG_DWORD, REG_SZ};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime as Clock;

pub const TIME_ZONE: &str = "hklm\\System\\CurrentControlSet\\Control\\TimeZoneInformation";
pub const CODE_PAGE: &str = "hklm\\System\\CurrentControlSet\\Control\\Nls\\CodePage";
pub const KEYBOARD: &str = "hkcu\\Keyboard Layout\\Preload";
pub const FONTS: &str = "hklm\\Software\\Microsoft\\Windows NT\\CurrentVersion\\Fonts";
pub const USER_FONTS: &str = "hkcu\\Software\\Microsoft\\Windows NT\\CurrentVersion\\Fonts";

fn dword(data: &mut Data, key: &str, name: &str, value: u32) {
    data.raw_set(key, name, REG_DWORD, value.to_le_bytes().to_vec());
}

fn text(data: &mut Data, key: &str, name: &str, value: &str) {
    data.raw_set(key, name, REG_SZ, utf16z(value));
}

fn systemtime_bytes(st: crate::intl::tz::SystemTime) -> Vec<u8> {
    [
        st.year,
        st.month,
        st.day_of_week,
        st.day,
        st.hour,
        st.minute,
        st.second,
        st.millis,
    ]
    .iter()
    .flat_map(|v| v.to_le_bytes())
    .collect()
}

pub fn time_zone(data: &mut Data, z: &crate::intl::tz::Zone) {
    use crate::intl::api::{rule_to_systemtime, zone_state};
    data.raw_ensure(TIME_ZONE);
    let dst_bias = if z.has_dst() { z.dst_bias } else { 0 };
    dword(data, TIME_ZONE, "Bias", z.bias as u32);
    dword(data, TIME_ZONE, "StandardBias", 0);
    dword(data, TIME_ZONE, "DaylightBias", dst_bias as u32);
    let active = if zone_state(z) == 2 {
        z.bias + dst_bias
    } else {
        z.bias
    };
    dword(data, TIME_ZONE, "ActiveTimeBias", active as u32);
    text(data, TIME_ZONE, "StandardName", &z.windows_key());
    text(data, TIME_ZONE, "DaylightName", &z.windows_daylight_name());
    text(data, TIME_ZONE, "TimeZoneKeyName", &z.windows_key());
    data.raw_set(
        TIME_ZONE,
        "StandardStart",
        REG_BINARY,
        systemtime_bytes(rule_to_systemtime(z.dst_end)),
    );
    data.raw_set(
        TIME_ZONE,
        "DaylightStart",
        REG_BINARY,
        systemtime_bytes(rule_to_systemtime(z.dst_start)),
    );
    dword(data, TIME_ZONE, "DynamicDaylightTimeDisabled", 0);
    dword(data, TIME_ZONE, "RealTimeIsUniversal", 1);
}

pub fn code_page(data: &mut Data, ansi: u32, oem: u32) {
    data.raw_ensure(CODE_PAGE);
    text(data, CODE_PAGE, "ACP", &ansi.to_string());
    text(data, CODE_PAGE, "OEMCP", &oem.to_string());
    text(data, CODE_PAGE, "MACCP", "10000");
}

pub fn keyboard(data: &mut Data, layout: crate::intl::keyboard::Layout) {
    data.raw_ensure(KEYBOARD);
    text(data, KEYBOARD, "1", &format!("{:08x}", layout.klid));
}

/// Nom affiché d'une police : « Arial Gras (TrueType) ».
fn font_value_name(full_name: &str, cff: bool) -> String {
    format!(
        "{full_name} ({})",
        if cff { "OpenType" } else { "TrueType" }
    )
}

type FontList = Vec<(String, String)>;
static FONT_CACHE: Mutex<Vec<(PathBuf, Option<Clock>, FontList)>> = Mutex::new(Vec::new());

/// Polices d'un dossier : (nom de valeur, nom de fichier), mis en cache
/// tant que le dossier ne change pas.
fn fonts_in(dir: &Path) -> FontList {
    let stamp = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
    let mut cache = FONT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, s, list)) = cache.iter().find(|(d, _, _)| d == dir) {
        if *s == stamp {
            return list.clone();
        }
    }
    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths.iter().filter(|p| p.is_file()) {
            let Some(file) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let faces = crate::dlls::gdi32_fonts::parse_font_file(path);
            if faces.is_empty() {
                continue;
            }
            let names: Vec<String> = faces
                .iter()
                .map(|f| {
                    if f.full_name.is_empty() {
                        f.family.clone()
                    } else {
                        f.full_name.clone()
                    }
                })
                .collect();
            // Collection .ttc : « A & B (TrueType) », comme Windows.
            list.push((
                font_value_name(&names.join(" & "), faces[0].cff),
                file.to_string(),
            ));
        }
    }
    cache.retain(|(d, _, _)| d != dir);
    cache.push((dir.to_path_buf(), stamp, list.clone()));
    list
}

pub fn fonts(data: &mut Data, key: &str, dir: &Path, absolute: bool) {
    let list = fonts_in(dir);
    if list.is_empty() && data.raw_key(key).is_none() {
        return;
    }
    data.raw_ensure(key);
    let known: Vec<String> = data
        .raw_values(key)
        .iter()
        .map(|v| super::string_of(&v.bytes).to_ascii_lowercase())
        .collect();
    for (name, file) in list {
        let value = if absolute {
            crate::paths::to_windows(&dir.join(&file))
        } else {
            file.clone()
        };
        let lower = value.to_ascii_lowercase();
        if known
            .iter()
            .any(|k| *k == lower || k.ends_with(&format!("\\{}", file.to_ascii_lowercase())))
        {
            continue;
        }
        if data.raw_get(key, &name).is_none() {
            text(data, key, &name, &value);
        }
    }
}

/// Applique toute la vue dynamique.
pub fn overlay(data: &mut Data) {
    time_zone(data, crate::intl::tz::current());
    let locale = crate::intl::locale::current();
    code_page(data, locale.ansi_cp, locale.oem_cp);
    keyboard(data, crate::intl::keyboard::current());
    fonts(
        data,
        FONTS,
        &crate::dlls::gdi32_fonts::windows_fonts_dir(),
        false,
    );
    let user = crate::paths::to_linux("C:\\AppData\\Local\\Microsoft\\Windows\\Fonts");
    fonts(data, USER_FONTS, &user, true);
}
