//! Fonctions Win32 relevées sur FileZilla 3.71 (wxWidgets 3.2, MinGW) :
//! identifiants RPC, noms de domaine internationaux (IdnToAscii,
//! punycode RFC 3492, #784), petites fonctions de kernel32, msvcrt, shell32,
//! shlwapi, uxtheme, comctl32, advapi32 et netapi32.
//!
//! Écrit d'après la documentation publique de Microsoft et la RFC 3492
//! (salle blanche, voir AGENTS.md).

use crate::dlls::Exports;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001_u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

// ---------- rpcrt4 ----------

fn guid_text(g: &Guid) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

fn parse_guid(text: &str) -> Option<Guid> {
    let t = text.trim();
    let t = t
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .unwrap_or(t);
    let parts: Vec<&str> = t.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    if parts.len() != 5 || parts.iter().zip(lens).any(|(p, l)| p.len() != l) {
        return None;
    }
    let hex = |s: &str| u64::from_str_radix(s, 16).ok();
    let mut data4 = [0u8; 8];
    let tail = format!("{}{}", parts[3], parts[4]);
    for (i, b) in data4.iter_mut().enumerate() {
        *b = u8::from_str_radix(&tail[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(Guid {
        data1: hex(parts[0])? as u32,
        data2: hex(parts[1])? as u16,
        data3: hex(parts[2])? as u16,
        data4,
    })
}

/// `UuidToStringW` : chaîne allouée, libérée par `RpcStringFreeW`.
unsafe extern "win64" fn uuid_to_string_w(uuid: *const Guid, out: *mut *mut u16) -> u32 {
    if out.is_null() {
        return 87;
    }
    let g = if uuid.is_null() {
        Guid::default()
    } else {
        std::ptr::read_unaligned(uuid)
    };
    let mut wide: Vec<u16> = guid_text(&g).encode_utf16().collect();
    wide.push(0);
    let p = libc::malloc(wide.len() * 2) as *mut u16;
    if p.is_null() {
        return 14; // RPC_S_OUT_OF_MEMORY
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
    *out = p;
    0
}

unsafe extern "win64" fn uuid_to_string_a(uuid: *const Guid, out: *mut *mut u8) -> u32 {
    if out.is_null() {
        return 87;
    }
    let g = if uuid.is_null() {
        Guid::default()
    } else {
        std::ptr::read_unaligned(uuid)
    };
    let text = format!("{}\0", guid_text(&g));
    let p = libc::malloc(text.len()) as *mut u8;
    if p.is_null() {
        return 14;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
    *out = p;
    0
}

/// `UuidFromStringW` : NULL donne l'UUID nul ; RPC_S_INVALID_STRING_UUID
/// (1705) pour un texte mal formé.
unsafe extern "win64" fn uuid_from_string_w(text: *const u16, out: *mut Guid) -> u32 {
    if out.is_null() {
        return 87;
    }
    if text.is_null() {
        *out = Guid::default();
        return 0;
    }
    match parse_guid(&crate::dlls::wstr(text)) {
        Some(g) => {
            std::ptr::write_unaligned(out, g);
            0
        }
        None => 1705,
    }
}

unsafe extern "win64" fn uuid_from_string_a(text: *const u8, out: *mut Guid) -> u32 {
    if out.is_null() {
        return 87;
    }
    if text.is_null() {
        *out = Guid::default();
        return 0;
    }
    match parse_guid(&crate::dlls::cstr(text)) {
        Some(g) => {
            std::ptr::write_unaligned(out, g);
            0
        }
        None => 1705,
    }
}

unsafe extern "win64" fn rpc_string_free(text: *mut *mut c_void) -> u32 {
    if !text.is_null() && !(*text).is_null() {
        libc::free(*text);
        *text = std::ptr::null_mut();
    }
    0
}

// ---------- kernel32 : noms de domaine internationaux ----------

pub(crate) const IDN_BASE: u32 = 36;
pub(crate) const IDN_TMIN: u32 = 1;
pub(crate) const IDN_TMAX: u32 = 26;
const IDN_SKEW: u32 = 38;
const IDN_DAMP: u32 = 700;

pub(crate) fn idn_adapt(mut delta: u32, points: u32, first: bool) -> u32 {
    delta /= if first { IDN_DAMP } else { 2 };
    delta += delta / points;
    let mut k = 0;
    while delta > ((IDN_BASE - IDN_TMIN) * IDN_TMAX) / 2 {
        delta /= IDN_BASE - IDN_TMIN;
        k += IDN_BASE;
    }
    k + (IDN_BASE - IDN_TMIN + 1) * delta / (delta + IDN_SKEW)
}

fn idn_digit(d: u32) -> char {
    if d < 26 {
        (b'a' + d as u8) as char
    } else {
        (b'0' + (d - 26) as u8) as char
    }
}

/// Encodage punycode d'une étiquette (RFC 3492, section 6.3).
pub fn punycode(label: &str) -> Option<String> {
    let input: Vec<u32> = label.chars().map(|c| c as u32).collect();
    let mut out: String = label.chars().filter(|c| c.is_ascii()).collect();
    let basic = out.chars().count() as u32;
    let mut h = basic;
    if basic > 0 {
        out.push('-');
    }
    let (mut n, mut delta, mut bias) = (128u32, 0u32, 72u32);
    while (h as usize) < input.len() {
        let m = *input.iter().filter(|&&c| c >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(h + 1)?)?;
        n = m;
        for &c in &input {
            if c < n {
                delta = delta.checked_add(1)?;
            }
            if c == n {
                let mut q = delta;
                let mut k = IDN_BASE;
                loop {
                    let t = if k <= bias {
                        IDN_TMIN
                    } else if k >= bias + IDN_TMAX {
                        IDN_TMAX
                    } else {
                        k - bias
                    };
                    if q < t {
                        break;
                    }
                    out.push(idn_digit(t + (q - t) % (IDN_BASE - t)));
                    q = (q - t) / (IDN_BASE - t);
                    k += IDN_BASE;
                }
                out.push(idn_digit(q));
                bias = idn_adapt(delta, h + 1, h == basic);
                delta = 0;
                h += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    Some(out)
}

/// Nom de domaine en ASCII : chaque étiquette non ASCII devient « xn--… ».
pub fn idn_to_ascii(name: &str) -> Option<String> {
    let labels: Option<Vec<String>> = name
        .split(['.', '\u{3002}', '\u{FF0E}', '\u{FF61}'])
        .map(|label| {
            let lower = label.to_lowercase();
            if lower.is_ascii() {
                Some(lower)
            } else {
                punycode(&lower).map(|p| format!("xn--{p}"))
            }
        })
        .collect();
    labels.map(|l| l.join("."))
}

/// `IdnToAscii(dwFlags, lpUnicodeCharStr, cchUnicodeChar, lpASCIICharStr,
/// cchASCIIChar)` : nombre de caractères écrits (ou nécessaires).
pub(crate) unsafe extern "win64" fn idn_to_ascii_w(
    _flags: u32,
    input: *const u16,
    count: i32,
    out: *mut u16,
    out_len: i32,
) -> i32 {
    if input.is_null() || count == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let (units, terminated) = if count < 0 {
        (crate::printf::wide_len(input), true)
    } else {
        (count as usize, false)
    };
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(input, units));
    let Some(ascii) = idn_to_ascii(&text) else {
        crate::process::set_last_error(123); // ERROR_INVALID_NAME
        return 0;
    };
    let mut wide: Vec<u16> = ascii.encode_utf16().collect();
    if terminated {
        wide.push(0);
    }
    if out_len == 0 {
        return wide.len() as i32;
    }
    if out.is_null() || (out_len as usize) < wide.len() {
        crate::process::set_last_error(122); // ERROR_INSUFFICIENT_BUFFER
        return 0;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), out, wide.len());
    wide.len() as i32
}

/// `GlobalHandle` : le handle d'un bloc à partir de son pointeur.
extern "win64" fn global_handle(mem: usize) -> usize {
    if mem == 0 {
        crate::process::set_last_error(6);
        return 0;
    }
    // Bloc GMEM_FIXED : le pointeur est le handle.
    if crate::dlls::kernel32::lookup_global_memory(mem).is_some() {
        return mem;
    }
    crate::dlls::kernel32::global_handle_of_pointer(mem).unwrap_or_else(|| {
        crate::process::set_last_error(6);
        0
    })
}

/// Pas de tampon d'écran de console lisible : échec documenté.
unsafe extern "win64" fn read_console_output_character_a(
    _console: usize,
    _buf: *mut u8,
    _len: u32,
    _coord: u32,
    read: *mut u32,
) -> i32 {
    if !read.is_null() {
        *read = 0;
    }
    crate::process::set_last_error(6);
    0
}

// ---------- msvcrt ----------

unsafe extern "win64" fn imp_fwprintf(args: *const u64) -> i32 {
    let stream = *args as *mut c_void;
    let fmt = *args.add(1) as *const u16;
    crate::dlls::crt_ucrt::stdio_common_vfwprintf(0, stream, fmt, 0, args.add(2))
}
crate::varargs_thunk!(fwprintf_thunk => imp_fwprintf);

/// Lecteur courant : toujours C: (3) chez Winlux.
extern "win64" fn getdrive() -> i32 {
    3
}

/// `_chdrive` : seul C: (3) existe comme lecteur courant.
extern "win64" fn chdrive(drive: i32) -> i32 {
    if drive == 3 {
        0
    } else {
        -1
    }
}

// ---------- shell32 ----------

/// `ExtractIconW` : pas d'icône extraite (NULL), comme pour un fichier
/// sans icône ; `nIconIndex == -1` demande le nombre d'icônes : 0.
extern "win64" fn extract_icon_w(_inst: usize, _file: *const u16, _index: u32) -> usize {
    0
}

/// `SHDefExtractIconW` : icônes standard de shell32.dll ou imageres.dll
/// (l'emplacement que rend SHGetStockIconInfo, #1169), prises dans la
/// liste système à la taille demandée (LOWORD grande, HIWORD petite) ;
/// S_FALSE et des sorties à NULL pour tout autre fichier.
pub(crate) unsafe extern "win64" fn sh_def_extract_icon_w(
    file: *const u16,
    index: i32,
    _flags: u32,
    large: *mut usize,
    small: *mut usize,
    size: u32,
) -> i32 {
    // HICON* de la taille d'un pointeur du programme (4 octets en x86).
    let put = |out: *mut usize, v: usize| {
        if !out.is_null() {
            crate::wow32::write_ptr(out as usize, v);
        }
    };
    put(large, 0);
    put(small, 0);
    if file.is_null() || !(0..7).contains(&index) {
        return 1; // S_FALSE
    }
    let name = crate::dlls::wstr(file).to_ascii_lowercase();
    let base = name.rsplit(['\\', '/']).next().unwrap_or("");
    if base != "shell32.dll" && base != "imageres.dll" {
        return 1;
    }
    let icon = |px: u32| {
        let px = match px {
            0 => 32,
            p => p.clamp(16, 256) as usize,
        };
        crate::dlls::icons::list_image(crate::dlls::icons::system_list_px(px), index)
            .map_or(0, crate::dlls::icons::new_icon)
    };
    if !large.is_null() {
        put(large, icon(size & 0xFFFF));
    }
    if !small.is_null() {
        put(small, icon((size >> 16).max(16)));
    }
    0 // S_OK
}

/// `SHGetIconOverlayIndexW` : aucune surimpression (-1).
extern "win64" fn sh_get_icon_overlay_index_w(_path: *const u16, _index: i32) -> i32 {
    -1
}

// ---------- shlwapi ----------

/// `SHAutoComplete` : la saisie reste un champ ordinaire.
extern "win64" fn sh_auto_complete(_hwnd: usize, _flags: u32) -> i32 {
    S_OK
}

// ---------- uxtheme (fenêtres Win32 non thémées : rien n'est défini) ----------

unsafe extern "win64" fn get_theme_background_extent(
    _theme: usize,
    _hdc: usize,
    _part: i32,
    _state: i32,
    content: *const [i32; 4],
    extent: *mut [i32; 4],
) -> i32 {
    if content.is_null() || extent.is_null() {
        return E_INVALIDARG;
    }
    *extent = *content;
    S_OK
}

unsafe extern "win64" fn get_theme_int(
    _theme: usize,
    _part: i32,
    _state: i32,
    _prop: i32,
    value: *mut i32,
) -> i32 {
    if !value.is_null() {
        *value = 0;
    }
    E_NOTIMPL
}

unsafe extern "win64" fn get_theme_margins(
    _theme: usize,
    _hdc: usize,
    _part: i32,
    _state: i32,
    _prop: i32,
    _rect: *const [i32; 4],
    margins: *mut [i32; 4],
) -> i32 {
    if !margins.is_null() {
        *margins = [0; 4];
    }
    E_NOTIMPL
}

// ---------- comctl32 ----------

extern "win64" fn image_list_set_drag_cursor_image(
    _list: usize,
    _index: i32,
    _x: i32,
    _y: i32,
) -> i32 {
    1
}

// ---------- advapi32 ----------

/// `AllocateLocallyUniqueId` : un LUID différent à chaque appel.
unsafe extern "win64" fn allocate_locally_unique_id(luid: *mut u64) -> i32 {
    static NEXT: AtomicU64 = AtomicU64::new(0x1_0000);
    if luid.is_null() {
        crate::process::set_last_error(998);
        return 0;
    }
    std::ptr::write_unaligned(luid, NEXT.fetch_add(1, Ordering::Relaxed));
    1
}

/// `LogonUserExW` : pas d'ouverture de session Windows (comptes Linux) :
/// ERROR_LOGON_FAILURE.
unsafe extern "win64" fn logon_user_ex_w(
    _user: *const u16,
    _domain: *const u16,
    _password: *const u16,
    _logon_type: u32,
    _provider: u32,
    token: *mut usize,
    _sid: usize,
    _profile: usize,
    _len: usize,
    _limits: usize,
) -> i32 {
    if !token.is_null() {
        *token = 0;
    }
    crate::process::set_last_error(1326);
    0
}

// ---------- netapi32 ----------

/// `NetShareEnum` : pas de partages Windows à énumérer (ERROR_NOT_SUPPORTED).
unsafe extern "win64" fn net_share_enum(
    _server: *const u16,
    _level: u32,
    buf: *mut usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
    _resume: *mut u32,
) -> u32 {
    if !buf.is_null() {
        *buf = 0;
    }
    if !read.is_null() {
        *read = 0;
    }
    if !total.is_null() {
        *total = 0;
    }
    50
}

pub fn all() -> Vec<(&'static str, Exports)> {
    vec![
        (
            "rpcrt4",
            crate::exports! {
                "UuidToStringW" => fn uuid_to_string_w,
                "UuidToStringA" => fn uuid_to_string_a,
                "UuidFromStringW" => fn uuid_from_string_w,
                "UuidFromStringA" => fn uuid_from_string_a,
                "RpcStringFreeW" => fn rpc_string_free,
                "RpcStringFreeA" => fn rpc_string_free,
            },
        ),
        (
            "kernel32",
            crate::exports! {
                "IdnToAscii" => fn idn_to_ascii_w,
                "GlobalHandle" => fn global_handle,
                "ReadConsoleOutputCharacterA" => fn read_console_output_character_a,
            },
        ),
        (
            "msvcrt",
            crate::exports! {
                "fwprintf" => fn fwprintf_thunk,
                "_getdrive" => fn getdrive,
                "_chdrive" => fn chdrive,
            },
        ),
        (
            "ucrtbase",
            crate::exports! {
                "_getdrive" => fn getdrive,
                "_chdrive" => fn chdrive,
            },
        ),
        (
            "shell32",
            crate::exports! {
                "ExtractIconW" => fn extract_icon_w,
                "SHDefExtractIconW" => fn sh_def_extract_icon_w,
                // Ordinal 6 : SHDefExtractIconW, importé ainsi par
                // wxmsw313u_core (fermeture d'une fenêtre de projet
                // d'Audacity, #1284).
                "#6" => fn sh_def_extract_icon_w,
                "SHGetIconOverlayIndexW" => fn sh_get_icon_overlay_index_w,
            },
        ),
        (
            "shlwapi",
            crate::exports! { "SHAutoComplete" => fn sh_auto_complete },
        ),
        (
            "uxtheme",
            crate::exports! {
                "GetThemeBackgroundExtent" => fn get_theme_background_extent,
                "GetThemeInt" => fn get_theme_int,
                "GetThemeMargins" => fn get_theme_margins,
            },
        ),
        (
            "comctl32",
            crate::exports! { "ImageList_SetDragCursorImage" => fn image_list_set_drag_cursor_image },
        ),
        (
            "advapi32",
            crate::exports! {
                "AllocateLocallyUniqueId" => fn allocate_locally_unique_id,
                "LogonUserExW" => fn logon_user_ex_w,
            },
        ),
        (
            "netapi32",
            crate::exports! { "NetShareEnum" => fn net_share_enum },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punycode_exemples_de_la_rfc_3492() {
        // RFC 3492, 7.1 (L) : « 3年B組金八先生 ».
        assert_eq!(
            punycode("3年b組金八先生").as_deref(),
            Some("3b-ww4c5e180e575a65lsy2b")
        );
        assert_eq!(punycode("bücher").as_deref(), Some("bcher-kva"));
        assert_eq!(
            idn_to_ascii("Bücher.Example").as_deref(),
            Some("xn--bcher-kva.example")
        );
        assert_eq!(
            idn_to_ascii("filezilla-project.org").as_deref(),
            Some("filezilla-project.org")
        );
    }

    #[test]
    fn uuid_texte_aller_retour() {
        let g = parse_guid("{6B29FC40-CA47-1067-B31D-00DD010662DA}").unwrap();
        assert_eq!(guid_text(&g), "6b29fc40-ca47-1067-b31d-00dd010662da");
        assert!(parse_guid("6b29fc40-ca47-1067-b31d").is_none());
    }

    /// #1284 : wxmsw313u_core importe SHDefExtractIconW par son ordinal 6.
    #[test]
    fn shell32_ordinal_6_est_sh_def_extract_icon_w() {
        let par_nom = crate::dlls::lookup("shell32", "SHDefExtractIconW");
        assert!(par_nom.is_some());
        assert_eq!(crate::dlls::lookup("shell32", "#6"), par_nom);
    }
}
