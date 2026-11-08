//! Test PE des boîtes communes de comdlg32 : « Ouvrir », « Enregistrer
//! sous », « Couleur » et « Police », appelées comme le fait PuTTY (clé
//! privée, journal, couleurs du terminal, police à espacement fixe).
//!
//! Sans affichage, les variables WINLUX_FILEDLG / WINLUX_CHOOSECOLOR /
//! WINLUX_CHOOSEFONT (posées ici par SetEnvironmentVariableW) font le choix
//! de l'utilisateur. Avec `WINLUX_CONTROLS_SHOW=<ms>`, chaque boîte reste
//! affichée ce temps-là (capture d'écran) puis est validée par Entrée.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use core::ffi::{c_char, c_void};
use winlux_testexe::*;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "C" {
    fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
    fn GetTempPathW(size: u32, buf: *mut u16) -> u32;
}

#[link(name = "comdlg32", kind = "raw-dylib")]
extern "C" {
    fn GetOpenFileNameW(ofn: *mut OpenFileName) -> i32;
    fn GetSaveFileNameW(ofn: *mut OpenFileName) -> i32;
    fn GetOpenFileNameA(ofn: *mut OpenFileName) -> i32;
    fn ChooseColorW(cc: *mut ChooseColor) -> i32;
    fn ChooseFontW(cf: *mut ChooseFont) -> i32;
    fn CommDlgExtendedError() -> u32;
    fn FindTextW(fr: *mut FindReplace) -> Handle;
    fn FindTextA(fr: *mut FindReplace) -> Handle;
    fn ReplaceTextW(fr: *mut FindReplace) -> Handle;
}

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    fn GetDlgItem(dlg: Handle, id: i32) -> Handle;
    fn RegisterWindowMessageW(name: *const u16) -> u32;
    fn RegisterWindowMessageA(name: *const c_char) -> u32;
    fn IsWindowEnabled(hwnd: Handle) -> i32;
}

#[link(name = "ole32", kind = "raw-dylib")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, model: u32) -> i32;
    fn CoCreateInstance(
        clsid: *const [u8; 16],
        outer: *mut c_void,
        ctx: u32,
        iid: *const [u8; 16],
        out: *mut *mut c_void,
    ) -> i32;
    fn CoTaskMemFree(p: *mut c_void);
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    fn SHCreateItemFromParsingName(
        path: *const u16,
        bc: *mut c_void,
        iid: *const [u8; 16],
        out: *mut *mut c_void,
    ) -> i32;
}

/// GUID écrit dans l'ordre de la mémoire (Data1..3 en petit-boutiste).
const fn guid(a: u32, b: u16, c: u16, d: [u8; 8]) -> [u8; 16] {
    let a = a.to_le_bytes();
    let b = b.to_le_bytes();
    let c = c.to_le_bytes();
    [
        a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5], d[6],
        d[7],
    ]
}
const CLSID_FILE_OPEN_DIALOG: [u8; 16] = guid(
    0xDC1C_5A9C,
    0xE88A,
    0x4DDE,
    [0xA5, 0xA1, 0x60, 0xF8, 0x2A, 0x20, 0xAE, 0xF7],
);
const CLSID_FILE_SAVE_DIALOG: [u8; 16] = guid(
    0xC0B4_E2F3,
    0xBA21,
    0x4773,
    [0x8D, 0xBA, 0x33, 0x5E, 0xC9, 0x46, 0xEB, 0x8B],
);
const IID_IFILE_OPEN_DIALOG: [u8; 16] = guid(
    0xD57C_7288,
    0xD4AD,
    0x4768,
    [0xBE, 0x02, 0x9D, 0x96, 0x95, 0x32, 0xD9, 0x60],
);
const IID_IFILE_SAVE_DIALOG: [u8; 16] = guid(
    0x84BC_CD23,
    0x5FDE,
    0x4CDB,
    [0xAE, 0xA4, 0xAF, 0x64, 0xB8, 0x3D, 0x78, 0xAB],
);
const IID_ISHELL_ITEM: [u8; 16] = guid(
    0x4382_6D1E,
    0xE718,
    0x42EE,
    [0xBC, 0x55, 0xA1, 0xE2, 0x61, 0xC3, 0x7B, 0xFE],
);

