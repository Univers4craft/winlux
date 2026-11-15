//! Dossiers spéciaux de Windows (`SHGetFolderPath`, `SHGetKnownFolderPath`,
//! `GetUserProfileDirectory`…) reliés aux vrais dossiers de l'utilisateur
//! Linux : « Documents » est le dossier Documents XDG, « Bureau » le bureau
//! XDG, AppData vit dans le dossier personnel. Les chemins rendus sont des
//! chemins Windows (`C:\Users\<nom>\…`), que `paths::to_linux` sait relire.
#![allow(non_snake_case)]

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
/// Dossier connu sans chemin (KF_CATEGORY_VIRTUAL), documenté pour
/// SHGetKnownFolderPath.
const E_FAIL: i32 = 0x8000_4005u32 as i32;
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
    /// `Program Files` vu par l'appelant, avec un sous-dossier : un
    /// programme 32 bits reçoit `Program Files (x86)`, comme sous WOW64
    /// (CSIDL_PROGRAM_FILES, CSIDL_PROGRAM_FILES_COMMON, FOLDERID_ProgramFiles,
    /// FOLDERID_ProgramFilesCommon) (#2002).
    ProgramFiles(&'static str),
    /// Fichier du profil (bibliothèque `.library-ms`) : jamais créé comme
    /// un dossier.
    ProfileFile(&'static str),
    /// Dossier virtuel (Panneau de configuration, Ordinateur…) : pas de
    /// chemin, SHGetKnownFolderPath rend E_FAIL.
    Virtual,
}

/// CSIDL documentés → emplacement ; 0x13, 0x1b, 0x1f, 0x2d, 0x30, 0x38 et
/// 0x3b ajoutés pour #2043.
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
        0x13 => Profile("AppData\\Roaming\\Microsoft\\Windows\\Network Shortcuts"),
        0x14 => Fixed("C:\\Windows\\Fonts"),
        0x15 => Profile("AppData\\Roaming\\Microsoft\\Windows\\Templates"),
        0x16 => CommonStartMenu(""),
        0x17 => CommonStartMenu("Programs"),
        0x18 => CommonStartMenu("Programs\\Startup"),
        0x19 => Fixed("C:\\Users\\Public\\Desktop"),
        0x1a => Profile("AppData\\Roaming"),
        0x1b => Profile("AppData\\Roaming\\Microsoft\\Windows\\Printer Shortcuts"),
        0x1c => Profile("AppData\\Local"),
        0x1f => Profile("Favorites"),
        0x20 => Profile("AppData\\Local\\Microsoft\\Windows\\INetCache"),
        0x21 => Profile("AppData\\Local\\Microsoft\\Windows\\INetCookies"),
        0x22 => Profile("AppData\\Local\\Microsoft\\Windows\\History"),
        0x23 => Fixed("C:\\ProgramData"),
        0x24 => Fixed("C:\\Windows"),
        0x25 => Fixed("C:\\Windows\\System32"),
        0x26 => ProgramFiles(""),
        0x27 => Profile("Pictures"),
        0x28 => Profile(""),
        0x29 => Fixed("C:\\Windows\\SysWOW64"),
        0x2a => Fixed("C:\\Program Files (x86)"),
        0x2b => ProgramFiles("Common Files"),
        0x2c => Fixed("C:\\Program Files (x86)\\Common Files"),
        0x2d => Fixed("C:\\ProgramData\\Microsoft\\Windows\\Templates"),
        0x2e => Fixed("C:\\Users\\Public\\Documents"),
        0x2f => CommonStartMenu("Programs\\Administrative Tools"),
        0x30 => StartMenu("Programs\\Administrative Tools"),
        0x35 => Fixed("C:\\Users\\Public\\Music"),
        0x36 => Fixed("C:\\Users\\Public\\Pictures"),
        0x37 => Fixed("C:\\Users\\Public\\Videos"),
        0x38 => Fixed("C:\\Windows\\Resources"),
        0x3b => Profile("AppData\\Local\\Microsoft\\Windows\\Burn\\Burn"),
        _ => return None,
    })
}

