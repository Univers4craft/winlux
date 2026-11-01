//! Dossiers connus de Windows : SHGetKnownFolderPath, SHGetFolderPathA/W.
//!
//! `C:\` est le dossier personnel. Les dossiers de l'utilisateur (Bureau,
//! Documents, Images…) suivent les dossiers XDG du bureau Linux
//! (~/.config/user-dirs.dirs) ; les données d'application vont dans
//! `C:\AppData\…`, comme pour les installateurs.

use std::path::{Path, PathBuf};

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const HRESULT_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;
const KF_FLAG_CREATE: u32 = 0x8000;
const CSIDL_FLAG_CREATE: i32 = 0x8000;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Folder {
    Profile,
    Desktop,
    Documents,
    Pictures,
    Music,
    Videos,
    Downloads,
    Templates,
    RoamingAppData,
    LocalAppData,
    LocalAppDataLow,
    ProgramData,
    Windows,
    System,
    Fonts,
    ProgramFiles,
    ProgramFilesX86,
    ProgramFilesCommon,
    Public,
    StartMenu,
    Programs,
    Startup,
}

/// KNOWNFOLDERID publics (forme textuelle) → dossier.
const KNOWN: &[(&str, Folder)] = &[
    ("5E6C858F-0E22-4760-9AFE-EA3317B67173", Folder::Profile),
    ("B4BFCC3A-DB2C-424C-B029-7FE99A87C641", Folder::Desktop),
    ("FDD39AD0-238F-46AF-ADB4-6C85480369C7", Folder::Documents),
    ("33E28130-4E1E-4676-835A-98395C3BC3BB", Folder::Pictures),
    ("4BD8D571-6D19-48D3-BE97-422220080E43", Folder::Music),
    ("18989B1D-99B5-455B-841C-AB7C74E4DDFC", Folder::Videos),
    ("374DE290-123F-4565-9164-39C4925E467B", Folder::Downloads),
    ("A63293E8-664E-48DB-A079-DF759E0509F7", Folder::Templates),
    (
        "3EB685DB-65F9-4CF6-A03A-E3EF65729F3D",
        Folder::RoamingAppData,
    ),
    ("F1B32785-6FBA-4FCF-9D55-7B8E7F157091", Folder::LocalAppData),
    (
        "A520A1A4-1780-4FF6-BD18-167343C5AF16",
        Folder::LocalAppDataLow,
    ),
    ("62AB5D82-FDC1-4DC3-A9DD-070D1D495D97", Folder::ProgramData),
    ("F38BF404-1D43-42F2-9305-67DE0B28FC23", Folder::Windows),
    ("1AC14E77-02E7-4E5D-B744-2EB1AE5198B7", Folder::System),
    ("D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27", Folder::System), // SystemX86
    ("FD228CB7-AE11-4AE3-864C-16F3910AB8FE", Folder::Fonts),
    ("905E63B6-C1BF-494E-B29C-65B732D3D21A", Folder::ProgramFiles),
    ("6D809377-6AF0-444B-8957-A3773F02200E", Folder::ProgramFiles), // ProgramFilesX64
    (
        "7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E",
        Folder::ProgramFilesX86,
    ),
    (
        "F7F1ED05-9F6D-47A2-AAAE-29D317C6F066",
        Folder::ProgramFilesCommon,
    ),
    ("DFDF76A2-C82A-4D63-906A-5644AC457385", Folder::Public),
    ("625B53C3-AB48-4EC1-BA1F-A1EF4146FC19", Folder::StartMenu),
    ("A77F5D77-2E2B-44C3-A6A2-ABA601054A51", Folder::Programs),
    ("B97D20BB-F46A-4C97-BA10-5E3608430854", Folder::Startup),
];

/// CSIDL → dossier (SHGetFolderPath).
fn from_csidl(csidl: i32) -> Option<Folder> {
    Some(match csidl & 0xFF {
        0x00 | 0x10 => Folder::Desktop,
        0x02 => Folder::Programs,
        0x05 => Folder::Documents,
        0x07 => Folder::Startup,
        0x0B => Folder::StartMenu,
        0x0D => Folder::Music,
        0x0E => Folder::Videos,
        0x14 => Folder::Fonts,
        0x15 => Folder::Templates,
        0x1A => Folder::RoamingAppData,
        0x1C => Folder::LocalAppData,
        0x23 => Folder::ProgramData,
        0x24 => Folder::Windows,
        0x25 | 0x29 => Folder::System,
        0x26 => Folder::ProgramFiles,
        0x27 => Folder::Pictures,
        0x28 => Folder::Profile,
        0x2A => Folder::ProgramFilesX86,
        0x2B => Folder::ProgramFilesCommon,
        _ => return None,
    })
}