/// Entrée `n` de la vtable d'un objet COM.
unsafe fn slot(obj: *mut c_void, n: usize) -> usize {
    *(*(obj as *const *const usize)).add(n)
}

unsafe fn release(obj: *mut c_void) {
    let f: extern "system" fn(*mut c_void) -> u32 = core::mem::transmute(slot(obj, 2));
    f(obj);
}

/// IShellItem::GetDisplayName(SIGDN_FILESYSPATH) copié dans `out`.
unsafe fn item_path(item: *mut c_void, out: &mut [u16]) -> bool {
    let f: extern "system" fn(*mut c_void, u32, *mut *mut u16) -> i32 =
        core::mem::transmute(slot(item, 5));
    let mut p: *mut u16 = core::ptr::null_mut();
    if f(item, 0x8005_8000, &mut p) != 0 || p.is_null() {
        return false;
    }
    let mut i = 0;
    while i + 1 < out.len() && *p.add(i) != 0 {
        out[i] = *p.add(i);
        i += 1;
    }
    out[i] = 0;
    CoTaskMemFree(p.cast());
    true
}

/// Boîte COM de Vista, appelée comme Notepad++ 8 (Fichier > Ouvrir,
/// Enregistrer sous) : CoCreateInstance, SetFileTypes, SetFolder, Show,
/// GetResult. Sans elle, Notepad++ ne montre rien (#699).
unsafe fn com_dialog(temp: &[u16], save: bool) {
    CoInitializeEx(core::ptr::null_mut(), 2);
    let (clsid, iid) = if save {
        (&CLSID_FILE_SAVE_DIALOG, &IID_IFILE_SAVE_DIALOG)
    } else {
        (&CLSID_FILE_OPEN_DIALOG, &IID_IFILE_OPEN_DIALOG)
    };
    let mut d: *mut c_void = core::ptr::null_mut();
    check!(
        CoCreateInstance(clsid, core::ptr::null_mut(), 1, iid, &mut d) == 0 && !d.is_null(),
        "CoCreateInstance(CLSID_FileOpenDialog / FileSaveDialog)"
    );
    let name = w(b"Cle privee PuTTY\0");
    let spec = w(b"*.ppk;*.log\0");
    let specs = [[name.as_ptr(), spec.as_ptr()]];
    let set_types: extern "system" fn(*mut c_void, u32, *const [*const u16; 2]) -> i32 =
        core::mem::transmute(slot(d, 4));
    check!(
        set_types(d, 1, specs.as_ptr()) == 0,
        "IFileDialog::SetFileTypes"
    );
    let mut folder: *mut c_void = core::ptr::null_mut();
    check!(
        SHCreateItemFromParsingName(
            temp.as_ptr(),
            core::ptr::null_mut(),
            &IID_ISHELL_ITEM,
            &mut folder
        ) == 0,
        "SHCreateItemFromParsingName"
    );
    let set_folder: extern "system" fn(*mut c_void, *mut c_void) -> i32 =
        core::mem::transmute(slot(d, 12));
    check!(set_folder(d, folder) == 0, "IFileDialog::SetFolder");
    release(folder);
    if save {
        let def = w(b"log\0");
        let set_def: extern "system" fn(*mut c_void, *const u16) -> i32 =
            core::mem::transmute(slot(d, 22));
        set_def(d, def.as_ptr());
    }
    let show: extern "system" fn(*mut c_void, Handle) -> i32 = core::mem::transmute(slot(d, 3));
    check!(show(d, OWNER) == 0, "IFileDialog::Show");
    let mut item: *mut c_void = core::ptr::null_mut();
    let get_result: extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 =
        core::mem::transmute(slot(d, 20));
    check!(
        get_result(d, &mut item) == 0 && !item.is_null(),
        "IFileDialog::GetResult"
    );
    let mut path = [0u16; 300];
    check!(
        item_path(item, &mut path),
        "IShellItem::GetDisplayName(SIGDN_FILESYSPATH)"
    );
    if save {
        check!(
            ends_with(&path, b"\\session-com.log"),
            "IFileSaveDialog : extension par defaut"
        );
    } else {
        check!(
            ends_with(&path, b"\\winlux-cle-test.ppk"),
            "IFileOpenDialog : fichier choisi"
        );
    }
    release(item);
    release(d);
}

