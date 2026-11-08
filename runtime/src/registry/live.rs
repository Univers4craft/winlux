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
    add_fonts(data, key, dir, absolute, list);
}

fn add_fonts(data: &mut Data, key: &str, dir: &Path, absolute: bool, list: FontList) {
    // La clé existe toujours sous Windows, même sans police (#1045).
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

/// Polices du système Linux (`/usr/share/fonts`…), visibles des .exe comme
/// les polices ajoutées hors de `C:\Windows\Fonts` : chemin complet en
/// valeur. Seuls les noms de fichiers sont lus (pas les fichiers eux-mêmes),
/// car la vue est recalculée à chaque chargement du registre.
fn system_fonts() -> Vec<(PathBuf, FontList)> {
    static CACHE: Mutex<Option<(Vec<Option<Clock>>, Vec<(PathBuf, FontList)>)>> = Mutex::new(None);
    let windows = crate::dlls::gdi32_fonts::windows_font_dirs();
    let roots: Vec<PathBuf> = crate::dlls::gdi32_fonts::font_dirs()
        .into_iter()
        .filter(|d| !windows.contains(d))
        .collect();
    let mut dirs = Vec::new();
    for root in &roots {
        collect_dirs(root, 0, &mut dirs);
    }
    let stamps: Vec<Option<Clock>> = dirs
        .iter()
        .map(|d| std::fs::metadata(d).and_then(|m| m.modified()).ok())
        .collect();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, list)) = cache.as_ref() {
        if *s == stamps {
            return list.clone();
        }
    }
    let mut out = Vec::new();
    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                l.ends_with(".ttf") || l.ends_with(".otf") || l.ends_with(".ttc")
            })
            .collect();
        files.sort();
        if files.is_empty() {
            continue;
        }
        let list = files
            .into_iter()
            .map(|f| {
                let lower = f.to_ascii_lowercase();
                let stem = f.rsplit_once('.').map_or(f.as_str(), |(s, _)| s);
                let shown = stem.replace(['-', '_'], " ");
                (font_value_name(&shown, lower.ends_with(".otf")), f)
            })
            .collect();
        out.push((dir.clone(), list));
    }
    *cache = Some((stamps, out.clone()));
    out
}

fn collect_dirs(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if depth > 6 || !dir.is_dir() || out.contains(&dir.to_path_buf()) {
        return;
    }
    out.push(dir.to_path_buf());
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut subs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        subs.sort();
        for sub in subs {
            collect_dirs(&sub, depth + 1, out);
        }
    }
}

/// Clés calculées : leurs valeurs ne sont jamais enregistrées sur le disque
/// (#1046), seulement recalculées au chargement.
pub const MANAGED: [&str; 5] = [TIME_ZONE, CODE_PAGE, KEYBOARD, FONTS, USER_FONTS];

/// Valeurs posées par la vue dynamique : (clé, nom, contenu).
pub type LiveValues = Vec<(String, String, Vec<u8>)>;

/// Applique toute la vue dynamique et rend les valeurs qu'elle a posées ou
/// changées, pour qu'elles ne soient pas écrites dans `registry.tsv`.
pub fn overlay(data: &mut Data) -> LiveValues {
    let before: Vec<(String, Vec<super::Value>)> = MANAGED
        .iter()
        .map(|k| (k.to_string(), data.raw_values(k)))
        .collect();
    apply(data);
    let mut live = Vec::new();
    for (key, old) in before {
        for v in data.raw_values(&key) {
            if !old.contains(&v) {
                live.push((key.clone(), v.name.clone(), v.bytes.clone()));
            }
        }
    }
    live
}

/// Retire d'une copie à enregistrer les valeurs calculées restées telles
/// quelles (une valeur changée par une appli, elle, est gardée).
pub fn strip(data: &mut Data, live: &LiveValues) {
    for (key, name, bytes) in live {
        let same = data.raw_get(key, name).is_some_and(|v| &v.bytes == bytes);
        if same {
            if let Some(k) = data.raw_key_mut(key) {
                k.values.remove(&name.to_lowercase());
            }
        }
    }
}

fn apply(data: &mut Data) {
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
    for (dir, list) in system_fonts() {
        add_fonts(data, FONTS, &dir, true, list);
    }
}