/// Dossier XDG (`XDG_DESKTOP_DIR`…) lu dans ~/.config/user-dirs.dirs.
fn xdg_dir(key: &str, home: &Path) -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let text = std::fs::read_to_string(config.join("user-dirs.dirs")).ok()?;
    parse_user_dirs(&text, key, home)
}

fn parse_user_dirs(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix(key)?.strip_prefix('=')?;
        let value = value.trim().trim_matches('"');
        let path = match value.strip_prefix("$HOME") {
            Some(rest) => home.join(rest.trim_start_matches('/')),
            None => PathBuf::from(value),
        };
        // « $HOME/ » seul signifie « pas de dossier dédié ».
        (path != home).then_some(path)
    })
}

/// Chemin Windows du dossier.
fn windows_path(folder: Folder) -> String {
    let home = crate::paths::drive_c();
    let user = |key: &str, fallback: &str| -> String {
        let p = xdg_dir(key, &home).unwrap_or_else(|| home.join(fallback));
        crate::paths::to_windows(&p)
    };
    match folder {
        Folder::Profile => "C:\\".into(),
        Folder::Desktop => user("XDG_DESKTOP_DIR", "Desktop"),
        Folder::Documents => user("XDG_DOCUMENTS_DIR", "Documents"),
        Folder::Pictures => user("XDG_PICTURES_DIR", "Pictures"),
        Folder::Music => user("XDG_MUSIC_DIR", "Music"),
        Folder::Videos => user("XDG_VIDEOS_DIR", "Videos"),
        Folder::Downloads => user("XDG_DOWNLOAD_DIR", "Downloads"),
        Folder::Templates => user("XDG_TEMPLATES_DIR", "Templates"),
        Folder::RoamingAppData => "C:\\AppData\\Roaming".into(),
        Folder::LocalAppData => "C:\\AppData\\Local".into(),
        Folder::LocalAppDataLow => "C:\\AppData\\LocalLow".into(),
        Folder::ProgramData => "C:\\ProgramData".into(),
        Folder::Windows => "C:\\Windows".into(),
        Folder::System => "C:\\Windows\\System32".into(),
        Folder::Fonts => "C:\\Windows\\Fonts".into(),
        Folder::ProgramFiles => "C:\\Program Files".into(),
        Folder::ProgramFilesX86 => "C:\\Program Files (x86)".into(),
        Folder::ProgramFilesCommon => "C:\\Program Files\\Common Files".into(),
        Folder::Public => "C:\\Public".into(),
        Folder::StartMenu => "C:\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu".into(),
        Folder::Programs => "C:\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs".into(),
        Folder::Startup => {
            "C:\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup".into()
        }
    }
}

/// Chemin du dossier, créé s'il le faut (KF_FLAG_CREATE, CSIDL_FLAG_CREATE).
fn resolve(folder: Folder, create: bool) -> String {
    let w = windows_path(folder);
    if create {
        let _ = std::fs::create_dir_all(crate::paths::to_linux(&w));
    }
    w
}

fn guid_text(g: &[u8; 16]) -> String {
    let d1 = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
    let d2 = u16::from_le_bytes([g[4], g[5]]);
    let d3 = u16::from_le_bytes([g[6], g[7]]);
    format!(
        "{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15]
    )
}

fn known(id: &[u8; 16]) -> Option<Folder> {
    let text = guid_text(id);
    KNOWN.iter().find(|(g, _)| *g == text).map(|(_, f)| *f)
}

/// `SHGetKnownFolderPath(rfid, flags, token, ppszPath)` : chaîne allouée
/// par CoTaskMemAlloc, à libérer par l'appelant.
pub unsafe extern "win64" fn sh_get_known_folder_path(
    id: *const [u8; 16],
    flags: u32,
    _token: usize,
    out: *mut *mut u16,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if id.is_null() {
        return E_INVALIDARG;
    }
    let Some(folder) = known(&*id) else {
        crate::trace!(
            "SHGetKnownFolderPath : dossier {{{}}} inconnu",
            guid_text(&*id)
        );
        return HRESULT_FILE_NOT_FOUND;
    };
    let path = resolve(folder, flags & KF_FLAG_CREATE != 0);
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let p = crate::dlls::ole32::co_task_mem_alloc(wide.len() * 2) as *mut u16;
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
    *out = p;
    S_OK
}