#[repr(C)]
struct OpenFileName {
    size: u32,
    owner: Handle,
    instance: Handle,
    filter: *const u16,
    custom_filter: *mut u16,
    max_cust_filter: u32,
    filter_index: u32,
    file: *mut u16,
    max_file: u32,
    file_title: *mut u16,
    max_file_title: u32,
    initial_dir: *const u16,
    title: *const u16,
    flags: u32,
    file_offset: u16,
    file_extension: u16,
    def_ext: *const u16,
    cust_data: isize,
    hook: usize,
    template: *const u16,
    reserved_ptr: *mut c_void,
    reserved: u32,
    flags_ex: u32,
}

#[repr(C)]
struct ChooseColor {
    size: u32,
    owner: Handle,
    instance: Handle,
    result: u32,
    custom: *mut u32,
    flags: u32,
    cust_data: isize,
    hook: usize,
    template: *const u16,
}

#[repr(C)]
struct LogFontW {
    height: i32,
    width: i32,
    escapement: i32,
    orientation: i32,
    weight: i32,
    italic: u8,
    underline: u8,
    strikeout: u8,
    charset: u8,
    out_precision: u8,
    clip_precision: u8,
    quality: u8,
    pitch_and_family: u8,
    face: [u16; 32],
}

#[repr(C)]
struct ChooseFont {
    size: u32,
    owner: Handle,
    dc: Handle,
    logfont: *mut LogFontW,
    point_size: i32,
    flags: u32,
    colors: u32,
    cust_data: isize,
    hook: usize,
    template: *const u16,
    instance: Handle,
    style: *mut u16,
    font_type: u16,
    size_min: i32,
    size_max: i32,
}

/// FINDREPLACE (commdlg.h), 80 octets en 64 bits.
#[repr(C)]
struct FindReplace {
    size: u32,
    owner: Handle,
    instance: Handle,
    flags: u32,
    find: *mut u16,
    replace: *mut u16,
    find_len: u16,
    replace_len: u16,
    cust_data: isize,
    hook: usize,
    template: *const u16,
}

const FR_DOWN: u32 = 0x1;
const FR_WHOLEWORD: u32 = 0x2;
const FR_MATCHCASE: u32 = 0x4;
const FR_FINDNEXT: u32 = 0x8;
const FR_REPLACE: u32 = 0x10;
const FR_REPLACEALL: u32 = 0x20;
const FR_DIALOGTERM: u32 = 0x40;
const FR_ENABLEHOOK: u32 = 0x100;

static mut SHOW_MS: u32 = 0;
static mut OWNER: Handle = core::ptr::null_mut();
/// Message « commdlg_FindReplace » et ce que le propriétaire en a reçu.
static mut FIND_MSG: u32 = 0;
static mut FIND_COUNT: u32 = 0;
static mut FIND_FLAGS: u32 = 0;
static mut FIND_LPARAM: isize = 0;
static mut HOOK_INIT: isize = 0;

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    if FIND_MSG != 0 && msg == FIND_MSG {
        FIND_COUNT += 1;
        FIND_LPARAM = lparam;
        FIND_FLAGS = (*(lparam as *const FindReplace)).flags;
        return 0;
    }
    DefWindowProcA(hwnd, msg, wparam, lparam)
}

/// Procédure crochet : retient le lParam de WM_INITDIALOG (la FINDREPLACE).
unsafe extern "win64" fn find_hook(_dlg: Handle, msg: u32, _wp: usize, lp: isize) -> isize {
    if msg == 0x0110 {
        HOOK_INIT = lp;
    }
    0
}

unsafe fn item_is(dlg: Handle, id: i32, text: &[u16]) -> bool {
    let mut buf = [0u16; 64];
    let n = GetWindowTextW(GetDlgItem(dlg, id), buf.as_mut_ptr(), 64) as usize;
    n + 1 == text.len() && buf[..n] == text[..n]
}

fn same(buf: &[u16], text: &[u16]) -> bool {
    buf.len() >= text.len() && buf[..text.len()] == *text
}