/// FOLDERID_* → emplacement. Les GUID et chemins par défaut viennent de la
/// page publique KNOWNFOLDERID (Microsoft Learn, win32 docs,
/// desktop-src/shell/knownfolderid.md) : les 124 identifiants qu'elle
/// documente sont servis (#2477), sauf ceux de OneDrive, absents tant
/// qu'aucun compte n'est configuré.
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
        Place::ProgramFiles(""),
    ),
    (
        "F7F1ED05-9F6D-47A2-AAAE-29D317C6F066",
        Place::ProgramFiles("Common Files"),
    ),
    (
        "6365D5A7-0F0D-45E5-87F6-0DA56B6A4F7D",
        Place::Fixed("C:\\Program Files\\Common Files"),
    ),
    (
        "DE974D24-D9C6-4D3E-BF91-F4455120B917",
        Place::Fixed("C:\\Program Files (x86)\\Common Files"),
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
    // Complément de la page KNOWNFOLDERID (#2477).
    // FOLDERID_AccountPictures
    ("008CA0B1-55B4-4C56-B8A8-4DE4B299D3BE", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\AccountPictures")),
    // FOLDERID_AddNewPrograms
    ("DE61D971-5EBC-4F02-A3A9-6C82895E5C04", Place::Virtual),
    // FOLDERID_AdminTools
    ("724EF170-A42D-4FEF-9F26-B60E846FBA4F", Place::StartMenu("Programs\\Administrative Tools")),
    // FOLDERID_AppDataDesktop
    ("B2C5E279-7ADD-439F-B28C-C41FE1BBF672", Place::Profile("AppData\\Local\\Desktop")),
    // FOLDERID_AppDataDocuments
    ("7BE16610-1F7F-44AC-BFF0-83E15F2FFCA1", Place::Profile("AppData\\Local\\Documents")),
    // FOLDERID_AppDataFavorites
    ("7CFBEFBC-DE1F-45AA-B843-A542AC536CC9", Place::Profile("AppData\\Local\\Favorites")),
    // FOLDERID_AppDataProgramData
    ("559D40A3-A036-40FA-AF61-84CB430A4D34", Place::Profile("AppData\\Local\\ProgramData")),
    // FOLDERID_ApplicationShortcuts
    ("A3918781-E5F2-4890-B3D9-A7E54332328C", Place::Profile("AppData\\Local\\Microsoft\\Windows\\Application Shortcuts")),
    // FOLDERID_AppsFolder
    ("1E87508D-89C2-42F0-8A7E-645A0F50CA58", Place::Virtual),
    // FOLDERID_AppUpdates
    ("A305CE99-F527-492B-8B1A-7E76FA98D6E4", Place::Virtual),
    // FOLDERID_CameraRoll
    ("AB5FB87B-7CE2-4F83-915D-550846C9537B", Place::Profile("Pictures\\Camera Roll")),
    // FOLDERID_CDBurning
    ("9E52AB10-F80D-49DF-ACB8-4330F5687855", Place::Profile("AppData\\Local\\Microsoft\\Windows\\Burn\\Burn")),
    // FOLDERID_ChangeRemovePrograms
    ("DF7266AC-9274-4867-8D55-3BD661DE872D", Place::Virtual),
    // FOLDERID_CommonAdminTools
    ("D0384E7D-BAC3-4797-8F14-CBA229B392B5", Place::CommonStartMenu("Programs\\Administrative Tools")),
    // FOLDERID_CommonOEMLinks
    ("C1BAE2D0-10DF-4334-BEDD-7AA20B227A9D", Place::Fixed("C:\\ProgramData\\OEM Links")),
    // FOLDERID_CommonStartup
    ("82A5EA35-D9CD-47C5-9629-E15D2F714E6E", Place::CommonStartMenu("Programs\\Startup")),
    // FOLDERID_CommonTemplates
    ("B94237E7-57AC-4347-9151-B08C6C32D1F7", Place::Fixed("C:\\ProgramData\\Microsoft\\Windows\\Templates")),
    // FOLDERID_ComputerFolder
    ("0AC0837C-BBF8-452A-850D-79D08E667CA7", Place::Virtual),
    // FOLDERID_ConflictFolder
    ("4BFEFB45-347D-4006-A5BE-AC0CB0567192", Place::Virtual),
    // FOLDERID_ConnectionsFolder
    ("6F0CD92B-2E97-45D1-88FF-B0D186B8DEDD", Place::Virtual),
    // FOLDERID_Contacts
    ("56784854-C6CB-462B-8169-88E350ACB882", Place::Profile("Contacts")),
    // FOLDERID_ControlPanelFolder
    ("82A74AEB-AEB4-465C-A014-D097EE346D63", Place::Virtual),
    // FOLDERID_Cookies
    ("2B0F765D-C0E9-4171-908E-08A611B84FF6", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\Cookies")),
    // FOLDERID_DeviceMetadataStore
    ("5CE4A5E9-E4EB-479D-B89F-130C02886155", Place::Fixed("C:\\ProgramData\\Microsoft\\Windows\\DeviceMetadataStore")),
    // FOLDERID_DocumentsLibrary
    ("7B0DB17D-9CD2-4A93-9733-46CC89022E7C", Place::ProfileFile("AppData\\Roaming\\Microsoft\\Windows\\Libraries\\Documents.library-ms")),
    // FOLDERID_Fonts
    ("FD228CB7-AE11-4AE3-864C-16F3910AB8FE", Place::Fixed("C:\\Windows\\Fonts")),
    // FOLDERID_Games
    ("CAC52C1A-B53D-4EDC-92D7-6B2E8AC19434", Place::Virtual),
    // FOLDERID_GameTasks
    ("054FAE61-4DD8-4787-80B6-090220C4B700", Place::Profile("AppData\\Local\\Microsoft\\Windows\\GameExplorer")),
    // FOLDERID_History
    ("D9DC8A3B-B784-432E-A781-5A1130A75963", Place::Profile("AppData\\Local\\Microsoft\\Windows\\History")),
    // FOLDERID_HomeGroup
    ("52528A6B-B9E3-4ADD-B60D-588C2DBA842D", Place::Virtual),
    // FOLDERID_HomeGroupCurrentUser
    ("9B74B6A3-0DFD-4F11-9E78-5F7800F2E772", Place::Virtual),
    // FOLDERID_ImplicitAppShortcuts
    ("BCB5256F-79F6-4CEE-B725-DC34E402FD46", Place::Profile("AppData\\Roaming\\Microsoft\\Internet Explorer\\Quick Launch\\User Pinned\\ImplicitAppShortcuts")),
    // FOLDERID_InternetCache
    ("352481E8-33BE-4251-BA85-6007CAEDCF9D", Place::Profile("AppData\\Local\\Microsoft\\Windows\\Temporary Internet Files")),
    // FOLDERID_InternetFolder
    ("4D9F7874-4E0C-4904-967B-40B0D20C3E4B", Place::Virtual),
    // FOLDERID_Libraries
    ("1B3EA5DC-B587-4786-B4EF-BD1DC332AEAE", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\Libraries")),
    // FOLDERID_Links
    ("BFB9D5E0-C6A9-404C-B2B2-AE6DB6AF4968", Place::Profile("Links")),
    // FOLDERID_LocalizedResourcesDir
    ("2A00375E-224C-49DE-B8D1-440DF7EF3DDC", Place::Fixed("C:\\Windows\\Resources\\0409")),
    // FOLDERID_MusicLibrary
    ("2112AB0A-C86A-4FFE-A368-0DE96E47012E", Place::ProfileFile("AppData\\Roaming\\Microsoft\\Windows\\Libraries\\Music.library-ms")),
    // FOLDERID_NetHood
    ("C5ABBF53-E17F-4121-8900-86626FC2C973", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\Network Shortcuts")),
    // FOLDERID_NetworkFolder
    ("D20BEEC4-5CA8-4905-AE3B-BF251EA09B53", Place::Virtual),
    // FOLDERID_Objects3D
    ("31C0DD25-9439-4F12-BF41-7FF4EDA38722", Place::Profile("3D Objects")),
    // FOLDERID_OriginalImages
    ("2C36C0AA-5812-4B87-BFD0-4CD0DFB19B39", Place::Profile("AppData\\Local\\Microsoft\\Windows Photo Gallery\\Original Images")),
    // FOLDERID_PhotoAlbums
    ("69D2CF90-FC33-4FB7-9A0C-EBB0F0FCB43C", Place::Profile("Pictures\\Slide Shows")),
    // FOLDERID_PicturesLibrary
    ("A990AE9F-A03B-4E80-94BC-9912D7504104", Place::ProfileFile("AppData\\Roaming\\Microsoft\\Windows\\Libraries\\Pictures.library-ms")),
    // FOLDERID_Playlists
    ("DE92C1C7-837F-4F69-A3BB-86E631204A23", Place::Profile("Music\\Playlists")),
    // FOLDERID_PrintersFolder
    ("76FC4E2D-D6AD-4519-A663-37BD56068185", Place::Virtual),
    // FOLDERID_PrintHood
    ("9274BD8D-CFD1-41C3-B35E-B13F55A758F4", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\Printer Shortcuts")),
    // FOLDERID_PublicDesktop
    ("C4AA340D-F20F-4863-AFEF-F87EF2E6BA25", Place::Fixed("C:\\Users\\Public\\Desktop")),
    // FOLDERID_PublicDownloads
    ("3D644C9B-1FB8-4F30-9B45-F670235F79C0", Place::Fixed("C:\\Users\\Public\\Downloads")),
    // FOLDERID_PublicGameTasks
    ("DEBF2536-E1A8-4C59-B6A2-414586476AEA", Place::Fixed("C:\\ProgramData\\Microsoft\\Windows\\GameExplorer")),
    // FOLDERID_PublicLibraries
    ("48DAF80B-E6CF-4F4E-B800-0E69D84EE384", Place::Fixed("C:\\ProgramData\\Microsoft\\Windows\\Libraries")),
    // FOLDERID_PublicMusic
    ("3214FAB5-9757-4298-BB61-92A9DEAA44FF", Place::Fixed("C:\\Users\\Public\\Music")),
    // FOLDERID_PublicPictures
    ("B6EBFB86-6907-413C-9AF7-4FC2ABF07CC5", Place::Fixed("C:\\Users\\Public\\Pictures")),
    // FOLDERID_PublicRingtones
    ("E555AB60-153B-4D17-9F04-A5FE99FC15EC", Place::Fixed("C:\\ProgramData\\Microsoft\\Windows\\Ringtones")),
    // FOLDERID_PublicUserTiles
    ("0482AF6C-08F1-4C34-8C90-E17EC98B1E17", Place::Fixed("C:\\Users\\Public\\AccountPictures")),
    // FOLDERID_PublicVideos
    ("2400183A-6185-49FB-A2D8-4A392A602BA3", Place::Fixed("C:\\Users\\Public\\Videos")),
    // FOLDERID_QuickLaunch
    ("52A4F021-7B75-48A9-9F6B-4B87A210BC8F", Place::Profile("AppData\\Roaming\\Microsoft\\Internet Explorer\\Quick Launch")),
    // FOLDERID_Recent
    ("AE50C081-EBD2-438A-8655-8A092E34987A", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\Recent")),
    // FOLDERID_RecordedTVLibrary
    ("1A6FDBA2-F42D-4358-A798-B74D745926C5", Place::Fixed("C:\\Users\\Public\\RecordedTV.library-ms")),
    // FOLDERID_RecycleBinFolder
    ("B7534046-3ECB-4C18-BE4E-64CD4CB7D6AC", Place::Virtual),
    // FOLDERID_ResourceDir
    ("8AD10C31-2ADB-4296-A8F7-E4701232C972", Place::Fixed("C:\\Windows\\Resources")),
    // FOLDERID_Ringtones
    ("C870044B-F49E-4126-A9C3-B52A1FF411E8", Place::Profile("AppData\\Local\\Microsoft\\Windows\\Ringtones")),
    // FOLDERID_RoamedTileImages
    ("AAA8D5A5-F1D6-4259-BAA8-78E7EF60835E", Place::Profile("AppData\\Local\\Microsoft\\Windows\\RoamedTileImages")),
    // FOLDERID_RoamingTiles
    ("00BCFC5A-ED94-4E48-96A1-3F6217F21990", Place::Profile("AppData\\Local\\Microsoft\\Windows\\RoamingTiles")),
    // FOLDERID_SampleMusic
    ("B250C668-F57D-4EE1-A63C-290EE7D1AA1F", Place::Fixed("C:\\Users\\Public\\Music\\Sample Music")),
    // FOLDERID_SamplePictures
    ("C4900540-2379-4C75-844B-64E6FAF8716B", Place::Fixed("C:\\Users\\Public\\Pictures\\Sample Pictures")),
    // FOLDERID_SamplePlaylists
    ("15CA69B3-30EE-49C1-ACE1-6B5EC372AFB5", Place::Fixed("C:\\Users\\Public\\Music\\Sample Playlists")),
    // FOLDERID_SampleVideos
    ("859EAD94-2E85-48AD-A71A-0969CB56A6CD", Place::Fixed("C:\\Users\\Public\\Videos\\Sample Videos")),
    // FOLDERID_SavedPictures
    ("3B193882-D3AD-4EAB-965A-69829D1FB59F", Place::Profile("Pictures\\Saved Pictures")),
    // FOLDERID_SavedPicturesLibrary
    ("E25B5812-BE88-4BD9-94B0-29233477B6C3", Place::ProfileFile("AppData\\Roaming\\Microsoft\\Windows\\Libraries\\SavedPictures.library-ms")),
    // FOLDERID_SavedSearches
    ("7D1D3A04-DEBB-4115-95CF-2F29DA2920DA", Place::Profile("Searches")),
    // FOLDERID_Screenshots
    ("B7BEDE81-DF94-4682-A7D8-57A52620B86F", Place::Profile("Pictures\\Screenshots")),
    // FOLDERID_SEARCH_CSC
    ("EE32E446-31CA-4ABA-814F-A5EBD2FD6D5E", Place::Virtual),
    // FOLDERID_SearchHistory
    ("0D4C3DB6-03A3-462F-A0E6-08924C41B5D4", Place::Profile("AppData\\Local\\Microsoft\\Windows\\ConnectedSearch\\History")),
    // FOLDERID_SearchHome
    ("190337D1-B8CA-4121-A639-6D472D16972A", Place::Virtual),
    // FOLDERID_SEARCH_MAPI
    ("98EC0E18-2098-4D44-8644-66979315A281", Place::Virtual),
    // FOLDERID_SearchTemplates
    ("7E636BFE-DFA9-4D5E-B456-D7B39851D8A9", Place::Profile("AppData\\Local\\Microsoft\\Windows\\ConnectedSearch\\Templates")),
    // FOLDERID_SendTo
    ("8983036C-27C0-404B-8F08-102D10DCFD74", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\SendTo")),
    // FOLDERID_SidebarDefaultParts
    ("7B396E54-9EC5-4300-BE0A-2482EBAE1A26", Place::ProgramFiles("Windows Sidebar\\Gadgets")),
    // FOLDERID_SidebarParts
    ("A75D362E-50FC-4FB7-AC2C-A8BEAA314493", Place::Profile("AppData\\Local\\Microsoft\\Windows Sidebar\\Gadgets")),
    // FOLDERID_SkyDrive : OneDrive non configuré, dossier absent du système.
    // FOLDERID_SkyDriveCameraRoll : OneDrive non configuré, dossier absent du système.
    // FOLDERID_SkyDriveDocuments : OneDrive non configuré, dossier absent du système.
    // FOLDERID_SkyDrivePictures : OneDrive non configuré, dossier absent du système.
    // FOLDERID_SyncManagerFolder
    ("43668BF8-C14E-49B2-97C9-747784D784B7", Place::Virtual),
    // FOLDERID_SyncResultsFolder
    ("289A9A43-BE44-4057-A41B-587A76D7E7F9", Place::Virtual),
    // FOLDERID_SyncSetupFolder
    ("0F214138-B1D3-4A90-BBA9-27CBC0C5389A", Place::Virtual),
    // FOLDERID_SystemX86
    ("D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27", Place::Fixed("C:\\Windows\\SysWOW64")),
    // FOLDERID_Templates
    ("A63293E8-664E-48DB-A079-DF759E0509F7", Place::Profile("AppData\\Roaming\\Microsoft\\Windows\\Templates")),
    // FOLDERID_UserPinned
    ("9E3995AB-1F9C-4F13-B827-48B24B6C7174", Place::Profile("AppData\\Roaming\\Microsoft\\Internet Explorer\\Quick Launch\\User Pinned")),
    // FOLDERID_UserProgramFiles
    ("5CD7AEE2-2219-4A67-B85D-6C9CE15660CB", Place::Profile("AppData\\Local\\Programs")),
    // FOLDERID_UserProgramFilesCommon
    ("BCBD3057-CA5C-4622-B42D-BC56DB0AE516", Place::Profile("AppData\\Local\\Programs\\Common")),
    // FOLDERID_UsersFiles
    ("F3CE0F7C-4901-4ACC-8648-D5D44B04EF8F", Place::Virtual),
    // FOLDERID_UsersLibraries
    ("A302545D-DEFF-464B-ABE8-61C8648D939B", Place::Virtual),
    // FOLDERID_VideosLibrary
    ("491E922F-5643-4AF4-A7EB-4E7A138D8174", Place::ProfileFile("AppData\\Roaming\\Microsoft\\Windows\\Libraries\\Videos.library-ms")),
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
        Place::ProgramFiles(sub) => join(program_files(crate::wow32::active()).into(), sub),
        Place::ProfileFile(rel) => crate::paths::profile_path(rel),
        Place::Virtual => String::new(),
    }
}

/// `Program Files` d'un programme 64 bits ou 32 bits.
fn program_files(x86: bool) -> &'static str {
    if x86 {
        "C:\\Program Files (x86)"
    } else {
        "C:\\Program Files"
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

pub(crate) fn guid_text(p: *const u8) -> Option<String> {
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

/// Résultat de la recherche d'un FOLDERID.
pub(crate) enum KnownFolder {
    Path(String),
    /// Dossier virtuel : connu, mais sans chemin.
    Virtual,
    /// GUID absent de la table : « not present on the system ».
    Unknown,
}

pub(crate) fn known_folder(guid: &str, flags: u32) -> KnownFolder {
    let Some((_, place)) = KNOWN_FOLDERS
        .iter()
        .find(|(g, _)| g.eq_ignore_ascii_case(guid))
    else {
        return KnownFolder::Unknown;
    };
    if matches!(place, Place::Virtual) {
        return KnownFolder::Virtual;
    }
    let path = place_path(place);
    ensure(place, &path, flags & KF_FLAG_CREATE != 0);
    KnownFolder::Path(path)
}

/// Chemin Windows d'un FOLDERID (texte `XXXXXXXX-XXXX-…`).
pub fn known_folder_path(guid: &str, flags: u32) -> Option<String> {
    match known_folder(guid, flags) {
        KnownFolder::Path(p) => Some(p),
        _ => None,
    }
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
    crate::trace!("SHGetFolderPathA(csidl {csidl:#x})");
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
    let found = csidl_path(csidl);
    crate::trace!(
        "SHGetFolderPathW(csidl {csidl:#x}, token {_token:#x}, flags {_flags}) -> {found:?}"
    );
    match found {
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
    // Codes documentés par SHGetKnownFolderPath : E_FAIL pour un dossier
    // sans chemin (virtuel), E_INVALIDARG pour un FOLDERID absent du
    // système. Le refus est journalisé : sans trace d'API, rien ne disait
    // quel GUID manquait (#2477).
    let path = match known_folder(&guid, flags) {
        KnownFolder::Path(p) => p,
        KnownFolder::Virtual => {
            crate::log_info!(
                "shell32",
                "SHGetKnownFolderPath({{{guid}}}) : dossier virtuel, sans chemin"
            );
            return E_FAIL;
        }
        KnownFolder::Unknown => {
            crate::log_warn!(
                "shell32",
                "SHGetKnownFolderPath({{{guid}}}) : FOLDERID inconnu"
            );
            return E_INVALIDARG;
        }
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
pub(crate) unsafe fn profile_out<T: Copy>(
    text: Vec<T>,
    zero: T,
    buf: *mut T,
    size: *mut u32,
) -> i32 {
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

/// GetAppContainerFolderPath (userenv) : selon la documentation publique, le
/// même chemin que SHGetKnownFolderPath(FOLDERID_LocalAppData), rendu par
/// CoTaskMemAlloc ; E_INVALIDARG si un des deux paramètres est NULL (#2548).
pub unsafe extern "win64" fn GetAppContainerFolderPath(sid: *const u16, out: *mut *mut u16) -> i32 {
    const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
    const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
    const E_FAIL: i32 = 0x8000_4005u32 as i32;
    if sid.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    let Some(path) = known_folder_path("F1B32785-6FBA-4FCF-9D55-7B8E7F157091", 0) else {
        return E_FAIL;
    };
    let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
    let p = crate::dlls::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    *out = p;
    0
}

pub fn userenv_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetAppContainerFolderPath" => fn GetAppContainerFolderPath,
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
    fn dossier_d_un_conteneur_est_local_app_data() {
        // #2548 : même chemin que FOLDERID_LocalAppData ; NULL refusé.
        let _teb = crate::process::test_teb();
        let _g = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(crate::dlls::lookup("userenv", "GetAppContainerFolderPath").is_some());
        let sid: Vec<u16> = "S-1-15-2-1-2-3-4-5-6-7".encode_utf16().chain([0]).collect();
        let mut out = std::ptr::null_mut::<u16>();
        unsafe {
            assert_eq!(GetAppContainerFolderPath(sid.as_ptr(), &mut out), 0);
            assert!(!out.is_null());
            assert_eq!(
                Some(crate::dlls::wstr(out)),
                known_folder_path("F1B32785-6FBA-4FCF-9D55-7B8E7F157091", 0)
            );
            crate::dlls::ole32::co_task_mem_free(out.cast());
            assert_eq!(
                GetAppContainerFolderPath(std::ptr::null(), &mut out) as u32,
                0x8007_0057
            );
            assert_eq!(
                GetAppContainerFolderPath(sid.as_ptr(), std::ptr::null_mut()) as u32,
                0x8007_0057
            );
        }
    }

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

    /// GUID texte → 16 octets en mémoire, comme le passe une appli.
    fn guid_bytes(t: &str) -> [u8; 16] {
        let h: String = t.chars().filter(|c| *c != '-').collect();
        let b: Vec<u8> = (0..16)
            .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        let mut g = [0u8; 16];
        g[..4].copy_from_slice(&[b[3], b[2], b[1], b[0]]);
        g[4..6].copy_from_slice(&[b[5], b[4]]);
        g[6..8].copy_from_slice(&[b[7], b[6]]);
        g[8..].copy_from_slice(&b[8..]);
        g
    }

    unsafe fn sh_path(guid: &str) -> (i32, Option<String>) {
        let g = guid_bytes(guid);
        let mut out: *mut u16 = std::ptr::null_mut();
        let hr = sh_get_known_folder_path(g.as_ptr(), 0, 0, &mut out);
        let text = (!out.is_null()).then(|| crate::dlls::wstr(out));
        if !out.is_null() {
            crate::dlls::ole32::co_task_mem_free(out.cast());
        }
        (hr, text)
    }

    /// #2477 : FOLDERID documentés servis, codes d'échec documentés.
    #[test]
    fn folderid_documentes_et_codes_d_echec() {
        // Tous les GUID de la table sont uniques et bien formés.
        let mut vus = std::collections::HashSet::new();
        for (g, _) in KNOWN_FOLDERS {
            assert_eq!(g.len(), 36, "{g}");
            assert_eq!(guid_text(guid_bytes(g).as_ptr()).as_deref(), Some(*g));
            assert!(vus.insert(g.to_ascii_uppercase()), "GUID en double : {g}");
        }
        // 124 GUID documentés, moins les 4 de OneDrive.
        assert_eq!(KNOWN_FOLDERS.len(), 120);
        unsafe {
            // Ceux que demandait l'installeur de Google Drive, entre autres.
            for (guid, attendu) in [
                (
                    "C4AA340D-F20F-4863-AFEF-F87EF2E6BA25",
                    "C:\\Users\\Public\\Desktop",
                ),
                (
                    "B94237E7-57AC-4347-9151-B08C6C32D1F7",
                    "C:\\ProgramData\\Microsoft\\Windows\\Templates",
                ),
                (
                    "3D644C9B-1FB8-4F30-9B45-F670235F79C0",
                    "C:\\Users\\Public\\Downloads",
                ),
                ("FD228CB7-AE11-4AE3-864C-16F3910AB8FE", "C:\\Windows\\Fonts"),
            ] {
                assert_eq!(sh_path(guid), (S_OK, Some(attendu.to_string())), "{guid}");
            }
            let (hr, p) = sh_path("52A4F021-7B75-48A9-9F6B-4B87A210BC8F");
            assert_eq!(hr, S_OK);
            assert!(p
                .unwrap()
                .ends_with("\\AppData\\Roaming\\Microsoft\\Internet Explorer\\Quick Launch"));
            // FOLDERID_ControlPanelFolder : virtuel, E_FAIL.
            assert_eq!(
                sh_path("82A74AEB-AEB4-465C-A014-D097EE346D63"),
                (E_FAIL, None)
            );
            // GUID absent du système : E_INVALIDARG, plus 0x80070002.
            assert_eq!(
                sh_path("01234567-89AB-CDEF-0123-456789ABCDEF"),
                (E_INVALIDARG, None)
            );
        }
    }

    #[test]
    fn program_files_selon_l_architecture() {
        assert_eq!(program_files(false), "C:\\Program Files");
        assert_eq!(program_files(true), "C:\\Program Files (x86)");
        // FOLDERID_ProgramFilesCommonX86 et X64 ne dépendent pas de l'appelant.
        assert_eq!(
            known_folder_path("DE974D24-D9C6-4D3E-BF91-F4455120B917", 0).unwrap(),
            "C:\\Program Files (x86)\\Common Files"
        );
        assert_eq!(
            known_folder_path("6365D5A7-0F0D-45E5-87F6-0DA56B6A4F7D", 0).unwrap(),
            "C:\\Program Files\\Common Files"
        );
        if !crate::wow32::active() {
            assert_eq!(csidl_path(0x2b).unwrap(), "C:\\Program Files\\Common Files");
            assert_eq!(
                known_folder_path("F7F1ED05-9F6D-47A2-AAAE-29D317C6F066", 0).unwrap(),
                "C:\\Program Files\\Common Files"
            );
        }
    }
}