/// `SHGetFolderPathW(hwnd, csidl, token, flags, path[MAX_PATH])`.
pub unsafe extern "win64" fn sh_get_folder_path_w(
    _owner: usize,
    csidl: i32,
    _token: usize,
    _flags: u32,
    path: *mut u16,
) -> i32 {
    if path.is_null() {
        return E_INVALIDARG;
    }
    let Some(folder) = from_csidl(csidl) else {
        return E_INVALIDARG;
    };
    let s = resolve(folder, csidl & CSIDL_FLAG_CREATE != 0);
    crate::dlls::put_w(&s, path, 260);
    S_OK
}

pub unsafe extern "win64" fn sh_get_folder_path_a(
    _owner: usize,
    csidl: i32,
    _token: usize,
    _flags: u32,
    path: *mut u8,
) -> i32 {
    if path.is_null() {
        return E_INVALIDARG;
    }
    let Some(folder) = from_csidl(csidl) else {
        return E_INVALIDARG;
    };
    let s = resolve(folder, csidl & CSIDL_FLAG_CREATE != 0);
    let bytes = crate::dlls::encode_ansi(&s);
    let n = bytes.len().min(259);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), path, n);
    *path.add(n) = 0;
    S_OK
}

/// `SHGetSpecialFolderPathW(hwnd, path, csidl, create)` : BOOL.
pub unsafe extern "win64" fn sh_get_special_folder_path_w(
    owner: usize,
    path: *mut u16,
    csidl: i32,
    create: i32,
) -> i32 {
    let flag = if create != 0 { CSIDL_FLAG_CREATE } else { 0 };
    i32::from(sh_get_folder_path_w(owner, csidl | flag, 0, 0, path) == S_OK)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SHGetKnownFolderPath" => fn sh_get_known_folder_path,
        "SHGetFolderPathW" => fn sh_get_folder_path_w,
        "SHGetFolderPathA" => fn sh_get_folder_path_a,
        "SHGetSpecialFolderPathW" => fn sh_get_special_folder_path_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guid(text: &str) -> [u8; 16] {
        let h: String = text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let b = |i: usize| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap();
        let mut g = [0u8; 16];
        g[..4].copy_from_slice(&u32::from_str_radix(&h[..8], 16).unwrap().to_le_bytes());
        g[4..6].copy_from_slice(&u16::from_str_radix(&h[8..12], 16).unwrap().to_le_bytes());
        g[6..8].copy_from_slice(&u16::from_str_radix(&h[12..16], 16).unwrap().to_le_bytes());
        for (i, slot) in g.iter_mut().enumerate().skip(8) {
            *slot = b(i);
        }
        g
    }

    #[test]
    fn dossiers_connus_et_csidl() {
        unsafe {
            let mut out = std::ptr::null_mut();
            let id = guid("3EB685DB-65F9-4CF6-A03A-E3EF65729F3D");
            assert_eq!(sh_get_known_folder_path(&id, 0, 0, &mut out), S_OK);
            assert_eq!(crate::dlls::wstr(out), "C:\\AppData\\Roaming");
            crate::dlls::ole32::co_task_mem_free(out as *mut _);
            let unknown = [0x11u8; 16];
            assert_eq!(
                sh_get_known_folder_path(&unknown, 0, 0, &mut out),
                HRESULT_FILE_NOT_FOUND
            );
            assert!(out.is_null());
            let mut buf = [0u16; 260];
            assert_eq!(sh_get_folder_path_w(0, 0x1C, 0, 0, buf.as_mut_ptr()), S_OK);
            assert_eq!(crate::dlls::wstr(buf.as_ptr()), "C:\\AppData\\Local");
            assert_eq!(
                sh_get_folder_path_w(0, 0x7F, 0, 0, buf.as_mut_ptr()),
                E_INVALIDARG
            );
            let mut a = [0u8; 260];
            assert_eq!(sh_get_folder_path_a(0, 0x24, 0, 0, a.as_mut_ptr()), S_OK);
            assert_eq!(&a[..11], b"C:\\Windows\0");
        }
    }

    #[test]
    fn lit_les_dossiers_xdg() {
        let home = Path::new("/home/ana");
        let text = "# commentaire\nXDG_DESKTOP_DIR=\"$HOME/Bureau\"\nXDG_MUSIC_DIR=\"$HOME/\"\nXDG_DOCUMENTS_DIR=\"/data/docs\"\n";
        assert_eq!(
            parse_user_dirs(text, "XDG_DESKTOP_DIR", home),
            Some(PathBuf::from("/home/ana/Bureau"))
        );
        assert_eq!(
            parse_user_dirs(text, "XDG_DOCUMENTS_DIR", home),
            Some(PathBuf::from("/data/docs"))
        );
        assert_eq!(parse_user_dirs(text, "XDG_MUSIC_DIR", home), None);
        assert_eq!(parse_user_dirs(text, "XDG_VIDEOS_DIR", home), None);
    }
}