/// Boîtes non modales Rechercher et Remplacer (#712) : le propriétaire
/// reçoit « commdlg_FindReplace » avec SA structure, le texte et les
/// options cochées, puis FR_DIALOGTERM à la fermeture.
unsafe fn find_replace() {
    let name = w(b"commdlg_FindReplace\0");
    FIND_MSG = RegisterWindowMessageW(name.as_ptr());
    let other = w(b"TaskbarCreated\0");
    check!(FIND_MSG >= 0xC000, "RegisterWindowMessageW(FINDMSGSTRING)");
    check!(
        RegisterWindowMessageW(other.as_ptr()) != FIND_MSG
            && RegisterWindowMessageA(c"COMMDLG_FINDREPLACE".as_ptr()) == FIND_MSG,
        "messages enregistres : un identifiant par nom, sans casse"
    );

    let mut what = [0u16; 64];
    let abc = w(b"abc\0");
    what[..abc.len()].copy_from_slice(&abc);
    let mut fr = FindReplace {
        size: core::mem::size_of::<FindReplace>() as u32,
        owner: OWNER,
        instance: core::ptr::null_mut(),
        flags: FR_DOWN,
        find: what.as_mut_ptr(),
        replace: core::ptr::null_mut(),
        find_len: 64,
        replace_len: 0,
        cust_data: 0,
        hook: 0,
        template: core::ptr::null(),
    };
    check!(fr.size == 80, "FINDREPLACE de 80 octets");
    let dlg = FindTextW(&mut fr);
    check!(!dlg.is_null() && IsWindow(dlg) != 0, "FindTextW ouvre la boite non modale");
    if dlg.is_null() {
        return;
    }
    check!(item_is(dlg, 0x480, &abc), "texte de depart repris du programme");
    check!(IsWindowEnabled(GetDlgItem(dlg, 1)) != 0, "Suivant actif avec du texte");
    let typed = w(b"Winlux\0");
    SetWindowTextW(GetDlgItem(dlg, 0x480), typed.as_ptr());
    SendMessageW(GetDlgItem(dlg, 0x411), 0x00F1, 1, 0); // Respecter la casse
    SendMessageW(dlg, 0x0111, 1, 0); // Suivant
    check!(
        FIND_COUNT == 1 && FIND_LPARAM == &fr as *const FindReplace as isize,
        "Suivant : le proprietaire recoit sa FINDREPLACE"
    );
    check!(
        FIND_FLAGS & (FR_FINDNEXT | FR_MATCHCASE | FR_DOWN) == (FR_FINDNEXT | FR_MATCHCASE | FR_DOWN)
            && FIND_FLAGS & FR_WHOLEWORD == 0,
        "Suivant : FR_FINDNEXT, casse, vers le bas"
    );
    check!(same(&what, &typed), "texte cherche recopie dans le tampon");
    SendMessageW(dlg, 0x0111, 0x420, 0); // Haut
    SendMessageW(GetDlgItem(dlg, 0x410), 0x00F1, 1, 0); // Mot entier
    SendMessageW(dlg, 0x0111, 1, 0);
    check!(
        FIND_COUNT == 2 && FIND_FLAGS & FR_DOWN == 0 && FIND_FLAGS & FR_WHOLEWORD != 0,
        "Haut et mot entier"
    );
    let empty = [0u16; 1];
    SetWindowTextW(GetDlgItem(dlg, 0x480), empty.as_ptr());
    check!(IsWindowEnabled(GetDlgItem(dlg, 1)) == 0, "Suivant grise sans texte");
    SendMessageW(dlg, 0x0111, 2, 0); // Annuler
    check!(
        FIND_COUNT == 3 && FIND_FLAGS & FR_DIALOGTERM != 0 && FIND_FLAGS & FR_FINDNEXT == 0,
        "fermeture : FR_DIALOGTERM"
    );
    check!(IsWindow(dlg) == 0, "boite detruite apres FR_DIALOGTERM");

    // Remplacer, avec procédure crochet.
    let mut with = [0u16; 64];
    fr.flags = FR_ENABLEHOOK;
    fr.hook = find_hook as *const () as usize;
    fr.replace = with.as_mut_ptr();
    fr.replace_len = 64;
    let dlg = ReplaceTextW(&mut fr);
    check!(!dlg.is_null(), "ReplaceTextW ouvre la boite non modale");
    if dlg.is_null() {
        return;
    }
    check!(
        HOOK_INIT == &fr as *const FindReplace as isize,
        "crochet : WM_INITDIALOG avec la FINDREPLACE"
    );
    let by = w(b"Ersatz\0");
    SetWindowTextW(GetDlgItem(dlg, 0x480), typed.as_ptr());
    SetWindowTextW(GetDlgItem(dlg, 0x481), by.as_ptr());
    SendMessageW(dlg, 0x0111, 0x400, 0); // Remplacer
    check!(
        FIND_FLAGS & FR_REPLACE != 0 && FIND_FLAGS & FR_DOWN != 0 && same(&with, &by),
        "Remplacer : FR_REPLACE et texte de remplacement"
    );
    SendMessageW(dlg, 0x0111, 0x401, 0); // Remplacer tout
    check!(
        FIND_FLAGS & FR_REPLACEALL != 0 && FIND_FLAGS & FR_REPLACE == 0,
        "Remplacer tout : FR_REPLACEALL"
    );
    SendMessageW(dlg, 0x0111, 2, 0);
    check!(IsWindow(dlg) == 0, "boite Remplacer fermee");

    // ANSI.
    let mut ansi = *b"abc\0\0\0\0\0\0\0\0\0\0\0\0\0";
    fr.flags = 0;
    fr.hook = 0;
    fr.find = ansi.as_mut_ptr().cast();
    fr.find_len = 16;
    let dlg = FindTextA(&mut fr);
    check!(!dlg.is_null(), "FindTextA");
    if !dlg.is_null() {
        SetWindowTextW(GetDlgItem(dlg, 0x480), typed.as_ptr());
        SendMessageW(dlg, 0x0111, 1, 0);
        check!(&ansi[..7] == b"Winlux\0", "FindTextA : tampon ANSI");
        SendMessageW(dlg, 0x0111, 2, 0);
    }

    // Tampon absent : NULL et FRERR_BUFFERLENGTHZERO.
    fr.find_len = 0;
    check!(
        FindTextW(&mut fr).is_null() && CommDlgExtendedError() == 0x4001,
        "tampon vide : FRERR_BUFFERLENGTHZERO"
    );
    FIND_MSG = 0;
}

