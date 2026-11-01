//! Dossiers spéciaux de Windows (`SHGetFolderPath`, `SHGetKnownFolderPath`,
//! `GetUserProfileDirectory`…) reliés aux vrais dossiers de l'utilisateur
//! Linux : « Documents » est le dossier Documents XDG, « Bureau » le bureau
//! XDG, AppData vit dans le dossier personnel. Les chemins rendus sont des
//! chemins Windows (`C:\Users\<nom>\…`), que `paths::to_linux` sait relire.
#![allow(non_snake_case)]

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
/// HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND) : dossier connu inconnu.
const E_NOTFOUND: i32 = 0x8007_0002u32 as i32;
const CSIDL_FLAG_CREATE: i32 = 0x8000;
const KF_FLAG_CREATE: u32 = 0x8000;

const START_MENU: &str = "AppData\\Roaming\\Microsoft\\Windows\\Start Menu";
const COMMON_START_MENU: &str = "C:\\ProgramData\\Microsoft\\Windows\\Start Menu";

/// Où se trouve un dossier spécial.
enum Place {
    /// Relatif au profil (`C:\Users\<nom>\…`).
    Profile(&'static str),
    /// Chemin Windows fixe.
    Fixed(&'static str),
    /// Menu Démarrer de l'utilisateur, avec un sous-dossier.
    StartMenu(&'static str),
    CommonStartMenu(&'static str),
}

fn csidl_place(csidl: i32) -> Option<Place> {
    use Place::*;
    Some(match csidl & 0xff {
        0x00 | 0x10 => Profile("Desktop"),
        0x02 => StartMenu("Programs"),
        0x05 => Profile("Documents"),
        0x06 => Profile("Favorites"),
        0x07 => StartMenu("Programs\\Startup"),
        0x08 => Profile("AppData\\Roaming\\Microsoft\\Windows\\Recent"),
        0x09 => Profile("AppData\\Roaming\\Microsoft\\Windows\\SendTo"),
        0x0b => StartMenu(""),
        0x0d => Profile("Music"),
        0x0e => Profile("Videos"),
        0x14 => Fixed("C:\\Windows\\Fonts"),
        0x15 => Profile("AppData\\Roaming\\Microsoft\\Windows\\Templates"),
        0x16 => CommonStartMenu(""),
        0x17 => CommonStartMenu("Programs"),
        0x18 => CommonStartMenu("Programs\\Startup"),
        0x19 => Fixed("C:\\Users\\Public\\Desktop"),
        0x1a => Profile("AppData\\Roaming"),
        0x1c => Profile("AppData\\Local"),
        0x20 => Profile("AppData\\Local\\Microsoft\\Windows\\INetCache"),
        0x21 => Profile("AppData\\Local\\Microsoft\\Windows\\INetCookies"),
        0x22 => Profile("AppData\\Local\\Microsoft\\Windows\\History"),
        0x23 => Fixed("C:\\ProgramData"),
        0x24 => Fixed("C:\\Windows"),
        0x25 => Fixed("C:\\Windows\\System32"),
        0x26 => Fixed("C:\\Program Files"),
        0x27 => Profile("Pictures"),
        0x28 => Profile(""),
        0x29 => Fixed("C:\\Windows\\SysWOW64"),
        0x2a => Fixed("C:\\Program Files (x86)"),
        0x2b => Fixed("C:\\Program Files\\Common Files"),
        0x2c => Fixed("C:\\Program Files (x86)\\Common Files"),
        0x2e => Fixed("C:\\Users\\Public\\Documents"),
        0x2f => CommonStartMenu("Programs\\Administrative Tools"),
        0x35 => Fixed("C:\\Users\\Public\\Music"),
        0x36 => Fixed("C:\\Users\\Public\\Pictures"),
        0x37 => Fixed("C:\\Users\\Public\\Videos"),
        _ => return None,
    })
}

/// FOLDERID_* les plus utilisés → emplacement.
const KNOWN_FOLDERS: &[(&str, Place)] = &[
    (
        "B4BFCC3A-DB2C-424C-B029-7FE99A87C641",
        Place::Profile("Desktop"),
    ),
    (
        "FDD39AD0-238F-46AF-ADB4-6C85480369C7",
        Place::Profile("Documents"),
    ),
    (
        "374DE290-123F-4565-9164-39C4925E467B",
        Place::Profile("Downloads"),
    ),
    (
        "4BD8D571-6D19-48D3-BE97-422220080E43",
        Place::Profile("Music"),
    ),
    (
        "33E28130-4E1E-4676-835A-98395C3BC3BB",
        Place::Profile("Pictures"),
    ),
    (
        "18989B1D-99B5-455B-841C-AB7C74E4DDFC",
        Place::Profile("Videos"),
    ),
    (
        "3EB685DB-65F9-4CF6-A03A-E3EF65729F3D",
        Place::Profile("AppData\\Roaming"),
    ),
    (
        "F1B32785-6FBA-4FCF-9D55-7B8E7F157091",
        Place::Profile("AppData\\Local"),
    ),
    (
        "A520A1A4-1780-4FF6-BD18-167343C5AF16",
        Place::Profile("AppData\\LocalLow"),
    ),
    ("5E6C858F-0E22-4760-9AFE-EA3317B67173", Place::Profile("")),
    (
        "4C5C32FF-BB9D-43B0-B5B4-2D72E54EAAA4",
        Place::Profile("Saved Games"),
    ),
    (
        "1777F761-68AD-4D8A-87BD-30B759FA33DD",
        Place::Profile("Favorites"),
    ),
    (
        "62AB5D82-FDC1-4DC3-A9DD-070D1D495D97",
        Place::Fixed("C:\\ProgramData"),
    ),
    (
        "905E63B6-C1BF-494E-B29C-65B732D3D21A",
        Place::Fixed("C:\\Program Files"),
    ),
    (
        "6D809377-6AF0-444B-8957-A3773F02200E",
        Place::Fixed("C:\\Program Files"),
    ),
    (
        "7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E",
        Place::Fixed("C:\\Program Files (x86)"),
    ),
    (
        "F38BF404-1D43-42F2-9305-67DE0B28FC23",
        Place::Fixed("C:\\Windows"),
    ),
    (
        "1AC14E77-02E7-4E5D-B744-2EB1AE5198B7",
        Place::Fixed("C:\\Windows\\System32"),
    ),
    (
        "FD228CB7-AE11-4AE3-864C-16F3910AB8B0",
        Place::Fixed("C:\\Windows\\Fonts"),
    ),
    (
        "DFDF76A2-C82A-4D63-906A-5644AC457385",
        Place::Fixed("C:\\Users\\Public"),
    ),
    (
        "ED4824AF-DCE4-45A8-81E2-FC7965083634",
        Place::Fixed("C:\\Users\\Public\\Documents"),
    ),
    (
        "0762D272-C50A-4BB0-A382-697DCD729B80",
        Place::Fixed("C:\\Users"),
    ),
    ("625B53C3-AB48-4EC1-BA1F-A1EF4146FC19", Place::StartMenu("")),
    (
        "A77F5D77-2E2B-44C3-A6A2-ABA601054A51",
        Place::StartMenu("Programs"),
    ),
    (
        "B97D20BB-F46A-4C97-BA10-5E3608430854",
        Place::StartMenu("Programs\\Startup"),
    ),
    (
        "A4115719-D62E-491D-AA7C-E74B8BE3B067",
        Place::CommonStartMenu(""),
    ),
    (
        "0139D44E-6AFE-49F2-8690-3DAFCAE6FFB8",
        Place::CommonStartMenu("Programs"),
    ),
];

fn place_path(place: &Place) -> String {
    let join = |base: String, sub: &str| {
        if sub.is_empty() {
            base
        } else {
            format!("{base}\\{sub}")
        }
    };
    match place {
        Place::Profile(rel) => crate::paths::profile_path(rel),
        Place::Fixed(p) => (*p).to_string(),
        Place::StartMenu(sub) => join(crate::paths::profile_path(START_MENU), sub),
        Place::CommonStartMenu(sub) => join(COMMON_START_MENU.to_string(), sub),
    }
}

/// Les dossiers du profil existent toujours sous Windows : on les crée à la
/// demande. Les dossiers « système » seulement si l'appli le demande.
fn ensure(place: &Place, path: &str, create: bool) {
    if create || matches!(place, Place::Profile(_) | Place::StartMenu(_)) {
        let _ = std::fs::create_dir_all(crate::paths::to_linux(path));
    }
}

/// Chemin Windows d'un CSIDL (`None` si inconnu).
pub fn csidl_path(csidl: i32) -> Option<String> {
    let place = csidl_place(csidl)?;
    let path = place_path(&place);
    ensure(&place, &path, csidl & CSIDL_FLAG_CREATE != 0);
    Some(path)
}

fn guid_text(p: *const u8) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY: un GUID fait 16 octets ; le pointeur vient de l'appli.
    let b = unsafe { std::slice::from_raw_parts(p, 16) };
    let d1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let d2 = u16::from_le_bytes([b[4], b[5]]);
    let d3 = u16::from_le_bytes([b[6], b[7]]);
    Some(format!(
        "{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

/// Chemin Windows d'un FOLDERID (texte `XXXXXXXX-XXXX-…`).
pub fn known_folder_path(guid: &str, flags: u32) -> Option<String> {
    let (_, place) = KNOWN_FOLDERS
        .iter()
        .find(|(g, _)| g.eq_ignore_ascii_case(guid))?;
    let path = place_path(place);
    ensure(place, &path, flags & KF_FLAG_CREATE != 0);
    Some(path)
}

unsafe fn write_ansi(s: &str, buf: *mut u8, cap: usize) -> bool {
    let b = crate::dlls::encode_ansi(s);
    if buf.is_null() || b.len() >= cap {
        return false;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
    *buf.add(b.len()) = 0;
    true
}

unsafe fn write_wide(s: &str, buf: *mut u16, cap: usize) -> bool {
    let w: Vec<u16> = s.encode_utf16().collect();
    if buf.is_null() || w.len() >= cap {
        return false;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    true
}

const MAX_PATH: usize = 260;

pub unsafe extern "win64" fn sh_get_folder_path_a(
    _owner: usize,
    csidl: i32,
    _token: usize,
    _flags: u32,
    path: *mut u8,
) -> i32 {
    match csidl_path(csidl) {
        Some(p) if write_ansi(&p, path, MAX_PATH) => S_OK,
        Some(_) => E_INVALIDARG,
        None => E_INVALIDARG,
    }
}

pub unsafe extern "win64" fn sh_get_folder_path_w(
    _owner: usize,
    csidl: i32,
    _token: usize,
    _flags: u32,
    path: *mut u16,
) -> i32 {
    match csidl_path(csidl) {
        Some(p) if write_wide(&p, path, MAX_PATH) => S_OK,
        _ => E_INVALIDARG,
    }
}

pub unsafe extern "win64" fn sh_get_special_folder_path_a(
    _owner: usize,
    path: *mut u8,
    csidl: i32,
    create: i32,
) -> i32 {
    let flag = if create != 0 { CSIDL_FLAG_CREATE } else { 0 };
    csidl_path(csidl | flag).is_some_and(|p| write_ansi(&p, path, MAX_PATH)) as i32
}

pub unsafe extern "win64" fn sh_get_special_folder_path_w(
    _owner: usize,
    path: *mut u16,
    csidl: i32,
    create: i32,
) -> i32 {
    let flag = if create != 0 { CSIDL_FLAG_CREATE } else { 0 };
    csidl_path(csidl | flag).is_some_and(|p| write_wide(&p, path, MAX_PATH)) as i32
}

pub unsafe extern "win64" fn sh_get_known_folder_path(
    id: *const u8,
    flags: u32,
    _token: usize,
    out: *mut *mut u16,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let Some(guid) = guid_text(id) else {
        return E_INVALIDARG;
    };
    let Some(path) = known_folder_path(&guid, flags) else {
        return E_NOTFOUND;
    };
    let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
    let mem = crate::dlls::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
    if mem.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), mem, w.len());
    *out = mem;
    S_OK
}

/// `GetUserProfileDirectory` : taille en caractères, terminateur compris.
unsafe fn profile_out<T: Copy>(text: Vec<T>, zero: T, buf: *mut T, size: *mut u32) -> i32 {
    if size.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut v = text;
    v.push(zero);
    if buf.is_null() || (*size as usize) < v.len() {
        *size = v.len() as u32;
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), buf, v.len());
    *size = v.len() as u32;
    1
}

pub unsafe extern "win64" fn GetUserProfileDirectoryW(
    _token: usize,
    buf: *mut u16,
    size: *mut u32,
) -> i32 {
    let p = crate::paths::profile_windows();
    profile_out(p.encode_utf16().collect(), 0u16, buf, size)
}

pub unsafe extern "win64" fn GetUserProfileDirectoryA(
    _token: usize,
    buf: *mut u8,
    size: *mut u32,
) -> i32 {
    let p = crate::paths::profile_windows();
    profile_out(crate::dlls::encode_ansi(&p), 0u8, buf, size)
}

pub unsafe extern "win64" fn GetProfilesDirectoryW(buf: *mut u16, size: *mut u32) -> i32 {
    profile_out("C:\\Users".encode_utf16().collect(), 0u16, buf, size)
}

pub unsafe extern "win64" fn GetProfilesDirectoryA(buf: *mut u8, size: *mut u32) -> i32 {
    profile_out(b"C:\\Users".to_vec(), 0u8, buf, size)
}

pub fn shell32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SHGetFolderPathA" => fn sh_get_folder_path_a,
        "SHGetFolderPathW" => fn sh_get_folder_path_w,
        "SHGetSpecialFolderPathA" => fn sh_get_special_folder_path_a,
        "SHGetSpecialFolderPathW" => fn sh_get_special_folder_path_w,
        "SHGetKnownFolderPath" => fn sh_get_known_folder_path,
    }
}

pub fn userenv_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetUserProfileDirectoryW" => fn GetUserProfileDirectoryW,
        "GetUserProfileDirectoryA" => fn GetUserProfileDirectoryA,
        "GetProfilesDirectoryW" => fn GetProfilesDirectoryW,
        "GetProfilesDirectoryA" => fn GetProfilesDirectoryA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossiers_speciaux() {
        let _g = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("winlux-kf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".config")).unwrap();
        std::fs::write(
            root.join(".config/user-dirs.dirs"),
            "XDG_DOCUMENTS_DIR=\"$HOME/Mes documents\"\n",
        )
        .unwrap();
        let saved: Vec<_> = ["WINLUX_HOME", "WINLUX_DRIVE_C", "XDG_CONFIG_HOME"]
            .iter()
            .map(|k| (*k, std::env::var_os(k)))
            .collect();
        std::env::set_var("WINLUX_HOME", &root);
        std::env::set_var("WINLUX_DRIVE_C", &root);
        std::env::set_var("XDG_CONFIG_HOME", root.join(".config"));

        let profile = crate::paths::profile_windows();
        let docs = csidl_path(0x05).unwrap();
        assert_eq!(docs, format!("{profile}\\Documents"));
        assert_eq!(crate::paths::to_linux(&docs), root.join("Mes documents"));
        assert!(root.join("Mes documents").is_dir(), "créé à la demande");
        assert_eq!(
            csidl_path(0x1a | CSIDL_FLAG_CREATE).unwrap(),
            format!("{profile}\\AppData\\Roaming")
        );
        assert!(root.join("AppData/Roaming").is_dir());
        assert_eq!(csidl_path(0x26).unwrap(), "C:\\Program Files");
        assert!(
            !root.join("Program Files").exists(),
            "pas créé sans CSIDL_FLAG_CREATE"
        );
        assert!(csidl_path(0x7f).is_none());

        // FOLDERID_Downloads en mémoire, comme le passe une appli.
        let guid: [u8; 16] = [
            0x90, 0xE2, 0x4D, 0x37, 0x3F, 0x12, 0x65, 0x45, 0x91, 0x64, 0x39, 0xC4, 0x92, 0x5E,
            0x46, 0x7B,
        ];
        let mut out: *mut u16 = std::ptr::null_mut();
        let hr = unsafe { sh_get_known_folder_path(guid.as_ptr(), 0, 0, &mut out) };
        assert_eq!(hr, S_OK);
        let text = unsafe { crate::dlls::wstr(out) };
        unsafe { crate::dlls::ole32::co_task_mem_free(out.cast()) };
        assert_eq!(text, format!("{profile}\\Downloads"));

        let mut buf = [0u16; 8];
        let mut size = buf.len() as u32;
        assert_eq!(
            unsafe { GetUserProfileDirectoryW(0, buf.as_mut_ptr(), &mut size) },
            0
        );
        assert_eq!(size as usize, profile.encode_utf16().count() + 1);
        let mut big = vec![0u16; size as usize];
        assert_eq!(
            unsafe { GetUserProfileDirectoryW(0, big.as_mut_ptr(), &mut size) },
            1
        );
        assert_eq!(String::from_utf16_lossy(&big[..big.len() - 1]), profile);

        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