/// Valide la boîte affichée comme un utilisateur : touche Entrée.
unsafe extern "win64" fn press_enter(hwnd: Handle, _msg: u32, id: usize, _time: u32) {
    KillTimer(hwnd, id);
    PostMessageW(GetFocus(), 0x0100, 0x0D, 0);
}

/// Avec affichage : Entrée après SHOW_MS millisecondes.
unsafe fn arm() {
    if SHOW_MS > 0 {
        SetTimer(OWNER, 7, SHOW_MS, press_enter as *const () as usize);
    }
}

fn w<const N: usize>(s: &[u8; N]) -> [u16; N] {
    let mut out = [0u16; N];
    for (o, &b) in out.iter_mut().zip(s.iter()) {
        *o = b as u16;
    }
    out
}

fn len(s: &[u16]) -> usize {
    s.iter().position(|&c| c == 0).unwrap_or(s.len())
}

fn ends_with(s: &[u16], tail: &[u8]) -> bool {
    let n = len(s);
    n >= tail.len()
        && s[n - tail.len()..n]
            .iter()
            .zip(tail)
            .all(|(&a, &b)| a == b as u16)
}

unsafe fn ofn(file: &mut [u16], dir: *const u16, filter: *const u16) -> OpenFileName {
    OpenFileName {
        size: core::mem::size_of::<OpenFileName>() as u32,
        owner: OWNER,
        instance: core::ptr::null_mut(),
        filter,
        custom_filter: core::ptr::null_mut(),
        max_cust_filter: 0,
        filter_index: 1,
        file: file.as_mut_ptr(),
        max_file: file.len() as u32,
        file_title: core::ptr::null_mut(),
        max_file_title: 0,
        initial_dir: dir,
        title: core::ptr::null(),
        flags: 0,
        file_offset: 0,
        file_extension: 0,
        def_ext: core::ptr::null(),
        cust_data: 0,
        hook: 0,
        template: core::ptr::null(),
        reserved_ptr: core::ptr::null_mut(),
        reserved: 0,
        flags_ex: 0,
    }
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let mut wait = [0u8; 16];
        let n = GetEnvironmentVariableA(
            c"WINLUX_CONTROLS_SHOW".as_ptr(),
            wait.as_mut_ptr().cast(),
            16,
        );
        for &b in &wait[..n as usize] {
            if b.is_ascii_digit() {
                SHOW_MS = SHOW_MS * 10 + (b - b'0') as u32;
            }
        }
        let class = c"WinluxCommdlgSuite";
        let wc = WndClassExA {
            size: core::mem::size_of::<WndClassExA>() as u32,
            style: 0,
            wnd_proc: wnd_proc as *const () as usize,
            cls_extra: 0,
            wnd_extra: 0,
            instance: core::ptr::null_mut(),
            icon: core::ptr::null_mut(),
            cursor: core::ptr::null_mut(),
            background: GetSysColorBrush(15),
            menu_name: core::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: core::ptr::null_mut(),
        };
        RegisterClassExA(&wc);
        OWNER = CreateWindowExA(
            0,
            class.as_ptr(),
            c"PuTTY".as_ptr(),
            0x00CF_0000,
            40,
            40,
            300,
            200,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!OWNER.is_null(), "fenetre proprietaire");

        // Fichier de test dans %TEMP%.
        let mut temp = [0u16; 260];
        let tl = GetTempPathW(260, temp.as_mut_ptr()) as usize;
        check!(tl > 0, "GetTempPathW");
        let name = b"winlux-cle-test.ppk";
        let mut path = [0u16; 300];
        path[..tl].copy_from_slice(&temp[..tl]);
        for (i, &b) in name.iter().enumerate() {
            path[tl + i] = b as u16;
        }
        let h = CreateFileW(
            path.as_ptr(),
            0x4000_0000,
            0,
            core::ptr::null_mut(),
            2,
            0x80,
            core::ptr::null_mut(),
        );
        CloseHandle(h);

        let filter = w(b"Cle privee PuTTY (*.ppk)\0*.ppk\0Tous les fichiers (*.*)\0*.*\0\0");
        let env_file = w(b"WINLUX_FILEDLG\0");
        let env_value = w(b"winlux-cle-test.ppk\0");
        SetEnvironmentVariableW(env_file.as_ptr(), env_value.as_ptr());

        // « Ouvrir » : clé privée qui doit exister.
        let mut file = [0u16; 300];
        for (i, &b) in name.iter().enumerate() {
            file[i] = b as u16;
        }
        let mut o = ofn(&mut file, temp.as_ptr(), filter.as_ptr());
        o.flags = 0x1000 | 0x800; // FILEMUSTEXIST | PATHMUSTEXIST
        arm();
        check!(GetOpenFileNameW(&mut o) == 1, "GetOpenFileNameW");
        check!(ends_with(&file, b"\\winlux-cle-test.ppk"), "chemin choisi");
        check!(file[1] == b':' as u16, "chemin Windows (X:)");
        check!(
            o.file_offset as usize == len(&file) - name.len(),
            "nFileOffset"
        );
        check!(
            o.file_extension as usize == len(&file) - 3,
            "nFileExtension"
        );

        // Fichier absent avec FILEMUSTEXIST : la boîte refuse puis s'annule.
        if SHOW_MS == 0 {
            let missing = w(b"absent-introuvable.ppk\0");
            SetEnvironmentVariableW(env_file.as_ptr(), missing.as_ptr());
            let mut file2 = [0u16; 300];
            let mut o2 = ofn(&mut file2, temp.as_ptr(), filter.as_ptr());
            o2.flags = 0x1000;
            check!(GetOpenFileNameW(&mut o2) == 0, "fichier absent refuse");
            check!(CommDlgExtendedError() == 0, "annulation sans erreur");
            SetEnvironmentVariableW(env_file.as_ptr(), env_value.as_ptr());

            // Tampon trop petit : FNERR_BUFFERTOOSMALL.
            let mut small = [0u16; 8];
            let mut o3 = ofn(&mut small, temp.as_ptr(), filter.as_ptr());
            check!(GetOpenFileNameW(&mut o3) == 0, "tampon trop petit");
            check!(CommDlgExtendedError() == 0x3003, "FNERR_BUFFERTOOSMALL");

            // Version ANSI.
            let mut file_a = [0u8; 300];
            let mut oa = ofn(&mut [0u16; 1], core::ptr::null(), core::ptr::null());
            oa.file = file_a.as_mut_ptr().cast();
            oa.max_file = 300;
            let mut temp_a = [0u8; 261];
            for i in 0..tl {
                temp_a[i] = temp[i] as u8;
            }
            oa.initial_dir = temp_a.as_ptr().cast();
            check!(GetOpenFileNameA(&mut oa) == 1, "GetOpenFileNameA");
            let n = file_a.iter().position(|&c| c == 0).unwrap_or(0);
            check!(
                file_a[..n].ends_with(b"\\winlux-cle-test.ppk"),
                "chemin ANSI"
            );
        }

        // « Enregistrer sous » : extension par défaut ajoutée.
        let log_name = w(b"session-putty\0");
        SetEnvironmentVariableW(env_file.as_ptr(), log_name.as_ptr());
        let mut out = [0u16; 300];
        out[..log_name.len()].copy_from_slice(&log_name);
        let log_filter = w(b"Journaux (*.log)\0*.log\0\0");
        let def = w(b"log\0");
        let mut s = ofn(&mut out, temp.as_ptr(), log_filter.as_ptr());
        s.def_ext = def.as_ptr();
        s.flags = 0x2; // OVERWRITEPROMPT
        arm();
        check!(GetSaveFileNameW(&mut s) == 1, "GetSaveFileNameW");
        check!(
            ends_with(&out, b"\\session-putty.log"),
            "extension par defaut"
        );

        // Boîtes COM de Vista (Notepad++ 8, #699).
        if SHOW_MS == 0 {
            let open = w(b"winlux-cle-test.ppk\0");
            SetEnvironmentVariableW(env_file.as_ptr(), open.as_ptr());
            com_dialog(&temp, false);
            let save = w(b"session-com\0");
            SetEnvironmentVariableW(env_file.as_ptr(), save.as_ptr());
            com_dialog(&temp, true);
        }

        // Couleur.
        let env = w(b"WINLUX_CHOOSECOLOR\0");
        let val = w(b"FF8000\0");
        SetEnvironmentVariableW(env.as_ptr(), val.as_ptr());
        let mut custom = [0x00FF_FFFFu32; 16];
        let mut cc = ChooseColor {
            size: core::mem::size_of::<ChooseColor>() as u32,
            owner: OWNER,
            instance: core::ptr::null_mut(),
            result: 0x0000_80FF,
            custom: custom.as_mut_ptr(),
            flags: 0x1 | 0x2, // RGBINIT | FULLOPEN
            cust_data: 0,
            hook: 0,
            template: core::ptr::null(),
        };
        arm();
        check!(ChooseColorW(&mut cc) == 1, "ChooseColorW");
        check!(cc.result == 0x0000_80FF, "couleur choisie (COLORREF)");

        // Police à espacement fixe, comme PuTTY.
        let env = w(b"WINLUX_CHOOSEFONT\0");
        let val = w(b",12,gras\0");
        SetEnvironmentVariableW(env.as_ptr(), val.as_ptr());
        let mut lf: LogFontW = core::mem::zeroed();
        lf.height = -13;
        lf.weight = 400;
        let face = w(b"Courier New\0");
        lf.face[..face.len()].copy_from_slice(&face);
        let mut cf = ChooseFont {
            size: core::mem::size_of::<ChooseFont>() as u32,
            owner: OWNER,
            dc: core::ptr::null_mut(),
            logfont: &mut lf,
            point_size: 0,
            flags: 0x4000 | 0x10000 | 0x40 | 0x1, // FIXEDPITCHONLY | FORCEFONTEXIST | INITTOLOGFONTSTRUCT | SCREENFONTS
            colors: 0,
            cust_data: 0,
            hook: 0,
            template: core::ptr::null(),
            instance: core::ptr::null_mut(),
            style: core::ptr::null_mut(),
            font_type: 0,
            size_min: 0,
            size_max: 0,
        };
        arm();
        check!(ChooseFontW(&mut cf) == 1, "ChooseFontW");
        if SHOW_MS == 0 {
            check!(cf.point_size == 120, "iPointSize 12 pt");
            check!(lf.height == -16, "lfHeight pour 12 pt");
            check!(lf.weight == 700, "gras");
        } else {
            check!(cf.point_size == 100, "iPointSize 10 pt");
        }
        check!(lf.face[0] != 0, "nom de police rendu");

        DeleteFileW(path.as_ptr());
        if SHOW_MS == 0 {
            find_replace();
        }
        ExitProcess(if FAILURES == 0 { 0 } else { 1 });
    }
}
