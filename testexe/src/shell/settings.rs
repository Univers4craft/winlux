//! Paramètres Linux/Windows : navigation Mica et commandes système réelles.
//! Les actions passent par winlux-control et les états viennent du système.
#![deny(unreachable_patterns)]

use crate::gfx::*;
use crate::locale::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

#[path = "printers.rs"]
mod printers;
#[path = "settings_gpu.rs"]
mod graphics;
/// Page « À propos » (logo, version, caractéristiques de l'appareil).
#[path = "about.rs"]
mod about;
#[path = "settings_display.rs"]
mod screens;
#[path = "settings_input.rs"]
mod input;

pub const CUSTOM_TITLE: bool = false;
pub const PAGE_SYSUPDATE: usize = 17;
pub const PAGE_PRIVACY: usize = 18;
pub const PAGE_CPU: usize = 19;
/// Page « Lecteurs réseau » (FTP, SFTP, SMB… montés comme un disque).
pub const PAGE_NETDRIVES: usize = 20;
pub const PAGE_PERSONALIZE: usize = 21;
/// Page « Bluetooth et appareils » (ouverte depuis la barre des tâches).
pub const BLUETOOTH_PAGE: usize = 22;
/// Page « Alimentation et batterie » (icône batterie de la barre des tâches).
pub const PAGE_POWER: usize = 23;
/// Graphismes : GPU, mesures, réglages avancés et overlay en jeu.
pub const PAGE_GRAPHICS: usize = 24;
pub const PAGE_REPORT: usize = 25;
/// Page « Imprimantes et scanners » (winspool + CUPS).
pub const PAGE_PRINTERS: usize = 26;
/// Page « Bureau à distance » : état publié par winlux-remote-config.
const PAGE_REMOTE: usize = 27;
/// Page « Partage et transfert » : dossiers partagés (SMB) et transfert des
/// réglages par fichier (shell/sharing.rs).
pub const PAGE_SHARING: usize = 28;
const PAGE_COUNT: usize = 29;
/// Appareils Bluetooth affichés (une ligne de boutons chacun).
const BT_ROWS: usize = 5;
/// Ordre d'affichage du menu : la personnalisation suit l'apparence,
/// l'alimentation suit le son et la batterie.
const NAV_ORDER: [usize; PAGE_COUNT] = [
    0, 1, 2, 3, 23, 4, 5, 6, 21, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 24, 20, 28, 22, 25, 26,
    27,
];
fn pages() -> [&'static [u8]; PAGE_COUNT] {
    [
        tr(S_SYSTEM_LINUX),
        tr(S_APPS_WINDOWS),
        tr(S_NETWORK),
        tr(S_SOUND_BATTERY),
        tr(S_KEYBOARD_TOUCHPAD),
        tr(S_MOUSE),
        tr(S_APPEARANCE),
        tr(S_UPDATES),
        tr(S_DISPLAY),
        tr(S_ABOUT),
        tr(S_EXECUTABLE),
        tr(S_BROWSERS),
        tr(S_ACCESSIBILITY),
        tr(S_VISUAL_EFFECTS),
        tr(S_LOCALIZATION),
        tr(S_FILE_MANAGER),
        tr(S_ADVANCED_SOUND),
        tr(S_SYSTEM_UPDATES),
        tr(S_PRIVACY),
        tr(S_PROCESSOR),
        tr(S_NETWORK_DRIVES),
        tr(S_PERSONALIZATION),
        tr(S_BLUETOOTH_DEVICES),
        tr(S_POWER_BATTERY),
        tr(S_GRAPHICS),
        tr(S_REPORT_PAGE),
        tr(S_PRINTERS),
        tr(S_REMOTE_DESKTOP),
        tr(S_SHARING),
    ]
}
const CAPACITY: usize = 32768;
#[derive(Clone, Copy)]
struct Buffer {
    bytes: [u8; CAPACITY],
    len: usize,
}
const EMPTY: Buffer = Buffer {
    bytes: [0; CAPACITY],
    len: 0,
};
impl Buffer {
    fn get(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
static mut STATUS: Buffer = EMPTY;
static mut ACTION: Buffer = EMPTY;
static mut LOG: Buffer = EMPTY;
static mut UPDATES: Buffer = EMPTY;
static mut PROGRESS: Buffer = EMPTY;
static mut REMOTE: Buffer = EMPTY;
static mut BUILD_INFO: Buffer = EMPTY;
static mut SYSUPDATES: Buffer = EMPTY;
static mut BLUETOOTH: Buffer = EMPTY;
static mut BT_CMD: [u8; 64] = [0; 64];
/// `/run/winlux/power.txt`, écrit par `winlux --power` (démon et commandes).
static mut POWER: Buffer = EMPTY;
/// Nombre de mises à jour système déjà signalé (u32::MAX : pas encore lu).
static mut NOTIFIED_UPDATES: u32 = u32::MAX;
static mut LAST_ANIMATION: u32 = 0;
static mut PAGE: usize = 0;
static mut SCROLL: usize = 0;
static mut LAST_POLL: u32 = 0;
static mut HOVER: i32 = -1;
static mut SHOW_LOG: bool = false;
static mut EXE_PATH: [u16; 2048] = [0; 2048];
static mut EXE_LEN: usize = 0;
static mut EXE_SCALE: usize = 0;
static mut EXE_TRACE: bool = false;
/// GPU préféré du programme : 0 auto, 1 intégré, 2 dédié (machines hybrides).
static mut EXE_GPU: usize = 0;
const EXE_GPUS: [&[u8]; 3] = [b"auto", b"integrated", b"dedicated"];
static mut EXE_NOTICE: &Tr4 = &[b"Les r\xE9glages sont facultatifs et propres \xE0 ce programme.", b"Settings are optional and specific to this program.", b"Los ajustes son opcionales y propios de este programa.", b"Die Einstellungen sind optional und gelten nur f\xFCr dieses Programm."];
const EXE_SCALES: [&[u8]; 5] = [b"auto", b"1", b"1.25", b"1.5", b"2"];
static mut MOUSE_SPEED: u32 = 50;
static mut LOCALE_KB_SCROLL: usize = 0;
static mut LOCALE_TZ_SCROLL: usize = 0;
use crate::gfx::{AUTO_THEME, ENABLE_BLUR, ENABLE_SMOOTH_ANIM, ENABLE_THUMBNAILS};

fn exe_profile_path(out: &mut [u16; 2060]) {
    unsafe {
        out[..EXE_LEN].copy_from_slice(&EXE_PATH[..EXE_LEN]);
        for (i, &byte) in b".winlux.ini".iter().enumerate() {
            out[EXE_LEN + i] = byte as u16;
        }
    }
}
pub unsafe fn open_exe_settings(path: &[u16]) {
    let len = path.iter().position(|&v| v == 0).unwrap_or(path.len());
    if len == 0 || len >= EXE_PATH.len() {
        return;
    }
    EXE_PATH.fill(0);
    EXE_PATH[..len].copy_from_slice(&path[..len]);
    EXE_LEN = len;
    EXE_SCALE = 0;
    EXE_TRACE = false;
    EXE_GPU = 0;
    crate::settings_cpu::reset_program();
    let mut profile = [0u16; 2060];
    exe_profile_path(&mut profile);
    let h = CreateFileW(
        profile.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if !h.is_null() && h as isize != -1 {
        let mut data = [0u8; 1024];
        let mut read = 0;
        ReadFile(h, data.as_mut_ptr(), 1024, &mut read, core::ptr::null_mut());
        CloseHandle(h);
        let data = &data[..(read as usize).min(1024)];
        crate::settings_cpu::load_program(data);
        EXE_SCALE = EXE_SCALES
            .iter()
            .position(|&v| v == field(data, b"ui_scale"))
            .unwrap_or(0);
        EXE_TRACE = field(data, b"trace") == b"1";
        EXE_GPU = EXE_GPUS
            .iter()
            .position(|&v| v == field(data, b"gpu"))
            .unwrap_or(0);
    }
    EXE_NOTICE = &[b"Un double-clic lance le programme, m\xEAme sans r\xE9glages enregistr\xE9s.", b"A double-click starts the program, even without saved settings.", b"Un doble clic inicia el programa, incluso sin ajustes guardados.", b"Ein Doppelklick startet das Programm, auch ohne gespeicherte Einstellungen."];
    PAGE = 10;
    HOVER = -1;
    SCROLL = 0;
    crate::wm::open(crate::wm::APP_SETTINGS);
}
unsafe fn save_exe_settings(reset: bool) {
    let mut path = [0u16; 2060];
    exe_profile_path(&mut path);
    if reset {
        let attributes = GetFileAttributesW(path.as_ptr());
        if attributes != u32::MAX && DeleteFileW(path.as_ptr()) == 0 {
            EXE_NOTICE = &[b"Impossible de supprimer les r\xE9glages : v\xE9rifiez les permissions.", b"Unable to delete the settings: check the permissions.", b"No se pueden eliminar los ajustes: compruebe los permisos.", b"Die Einstellungen k\xF6nnen nicht gel\xF6scht werden: Pr\xFCfen Sie die Berechtigungen."];
            return;
        }
        EXE_SCALE = 0;
        EXE_TRACE = false;
        EXE_GPU = 0;
        crate::settings_cpu::reset_program();
        EXE_NOTICE = &[b"Valeurs par d\xE9faut restaur\xE9es.", b"Defaults restored.", b"Valores predeterminados restaurados.", b"Standardwerte wiederhergestellt."];
        return;
    }
    let h = CreateFileW(
        path.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        EXE_NOTICE = &[b"Enregistrement impossible : le dossier du programme doit \xEAtre accessible en \xE9criture.", b"Unable to save: the program folder must be writable.", b"No se puede guardar: la carpeta del programa debe admitir escritura.", b"Speichern nicht m\xF6glich: Der Programmordner muss beschreibbar sein."];
        return;
    }
    let mut data = [0u8; 512];
    let scale = EXE_SCALES[EXE_SCALE];
    data[..9].copy_from_slice(b"ui_scale=");
    data[9..9 + scale.len()].copy_from_slice(scale);
    let mut n = 9 + scale.len();
    data[n..n + 7].copy_from_slice(b"\ntrace=");
    data[n + 7] = if EXE_TRACE { b'1' } else { b'0' };
    data[n + 8] = b'\n';
    n += 9;
    // Le choix du GPU n'est écrit que s'il diffère de l'automatique.
    if EXE_GPU != 0 {
        let gpu = EXE_GPUS[EXE_GPU.min(2)];
        data[n..n + 4].copy_from_slice(b"gpu=");
        data[n + 4..n + 4 + gpu.len()].copy_from_slice(gpu);
        data[n + 4 + gpu.len()] = b'\n';
        n += 5 + gpu.len();
    }
    let total = n + crate::settings_cpu::write_program(&mut data[n..]);
    let mut written = 0;
    let ok = WriteFile(
        h,
        data.as_ptr(),
        total as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    EXE_NOTICE = if ok != 0 && written == total as u32 {
        &[b"R\xE9glages enregistr\xE9s pour le prochain lancement.", b"Settings saved for the next launch.", b"Ajustes guardados para el pr\xF3ximo inicio.", b"Einstellungen f\xFCr den n\xE4chsten Start gespeichert."]
    } else {
        &[b"Enregistrement incomplet : r\xE9essayez.", b"Save incomplete: try again.", b"Guardado incompleto: int\xE9ntelo de nuevo.", b"Speichern unvollst\xE4ndig: Versuchen Sie es erneut."]
    };
}

unsafe fn read(path: &core::ffi::CStr, target: &mut Buffer) -> bool {
    let file = CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    let mut raw = [0u8; CAPACITY];
    let mut count = 0;
    if file as isize != -1 && !file.is_null() {
        let size = GetFileSize(file, core::ptr::null_mut());
        // Garder le début des notes (version/résumé), la fin des journaux.
        if path.to_bytes().ends_with(b".log") && size != u32::MAX && size > (CAPACITY - 1) as u32 {
            SetFilePointer(
                file,
                (size - (CAPACITY - 1) as u32) as i32,
                core::ptr::null_mut(),
                0,
            );
        }
        ReadFile(
            file,
            raw.as_mut_ptr(),
            (CAPACITY - 1) as u32,
            &mut count,
            core::ptr::null_mut(),
        );
        CloseHandle(file);
    }
    let mut next = EMPTY;
    // Les journaux Linux sont UTF-8, TextOutA attend Windows-1252.
    // Une fin de journal peut commencer au milieu d'un caractere UTF-8.
    let mut begin = 0;
    while begin < count as usize && raw[begin] & 0xc0 == 0x80 {
        begin += 1;
    }
    let source = core::str::from_utf8(&raw[begin..count as usize]).unwrap_or("");
    for ch in source.chars().take(CAPACITY - 1) {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        next.bytes[next.len] = match ch as u32 {
            0..=255 => ch as u8,
            0x2019 => b'\'',
            0x2013 | 0x2014 => b'-',
            0x20ac => 0x80,
            0x153 => 0x9c,
            _ => b'?',
        };
        if ch == '\t' {
            next.bytes[next.len] = b' ';
        }
        next.len += 1;
    }
    let changed = next.len != target.len || next.get() != target.get();
    *target = next;
    changed
}
/// Vrai si l'interface est en anglais (Paramètres > Langue).
pub unsafe fn english() -> bool {
    CURRENT_LANG == Lang::En
}

unsafe fn value(key: &[u8]) -> &'static [u8] {
    field(STATUS.get(), key)
}
unsafe fn pvalue(key: &[u8]) -> &'static [u8] {
    field(POWER.get(), key)
}
pub(crate) unsafe fn exe_path() -> &'static [u16] {
    &EXE_PATH[..EXE_LEN]
}
pub(crate) unsafe fn exe_notice() -> &'static [u8] {
    tr4(EXE_NOTICE)
}
pub(crate) unsafe fn save_exe_settings_pub(reset: bool) {
    save_exe_settings(reset);
}
pub(crate) unsafe fn launch_exe() {
    let verb = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
    if EXE_LEN > 0
        && ShellExecuteW(
            core::ptr::null_mut(),
            verb.as_ptr(),
            EXE_PATH.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            1,
        ) as isize
            <= 32
    {
        EXE_NOTICE = &[b"Lancement impossible : consultez le journal de lancement Winlux.", b"Unable to start: see the Winlux launch log.", b"No se puede iniciar: consulte el registro de inicio de Winlux.", b"Start nicht m\xF6glich: Siehe Winlux-Startprotokoll."];
    }
}
pub(crate) fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        if line.len() > key.len() && &line[..key.len()] == key && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}
unsafe fn available() -> bool {
    value(b"control_available") == b"1"
}
pub unsafe fn service_available() -> bool {
    available()
}
pub unsafe fn status_value(key: &[u8]) -> &'static [u8] {
    value(key)
}
unsafe fn sysupdate_busy() -> bool {
    value(b"sysupdate_busy") == b"1"
}
unsafe fn updating() -> bool {
    // The Linux helper checks the owner PID; a stale progress file must never
    // lock the interface forever after a killed updater.
    value(b"update_busy") == b"1"
}
unsafe fn already_up_to_date() -> bool {
    UPDATES
        .get()
        .split(|&c| c == b'\n')
        .any(|line| line == b"Winlux est \xE0 jour.")
}
unsafe fn restart_required() -> bool {
    matches!(value(b"update_action"), b"reboot" | b"restart-session")
}
fn restart_rect(c: &Rect) -> Rect {
    rect(c.left, c.top + 108, c.right, c.top + 156)
}
pub(crate) fn number(s: &[u8]) -> u32 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u32, |n, &c| {
            n.saturating_mul(10).saturating_add((c - b'0') as u32)
        })
}

/// Telemetrie reelle pour la barre, meme quand Parametres est ferme.
pub unsafe fn system_tick() -> bool {
    let now = GetTickCount();
    if LAST_POLL != 0 && now.wrapping_sub(LAST_POLL) < 250 {
        return false;
    }
    LAST_POLL = now;
    let mut changed = read(c"Z:\\run\\winlux\\control.txt", &mut STATUS);
    if GetFileAttributesA(c"Z:\\run\\winlux\\browser-chooser".as_ptr()) != u32::MAX {
        DeleteFileA(c"Z:\\run\\winlux\\browser-chooser".as_ptr());
        open_page(11);
        changed = true;
    }
    changed |= crate::netdrives::poll(
        PAGE == PAGE_NETDRIVES && crate::wm::is_open(crate::wm::APP_SETTINGS),
    );
    changed |=
        crate::sharing::poll(PAGE == PAGE_SHARING && crate::wm::is_open(crate::wm::APP_SETTINGS));
    changed |= read(c"Z:\\run\\winlux\\action.txt", &mut ACTION);
    changed |= read(
        if PAGE == 7 {
            c"Z:\\run\\winlux\\update.log"
        } else if PAGE == PAGE_SYSUPDATE {
            c"Z:\\run\\winlux\\sysupdate.log"
        } else {
            c"Z:\\run\\winlux\\action.log"
        },
        &mut LOG,
    );
    changed |= read(c"Z:\\run\\winlux\\updates.txt", &mut UPDATES);
    changed |= read(c"Z:\\run\\winlux\\bluetooth.txt", &mut BLUETOOTH);
    crate::desktop::BLUETOOTH_STATE = if field(BLUETOOTH.get(), b"powered") != b"1" {
        0
    } else if number(field(BLUETOOTH.get(), b"connected")) > 0 {
        2
    } else {
        1
    };
    changed |= read(c"Z:\\run\\winlux\\update-progress", &mut PROGRESS);
    if PAGE == PAGE_REMOTE {
        changed |= read(c"Z:\\run\\winlux\\remote.txt", &mut REMOTE);
    }
    if PAGE == PAGE_GRAPHICS {
        changed |= crate::gpuinfo::refresh(false);
    }
    if PAGE == 9 {
        changed |= read(c"Z:\\usr\\share\\winlux\\BUILD-INFO", &mut BUILD_INFO);
    }
    if PAGE == PAGE_SYSUPDATE {
        changed |= read(c"Z:\\run\\winlux\\sysupdates.txt", &mut SYSUPDATES);
    }
    if PAGE == PAGE_POWER {
        changed |= read(c"Z:\\run\\winlux\\power.txt", &mut POWER);
    }
    if available() {
        notify_updates();
    }
    if available() {
        crate::desktop::NETWORK_CONNECTED = number(value(b"network_connected"));
        let audio = value(b"audio");
        if audio.starts_with(b"Volume: ") {
            let muted = audio.windows(5).any(|w| w == b"MUTED");
            let v = &audio[8..];
            let n = if v.starts_with(b"1") {
                100
            } else if v.len() >= 4 {
                number(&v[2..4])
            } else {
                0
            };
            crate::desktop::VOLUME_LEVEL = if muted { 0 } else { n };
        } else if audio.first().is_some_and(u8::is_ascii_digit) {
            crate::desktop::VOLUME_LEVEL = if audio.windows(3).any(|w| w == b"off") {
                0
            } else {
                number(audio).min(100)
            };
        } else {
            crate::desktop::VOLUME_LEVEL = 0;
        }
        match value(b"align") {
            b"left" => crate::desktop::ALIGN_LEFT = true,
            b"center" => crate::desktop::ALIGN_LEFT = false,
            _ => {}
        }
        match value(b"theme") {
            b"light" => crate::gfx::set_theme(false),
            b"dark" => crate::gfx::set_theme(true),
            _ => {}
        }
        crate::gfx::check_auto_theme();
    }
    changed
}
/// Relaie un réglage au service système quand il existe, sans message sinon
/// (la personnalisation est déjà enregistrée côté bureau).
pub unsafe fn control_quiet(action: &core::ffi::CStr) {
    if available() {
        control(action);
    }
}
/// Notification quand de nouvelles mises à jour système apparaissent.
unsafe fn notify_updates() {
    let count = number(value(b"sysupdate_count"));
    if !crate::login::logged_in() || count == NOTIFIED_UPDATES {
        return;
    }
    let first = NOTIFIED_UPDATES == u32::MAX;
    NOTIFIED_UPDATES = count;
    if count == 0 || first && value(b"sysupdate_mode") == b"auto" {
        return;
    }
    let mut msg = [0u8; 96];
    let mut n = push_number(&mut msg, 0, count);
    n = push(
        &mut msg,
        n,
        if count == 1 {
            tr4(&[b" mise \xE0 jour syst\xE8me disponible - Param\xE8tres > Mises \xE0 jour syst\xE8me", b" system update available - Settings > System updates", b" actualizaci\xF3n del sistema disponible - Configuraci\xF3n > Actualizaciones del sistema", b" Systemupdate verf\xFCgbar - Einstellungen > Systemupdates"])
        } else {
            tr4(&[b" mises \xE0 jour syst\xE8me disponibles - Param\xE8tres > Mises \xE0 jour syst\xE8me", b" system updates available - Settings > System updates", b" actualizaciones del sistema disponibles - Configuraci\xF3n > Actualizaciones del sistema", b" Systemupdates verf\xFCgbar - Einstellungen > Systemupdates"])
        },
    );
    crate::desktop::toast(&msg[..n]);
}
fn push(buf: &mut [u8], at: usize, s: &[u8]) -> usize {
    let n = s.len().min(buf.len().saturating_sub(at));
    buf[at..at + n].copy_from_slice(&s[..n]);
    at + n
}
fn push_number(buf: &mut [u8], at: usize, v: u32) -> usize {
    let mut digits = [0u8; 10];
    let mut i = digits.len();
    let mut v = v;
    loop {
        i -= 1;
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    push(buf, at, &digits[i..])
}
/// Lignes `device=MAC|nom|type|appairé|connecté` de bluetooth.txt.
fn bt_device(data: &[u8], index: usize) -> Option<&[u8]> {
    data.split(|&c| c == b'\n')
        .filter_map(|l| l.strip_prefix(b"device="))
        .nth(index)
}
fn bt_field(dev: &[u8], i: usize) -> &[u8] {
    dev.split(|&c| c == b'|').nth(i).unwrap_or(b"")
}
/// Commande `bluetooth-<verbe> <MAC>` pour winlux-control.
unsafe fn bt_command(verb: &[u8], mac: &[u8]) -> Option<&'static core::ffi::CStr> {
    let buf = &mut *core::ptr::addr_of_mut!(BT_CMD);
    let n = verb.len() + 1 + mac.len();
    if mac.len() != 17 || n >= buf.len() {
        return None;
    }
    buf[..verb.len()].copy_from_slice(verb);
    buf[verb.len()] = b' ';
    buf[verb.len() + 1..n].copy_from_slice(mac);
    buf[n] = 0;
    core::ffi::CStr::from_bytes_with_nul(&buf[..=n]).ok()
}
/// Action du bouton principal d'un appareil : appairer, connecter, déconnecter.
fn bt_primary(dev: &[u8]) -> (&'static [u8], &'static [u8]) {
    match (bt_field(dev, 3), bt_field(dev, 4)) {
        (b"1", b"1") => (b"bluetooth-disconnect", tr4(&[b"D\xE9connecter", b"Disconnect", b"Desconectar", b"Trennen"])),
        (b"1", _) => (b"bluetooth-connect", tr4(&[b"Connecter", b"Connect", b"Conectar", b"Verbinden"])),
        _ => (b"bluetooth-pair", tr4(&[b"Appairer", b"Pair", b"Emparejar", b"Koppeln"])),
    }
}
unsafe fn draw_bluetooth(g: &mut Gfx, c: &Rect) {
    let bt = BLUETOOTH.get();
    let state: &[u8] = if field(bt, b"present") != b"1" {
        tr4(&[b"Aucun adaptateur Bluetooth d\xE9tect\xE9 (BlueZ).", b"No Bluetooth adapter found (BlueZ).", b"No se detect\xF3 ning\xFAn adaptador Bluetooth (BlueZ).", b"Kein Bluetooth-Adapter erkannt (BlueZ)."])
    } else if field(bt, b"powered") != b"1" {
        tr4(&[b"D\xE9sactiv\xE9", b"Off", b"Desactivado", b"Aus"])
    } else if field(bt, b"discovering") == b"1" {
        tr4(&[b"Activ\xE9 - recherche d'appareils en cours...", b"On - searching for devices...", b"Activado - buscando dispositivos...", b"Ein - Ger\xE4te werden gesucht..."])
    } else {
        tr4(&[b"Activ\xE9 - visible pour les appareils \xE0 proximit\xE9", b"On - visible to nearby devices", b"Activado - visible para dispositivos cercanos", b"Ein - f\xFCr Ger\xE4te in der N\xE4he sichtbar"])
    };
    row(g, c, 44, b"Bluetooth", state);
    button(g, c, 0, 0, tr4(&[b"Activer", b"Turn on", b"Activar", b"Aktivieren"]), 0);
    button(g, c, 0, 1, tr4(&[b"D\xE9sactiver", b"Turn off", b"Desactivar", b"Deaktivieren"]), 1);
    button(g, c, 0, 2, tr4(&[b"Rechercher des appareils", b"Search for devices", b"Buscar dispositivos", b"Nach Ger\xE4ten suchen"]), 2);
    let mut shown = 0;
    for k in 0..BT_ROWS {
        let Some(dev) = bt_device(bt, k) else { break };
        let mut label = [0u8; 96];
        let mut n = 0;
        let kind: &[u8] = match bt_field(dev, 2) {
            b"audio" => b" (audio)",
            b"clavier" => tr4(&[b" (clavier)", b" (keyboard)", b" (teclado)", b" (Tastatur)"]),
            b"souris" => tr4(&[b" (souris)", b" (mouse)", b" (mouse)", b" (Maus)"]),
            b"manette" => tr4(&[b" (manette)", b" (gamepad)", b" (mando)", b" (Gamepad)"]),
            b"telephone" => tr4(&[b" (t\xE9l\xE9phone)", b" (phone)", b" (tel\xE9fono)", b" (Telefon)"]),
            _ => b"",
        };
        let status: &[u8] = if bt_field(dev, 4) == b"1" {
            tr4(&[b" - connect\xE9", b" - connected", b" - conectado", b" - verbunden"])
        } else {
            b""
        };
        for part in [bt_field(dev, 1), kind, status] {
            let take = part.len().min(label.len() - n);
            label[n..n + take].copy_from_slice(&part[..take]);
            n += take;
        }
        let row_i = k as i32 + 1;
        let id = 3 + 3 * k as i32;
        button(g, c, row_i, 0, &label[..n], id);
        button(g, c, row_i, 1, bt_primary(dev).1, id + 1);
        if bt_field(dev, 3) == b"1" {
            button(g, c, row_i, 2, tr4(&[b"Oublier", b"Forget", b"Olvidar", b"Entfernen"]), id + 2);
        }
        shown += 1;
    }
    let y = 108 + (shown.max(1) + 1) * 44 + 16;
    if shown == 0 && field(bt, b"powered") == b"1" {
        row(
            g,
            c,
            108 + 44,
            tr4(&[b"Appareils", b"Devices", b"Dispositivos", b"Ger\xE4te"]),
            tr4(&[b"Aucun appareil : lancez une recherche et mettez l'appareil en mode appairage.", b"No devices: start a search and put the device in pairing mode.", b"Ning\xFAn dispositivo: inicie una b\xFAsqueda y ponga el dispositivo en modo de emparejamiento.", b"Keine Ger\xE4te: Starten Sie eine Suche und versetzen Sie das Ger\xE4t in den Kopplungsmodus."]),
        );
    }
    let mut summary = [0u8; 96];
    let mut n = 0;
    for part in [
        tr4(&[b"Manettes : ", b"Gamepads: ", b"Mandos: ", b"Gamepads: "]),
        value(b"gamepads"),
        tr4(&[b"   Webcams : ", b"   Webcams: ", b"   C\xE1maras web: ", b"   Webcams: "]),
        value(b"cameras"),
    ] {
        let take = part.len().min(summary.len() - n);
        summary[n..n + take].copy_from_slice(&part[..take]);
        n += take;
    }
    row(
        g,
        c,
        y,
        tr4(&[b"P\xE9riph\xE9riques des programmes Windows", b"Windows program devices", b"Dispositivos de los programas de Windows", b"Ger\xE4te der Windows-Programme"]),
        &summary[..n],
    );
    row(g, c, y + 60, tr4(&[b"Micro par d\xE9faut", b"Default microphone", b"Micr\xF3fono predeterminado", b"Standardmikrofon"]), value(b"microphone"));
    journal(g, c, y + 124, LOG.get());
}
pub unsafe fn control(action: &core::ffi::CStr) -> bool {
    if (updating() || sysupdate_busy())
        && matches!(
            action.to_bytes(),
            b"poweroff" | b"reboot" | b"restart-session" | b"update-apply" | b"update-rollback"
        )
    {
        crate::desktop::toast(tr4(&[b"Installation en cours : attendez la fin avant de red\xE9marrer.", b"Installation in progress: wait until it finishes before restarting.", b"Instalaci\xF3n en curso: espere a que termine antes de reiniciar.", b"Installation l\xE4uft: Warten Sie bis zum Ende, bevor Sie neu starten."]));
        return false;
    }
    if ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-control".as_ptr(),
        action.as_ptr(),
        core::ptr::null(),
        0,
    ) as isize
        > 32
    {
        ACTION = EMPTY;
        let s = tr4(&[b"Operation en cours...", b"Operation in progress...", b"Operaci\xF3n en curso...", b"Vorgang l\xE4uft..."]);
        ACTION.bytes[..s.len()].copy_from_slice(s);
        ACTION.len = s.len();
        LAST_POLL = 0;
        true
    } else {
        crate::desktop::toast(tr4(&[b"Le service de contr\xF4le Winlux n'est pas disponible.", b"The Winlux control service is not available.", b"El servicio de control de Winlux no est\xE1 disponible.", b"Der Winlux-Steuerungsdienst ist nicht verf\xFCgbar."]));
        false
    }
}
pub unsafe fn open_page(page: usize) {
    if crate::wm::native_settings(page) {
        return;
    }
    PAGE = page.min(PAGE_COUNT - 1);
    SCROLL = 0;
    LAST_POLL = 0;
    crate::wm::open(crate::wm::APP_SETTINGS);
    system_tick();
}
fn content(f: &Frame) -> Rect {
    let nav = if f.client.right - f.client.left < 700 {
        156
    } else {
        210
    };
    rect(
        f.client.left + nav + 26,
        f.client.top + 42,
        f.client.right - 26,
        f.client.bottom - 20,
    )
}
fn nav_rect(f: &Frame, i: usize) -> Rect {
    let c = content(f);
    // Toutes les pages doivent tenir dans la hauteur de la fenêtre.
    let step = ((f.client.bottom - f.client.top - 72) / PAGE_COUNT as i32).clamp(20, 42);
    rect(
        f.client.left + 10,
        f.client.top + 64 + i as i32 * step,
        c.left - 30,
        f.client.top + 64 + i as i32 * step + step - 4,
    )
}
fn button_rect(c: &Rect, row: i32, col: i32) -> Rect {
    let w = ((c.right - c.left - 16) / 3).max(70);
    rect(
        c.left + col * (w + 8),
        c.top + 108 + row * 44,
        c.left + col * (w + 8) + w,
        c.top + 144 + row * 44,
    )
}
unsafe fn button(g: &mut Gfx, c: &Rect, row: i32, col: i32, label: &[u8], id: i32) {
    let p = pal();
    let r = button_rect(c, row, col);
    if r.bottom > c.bottom - 24 {
        return;
    }
    let disabled = updating() && (PAGE == 0 || PAGE == 7 && id != 4)
        || PAGE == PAGE_SYSUPDATE && sysupdate_busy() && id >= 18;
    let selected = match (PAGE, id) {
        (5, 0) => unsafe { MOUSE_SPEED == 25 },
        (5, 1) => unsafe { MOUSE_SPEED == 50 },
        (5, 2) => unsafe { MOUSE_SPEED == 75 },
        (6, 0) => value(b"align") == b"left",
        (6, 1) => value(b"align") == b"center",
        (6, 3) => value(b"theme") == b"dark",
        (6, 4) => value(b"theme") == b"light",
        (10, 0..=4) => EXE_SCALE == id as usize,
        (10, 5) => EXE_TRACE,
        (10, 12..=14) => EXE_GPU == (id - 12) as usize,
        (11, 1) => value(b"browser_default") == b"edge",
        (11, 4) => value(b"browser_default") == b"chrome",
        (11, 7) => value(b"browser_default") == b"firefox",
        (14, 0..=3) => (unsafe { CURRENT_LANG } as usize) == id as usize,
        (14, 10..=17) => {
            let code = KEYBOARD_CODES.get((id as usize).wrapping_sub(10));
            code.map_or(false, |c| value(b"layout") == *c)
        }
        (14, 30..=41) => {
            let tz = TZ_IDS.get((id as usize).wrapping_sub(30));
            tz.map_or(false, |t| value(b"timezone") == *t)
        }
        (15, 0) => unsafe { crate::explorer::current_view() == 1 },
        (15, 1) => unsafe { crate::explorer::current_view() == 5 },
        (15, 2) => unsafe { crate::explorer::current_view() == 6 },
        (15, 3) => unsafe { crate::explorer::show_hidden() },
        (15, 4) => unsafe { !crate::explorer::show_hidden() },
        (15, 6) => unsafe { crate::explorer::show_ext() },
        (15, 7) => unsafe { !crate::explorer::show_ext() },
        (PAGE_REMOTE, 0) => remote(b"enabled") == b"1",
        (PAGE_REMOTE, 1) => remote(b"enabled") != b"1",
        (PAGE_REMOTE, 3) => remote(b"view_only") != b"1",
        (PAGE_REMOTE, 4) => remote(b"view_only") == b"1",
        (PAGE_REMOTE, 5) => remote(b"legacy") == b"1",
        (PAGE_REMOTE, 13) => remote(b"port") == b"5900",
        (PAGE_REMOTE, id @ 21..=26) => remote_account(id as usize - 21).is_some_and(|(_, allowed)| allowed),
        (PAGE_SYSUPDATE, 0) => value(b"sysupdate_mode") == b"auto",
        (PAGE_SYSUPDATE, 1) => !matches!(value(b"sysupdate_mode"), b"auto" | b"scheduled"),
        (PAGE_SYSUPDATE, 2) => value(b"sysupdate_mode") == b"scheduled",
        (PAGE_POWER, 0) => pvalue(b"power_profile") == b"performance",
        (PAGE_POWER, 1) => pvalue(b"power_profile") == b"balanced",
        (PAGE_POWER, 2) => pvalue(b"power_profile") == b"saver",
        (PAGE_PRIVACY, 0) => value(b"privacy_telemetry") != b"on",
        (PAGE_PRIVACY, 1) => value(b"privacy_telemetry") == b"on",
        (PAGE_PRIVACY, 9) => value(b"privacy_microphone") != b"deny",
        (PAGE_PRIVACY, 10) => value(b"privacy_camera") != b"deny",
        (PAGE_PRIVACY, 11) => value(b"privacy_location") == b"allow",
        (PAGE_PRIVACY, 18) => value(b"privacy_location_history") != b"on",
        (PAGE_PRIVACY, 19) => value(b"privacy_location_history") == b"on",
        (BLUETOOTH_PAGE, 0) => field(BLUETOOTH.get(), b"powered") == b"1",
        (BLUETOOTH_PAGE, 1) => {
            field(BLUETOOTH.get(), b"present") == b"1" && field(BLUETOOTH.get(), b"powered") != b"1"
        }
        (PAGE_REPORT, 9) => value(b"report_offer") != b"off",
        (PAGE_REPORT, 10) => value(b"report_offer") == b"off",
        (PAGE_REPORT, 18) => value(b"report_redact") != b"off",
        (PAGE_REPORT, 19) => value(b"report_redact") == b"off",
        (PAGE_REPORT, 27) => !matches!(value(b"report_mode"), b"browser" | b"file"),
        (PAGE_REPORT, 28) => value(b"report_mode") == b"browser",
        (PAGE_REPORT, 29) => value(b"report_mode") == b"file",
        _ => false,
    };
    let primary = PAGE == 7 && id == 1
        || PAGE == 8 && id == 2
        || PAGE == PAGE_SYSUPDATE && id == 19 && number(value(b"sysupdate_count")) > 0;
    round_rect(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        7,
        if selected { p.accent } else { p.border },
    );
    round_rect(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        6,
        if disabled {
            p.surface
        } else if primary {
            p.accent
        } else if selected {
            p.selected
        } else if HOVER == id {
            p.hover
        } else {
            p.surface2
        },
    );
    let mut end = label.len();
    while end > 0 && text_width(&label[..end], 11) > r.right - r.left - 12 {
        end -= 1;
    }
    text_centered(
        g.hdc,
        g.f.small,
        11,
        if disabled {
            p.text3
        } else if primary {
            rgb(18, 24, 34)
        } else {
            p.text
        },
        (r.left + r.right) / 2,
        r.top + 12,
        &label[..end],
    );
}
unsafe fn row(g: &mut Gfx, c: &Rect, y: i32, label: &[u8], val: &[u8]) {
    if c.top + y + 42 >= c.bottom - 30 {
        return;
    }
    let p = pal();
    round_rect(
        g.hdc,
        g.b,
        c.left - 8,
        c.top + y - 7,
        c.right + 8,
        c.top + y + 45,
        8,
        p.border,
    );
    round_rect(
        g.hdc,
        g.b,
        c.left - 7,
        c.top + y - 6,
        c.right + 7,
        c.top + y + 44,
        7,
        p.surface,
    );
    let mut label_end = label.len();
    while label_end > 0 && text_width(&label[..label_end], 13) > c.right - c.left {
        label_end -= 1;
    }
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        c.left,
        c.top + y,
        &label[..label_end],
    );
    let mut end = val.len();
    while end > 0 && text_width(&val[..end], 13) > c.right - c.left {
        end -= 1;
    }
    text(
        g.hdc,
        g.f.body,
        p.text2,
        c.left,
        c.top + y + 22,
        if val.is_empty() {
            tr4(&[b"Information indisponible", b"Information unavailable", b"Informaci\xF3n no disponible", b"Information nicht verf\xFCgbar"])
        } else {
            &val[..end]
        },
    );
}
unsafe fn journal(g: &mut Gfx, c: &Rect, start: i32, data: &[u8]) {
    let p = pal();
    let bottom = c.bottom - 34;
    if c.top + start >= bottom {
        return;
    }
    fill(g.hdc, g.b, c.left, c.top + start, c.right, bottom, p.mica);
    let rows = ((bottom - c.top - start - 12) / 18).max(0) as usize;
    let width = (c.right - c.left - 20).max(20);
    let mut total: usize = 0;
    let detailed = PAGE == 7 && !SHOW_LOG;
    for line in data
        .split(|&v| v == b'\n')
        .filter_map(|line| journal_line(line, detailed))
    {
        let mut rest = line;
        loop {
            total += 1;
            let n = wrap_len(rest, width);
            if n == rest.len() {
                break;
            }
            rest = &rest[n..];
        }
    }
    SCROLL = SCROLL.min(total.saturating_sub(rows));
    let mut index = 0;
    for line in data
        .split(|&v| v == b'\n')
        .filter_map(|line| journal_line(line, detailed))
    {
        let mut rest = line;
        loop {
            let n = wrap_len(rest, width);
            if index >= SCROLL && index < SCROLL + rows {
                text(
                    g.hdc,
                    g.f.small,
                    p.text2,
                    c.left + 10,
                    c.top + start + 8 + (index - SCROLL) as i32 * 18,
                    &rest[..n],
                );
            }
            index += 1;
            if n == rest.len() {
                break;
            }
            rest = &rest[n..];
        }
    }
}

fn journal_line(mut line: &[u8], detailed: bool) -> Option<&[u8]> {
    if detailed {
        // Le titre/version est déjà affiché ; réserver la place aux détails
        // plutôt qu'aux lignes vides et aux marqueurs de titres Markdown.
        if line.is_empty() || line.starts_with(b"# ") {
            return None;
        }
        if line.starts_with(b"##") {
            while line.first() == Some(&b'#') {
                line = &line[1..];
            }
            if line.first() == Some(&b' ') {
                line = &line[1..];
            }
        }
    }
    Some(line)
}

// Retour à la ligne par mots, sans perdre les lignes longues ni les accents.
fn wrap_len(line: &[u8], width: i32) -> usize {
    let mut end = 0;
    while end < line.len() && text_width(&line[..end + 1], 11) <= width {
        end += 1;
    }
    if end == line.len() {
        return end;
    }
    let word = line[..end].iter().rposition(|&v| v == b' ');
    word.map(|n| n + 1).filter(|&n| n > 0).unwrap_or(end.max(1))
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    MOUSE_SPEED = match value(b"mouse_speed") {
        b"25" => 25,
        b"75" => 75,
        _ => 50,
    };
    let p = pal();
    let c = content(f);
    // Conserver le panneau Mica et les accents de la nouvelle interface,
    // sans ses bascules simulées ni ses numéros de version Windows fictifs.
    fill(
        g.hdc,
        g.b,
        f.client.left,
        f.client.top,
        f.client.right,
        f.client.bottom,
        p.mica,
    );
    fill(
        g.hdc,
        g.b,
        f.client.left,
        f.client.top,
        c.left - 20,
        f.client.bottom,
        p.surface,
    );
    fill(
        g.hdc,
        g.b,
        c.left - 20,
        f.client.top,
        c.left - 19,
        f.client.bottom,
        p.border,
    );
    text(
        g.hdc,
        g.f.large,
        p.text,
        f.client.left + 20,
        f.client.top + 20,
        tr4(&[b"Param\xE8tres", b"Settings", b"Configuraci\xF3n", b"Einstellungen"]),
    );
    let page_names = pages();
    for (pos, &i) in NAV_ORDER.iter().enumerate() {
        let name = &page_names[i];
        let r = nav_rect(f, pos);
        if i == PAGE {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.selected);
            fill(
                g.hdc,
                g.b,
                r.left,
                r.top + 8,
                r.left + 3,
                r.bottom - 8,
                p.accent,
            );
        }
        let mut end = name.len();
        while end > 0 && text_width(&name[..end], 13) > r.right - r.left - 16 {
            end -= 1;
        }
        // Centré dans la rangée, même quand les pages se resserrent.
        let ty = r.top + (r.bottom - r.top - 18) / 2;
        text(g.hdc, g.f.body, p.text, r.left + 8, ty, &name[..end]);
    }
    text(
        g.hdc,
        g.f.small,
        p.text3,
        c.left,
        c.top - 22,
        tr(S_SETTINGS_SUBTITLE),
    );
    text(g.hdc, g.f.large, p.text, c.left, c.top, page_names[PAGE]);
    fill(
        g.hdc,
        g.b,
        c.left,
        c.top + 30,
        c.left + 44,
        c.top + 33,
        p.accent,
    );
    match PAGE {
        PAGE_GRAPHICS => graphics::draw(g, f),
        8 => screens::draw(g, f),
        4 => input::draw(g, f),
        PAGE_PERSONALIZE => crate::personalize::draw_settings(g, &c),
        5 => {
            let speed_str = unsafe {
                if MOUSE_SPEED == 25 {
                    "Lente"
                } else if MOUSE_SPEED == 50 {
                    "Normale"
                } else {
                    "Rapide"
                }
            };
            row(g, &c, 44, tr4(&[b"Rapidit\xE9 de la souris", b"Mouse speed", b"Velocidad del mouse", b"Mausgeschwindigkeit"]), speed_str.as_bytes());
            button(g, &c, 0, 0, tr4(&[b"Lente", b"Slow", b"Lenta", b"Langsam"]), 0);
            button(g, &c, 0, 1, tr4(&[b"Normale", b"Normal", b"Normal", b"Normal"]), 1);
            button(g, &c, 0, 2, tr4(&[b"Rapide", b"Fast", b"R\xE1pida", b"Schnell"]), 2);
        }
        12 => crate::a11y::page_draw(g, &c),
        13 => {
            row(g, &c, 44, tr4(&[b"Effets visuels", b"Visual effects", b"Efectos visuales", b"Visuelle Effekte"]), tr4(&[b"Options d'effet visuels.", b"Visual effect options.", b"Opciones de efectos visuales.", b"Optionen f\xFCr visuelle Effekte."]));
            row(
                g,
                &c,
                84,
                tr4(&[b"Effet de flou", b"Blur effect", b"Efecto de desenfoque", b"Unsch\xE4rfeeffekt"]),
                if unsafe { ENABLE_BLUR } {
                    tr4(&[b"Activ\xE9", b"On", b"Activado", b"Ein"])
                } else {
                    tr4(&[b"D\xE9sactiv\xE9", b"Off", b"Desactivado", b"Aus"])
                },
            );
            button(g, &c, 0, 0, tr4(&[b"Activer", b"Turn on", b"Activar", b"Aktivieren"]), 0);
            button(g, &c, 0, 1, tr4(&[b"D\xE9sactiver", b"Turn off", b"Desactivar", b"Deaktivieren"]), 1);
            row(
                g,
                &c,
                124,
                tr4(&[b"Animations fluides", b"Smooth animations", b"Animaciones fluidas", b"Fl\xFCssige Animationen"]),
                if crate::gfx::animations() {
                    tr4(&[b"Activ\xE9es", b"On", b"Activadas", b"Ein"])
                } else {
                    tr4(&[b"D\xE9sactiv\xE9es", b"Off", b"Desactivadas", b"Aus"])
                },
            );
            button(g, &c, 1, 0, tr4(&[b"Activer", b"Turn on", b"Activar", b"Aktivieren"]), 3);
            button(g, &c, 1, 1, tr4(&[b"D\xE9sactiver", b"Turn off", b"Desactivar", b"Deaktivieren"]), 4);
            row(
                g,
                &c,
                164,
                tr4(&[b"Miniatures des fen\xEAtres", b"Window thumbnails", b"Miniaturas de ventanas", b"Fenstervorschauen"]),
                if unsafe { ENABLE_THUMBNAILS } {
                    tr4(&[b"Activ\xE9es", b"On", b"Activadas", b"Ein"])
                } else {
                    tr4(&[b"D\xE9sactiv\xE9es", b"Off", b"Desactivadas", b"Aus"])
                },
            );
            button(g, &c, 2, 0, tr4(&[b"Activer", b"Turn on", b"Activar", b"Aktivieren"]), 6);
            button(g, &c, 2, 1, tr4(&[b"D\xE9sactiver", b"Turn off", b"Desactivar", b"Deaktivieren"]), 7);
            row(
                g,
                &c,
                204,
                tr4(&[b"Th\xE8me automatique", b"Automatic theme", b"Tema autom\xE1tico", b"Automatisches Design"]),
                if unsafe { AUTO_THEME } {
                    tr4(&[b"Activ\xE9", b"On", b"Activado", b"Ein"])
                } else {
                    tr4(&[b"D\xE9sactiv\xE9", b"Off", b"Desactivado", b"Aus"])
                },
            );
            button(g, &c, 3, 0, tr4(&[b"Activer", b"Turn on", b"Activar", b"Aktivieren"]), 9);
            button(g, &c, 3, 1, tr4(&[b"D\xE9sactiver", b"Turn off", b"Desactivar", b"Deaktivieren"]), 10);
        }
        11 => {
            row(
                g,
                &c,
                44,
                tr4(&[b"Navigateur par d\xE9faut", b"Default browser", b"Navegador predeterminado", b"Standardbrowser"]),
                match value(b"browser_default") {
                    b"chrome" => b"Google Chrome",
                    b"firefox" => b"Mozilla Firefox",
                    _ => b"Microsoft Edge",
                },
            );
            for (i, labels) in [
                [
                    tr4(&[b"Installer Edge", b"Install Edge", b"Instalar Edge", b"Edge installieren"]),
                    tr4(&[b"Choisir Edge", b"Choose Edge", b"Elegir Edge", b"Edge w\xE4hlen"]),
                    tr4(&[b"Ouvrir Edge", b"Open Edge", b"Abrir Edge", b"Edge \xF6ffnen"]),
                ],
                [
                    tr4(&[b"Installer Chrome", b"Install Chrome", b"Instalar Chrome", b"Chrome installieren"]),
                    tr4(&[b"Choisir Chrome", b"Choose Chrome", b"Elegir Chrome", b"Chrome w\xE4hlen"]),
                    tr4(&[b"Ouvrir Chrome", b"Open Chrome", b"Abrir Chrome", b"Chrome \xF6ffnen"]),
                ],
                [
                    tr4(&[b"Installer Firefox", b"Install Firefox", b"Instalar Firefox", b"Firefox installieren"]),
                    tr4(&[b"Choisir Firefox", b"Choose Firefox", b"Elegir Firefox", b"Firefox w\xE4hlen"]),
                    tr4(&[b"Ouvrir Firefox", b"Open Firefox", b"Abrir Firefox", b"Firefox \xF6ffnen"]),
                ],
            ]
            .iter()
            .enumerate()
            {
                for (j, label) in labels.iter().enumerate() {
                    button(g, &c, i as i32, j as i32, label, (i * 3 + j) as i32);
                }
            }
            for (i, key) in [
                b"browser_edge".as_slice(),
                b"browser_chrome",
                b"browser_firefox",
            ]
            .iter()
            .enumerate()
            {
                row(
                    g,
                    &c,
                    272 + i as i32 * 65,
                    [
                        b"Microsoft Edge".as_slice(),
                        b"Google Chrome",
                        b"Mozilla Firefox",
                    ][i],
                    match value(key) {
                        b"installed" => tr4(&[b"Install\xE9", b"Installed", b"Instalado", b"Installiert"]),
                        b"available" => tr4(&[b"Paquet pr\xEAt - installation hors ligne", b"Package ready - offline installation", b"Paquete listo - instalaci\xF3n sin conexi\xF3n", b"Paket bereit - Offline-Installation"]),
                        b"update" => tr4(&[b"Mise \xE0 jour disponible", b"Update available", b"Actualizaci\xF3n disponible", b"Update verf\xFCgbar"]),
                        _ => tr4(&[b"Paquet absent de cette image", b"Package not in this image", b"Paquete ausente de esta imagen", b"Paket nicht in diesem Image"]),
                    },
                );
            }
        }
        10 => {
            if EXE_LEN == 0 {
                row(
                    g,
                    &c,
                    44,
                    tr4(&[b"Choisir un programme", b"Choose a program", b"Elegir un programa", b"Programm ausw\xE4hlen"]),
                    tr4(&[b"Clic droit sur un .exe dans Explorer, puis R\xE9glages de l'ex\xE9cutable.", b"Right-click an .exe in File Explorer, then Executable settings.", b"Haga clic derecho en un .exe en el Explorador y luego en Ajustes del ejecutable.", b"Rechtsklick auf eine .exe im Explorer, dann Einstellungen der ausf\xFChrbaren Datei."]),
                );
                return;
            }
            let mut label = [b'?'; 160];
            let n = EXE_LEN.min(label.len());
            for i in 0..n {
                label[i] = if EXE_PATH[i] <= 255 {
                    EXE_PATH[i] as u8
                } else {
                    b'?'
                };
            }
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text2,
                c.left,
                c.top + 48,
                &label[..n],
                c.right - c.left,
            );
            for (i, label) in [
                tr4(&[b"\xC9chelle auto", b"Auto scale", b"Escala autom\xE1tica", b"Automatische Skalierung"]),
                b"100 %",
                b"125 %",
                b"150 %",
                b"200 %",
                tr4(&[b"Journal diagnostic", b"Diagnostic log", b"Registro de diagn\xF3stico", b"Diagnoseprotokoll"]),
                tr4(&[b"Enregistrer", b"Save", b"Guardar", b"Speichern"]),
                tr4(&[b"Valeurs par d\xE9faut", b"Defaults", b"Valores predeterminados", b"Standardwerte"]),
                tr4(&[b"Lancer le .exe", b"Run the .exe", b"Ejecutar el .exe", b".exe starten"]),
                tr4(&[b"Processeur et priorit\xE9\x85", b"Processor and priority\x85", b"Procesador y prioridad\x85", b"Prozessor und Priorit\xE4t\x85"]),
            ]
            .iter()
            .enumerate()
            {
                if i == 9 && !crate::settings_cpu::available() {
                    continue;
                }
                button(g, &c, i as i32 / 3, i as i32 % 3, label, i as i32);
            }
            // Machines hybrides uniquement : sinon il n'y a rien à choisir.
            if graphics::exe_choices() {
                for (i, label) in [
                    tr4(&[b"GPU : automatique", b"GPU: automatic", b"GPU: autom\xE1tica", b"GPU: automatisch"]),
                    tr4(&[b"GPU \xE9conomie", b"Power-saving GPU", b"GPU de ahorro", b"Energiespar-GPU"]),
                    tr4(&[b"GPU performances", b"High-performance GPU", b"GPU de alto rendimiento", b"Hochleistungs-GPU"]),
                ]
                .iter()
                .enumerate()
                {
                    button(g, &c, 4, i as i32, label, 12 + i as i32);
                }
            }
            row(
                g,
                &c,
                350,
                tr4(&[b"Application des r\xE9glages", b"Applying settings", b"Aplicaci\xF3n de los ajustes", b"Anwendung der Einstellungen"]),
                tr4(&[b"Enregistrer puis relancer le programme. Aucun profil n'est obligatoire.", b"Save, then restart the program. No profile is required.", b"Guarde y vuelva a iniciar el programa. Ning\xFAn perfil es obligatorio.", b"Speichern und das Programm neu starten. Kein Profil ist erforderlich."]),
            );
            row(g, &c, 410, tr4(&[b"P\xE9rim\xE8tre", b"Scope", b"Alcance", b"Umfang"]), tr4(&[b"\xC9chelle du rendu Winlux et traces ; les API manquantes restent \xE0 impl\xE9menter.", b"Winlux rendering scale and traces; missing APIs still need implementing.", b"Escala del renderizado de Winlux y trazas; las API que faltan quedan por implementar.", b"Skalierung der Winlux-Darstellung und Ablaufprotokolle; fehlende APIs m\xFCssen noch implementiert werden."]));
        }
        9 => about::draw(g, &c, BUILD_INFO.get(), value(b"version")),
        0 => {
            row(g, &c, 44, tr4(&[b"Version de Winlux", b"Winlux version", b"Versi\xF3n de Winlux", b"Winlux-Version"]), value(b"version"));
            button(g, &c, 0, 0, tr4(&[b"Relancer le bureau", b"Restart the desktop", b"Reiniciar el escritorio", b"Desktop neu starten"]), 0);
            button(g, &c, 0, 1, tr4(&[b"Red\xE9marrer", b"Restart", b"Reiniciar", b"Neu starten"]), 1);
            button(g, &c, 0, 2, tr4(&[b"Arr\xEAter", b"Shut down", b"Apagar", b"Herunterfahren"]), 2);
            row(g, &c, 170, tr4(&[b"Noyau Linux", b"Linux kernel", b"Kernel de Linux", b"Linux-Kernel"]), value(b"kernel"));
            row(
                g,
                &c,
                230,
                tr4(&[b"P\xE9riph\xE9riques d'entr\xE9e d\xE9tect\xE9s", b"Input devices found", b"Dispositivos de entrada detectados", b"Erkannte Eingabeger\xE4te"]),
                value(b"input_devices"),
            );
            row(
                g,
                &c,
                290,
                tr4(&[b"Stockage du syst\xE8me", b"System storage", b"Almacenamiento del sistema", b"Systemspeicher"]),
                if value(b"persistent") == b"1" {
                    tr4(&[b"Disque persistant : r\xE9glages et mises \xE0 jour conserv\xE9s.", b"Persistent disk: settings and updates are kept.", b"Disco persistente: se conservan los ajustes y las actualizaciones.", b"Persistenter Datentr\xE4ger: Einstellungen und Updates bleiben erhalten."])
                } else {
                    tr4(&[b"Session Live en RAM : modifications perdues au red\xE9marrage.", b"Live session in RAM: changes are lost on restart.", b"Sesi\xF3n Live en RAM: los cambios se pierden al reiniciar.", b"Live-Sitzung im RAM: \xC4nderungen gehen beim Neustart verloren."])
                },
            );
            journal(g, &c, 356, LOG.get());
        }
        1 => {
            row(
                g,
                &c,
                44,
                tr4(&[b"Couche Windows / Win32", b"Windows / Win32 layer", b"Capa Windows / Win32", b"Windows-/Win32-Schicht"]),
                tr4(&[b"Runtime Winlux - .exe via Explorer", b"Winlux runtime - .exe via File Explorer", b"Runtime de Winlux - .exe mediante el Explorador", b"Winlux-Runtime - .exe \xFCber den Explorer"]),
            );
            button(g, &c, 0, 0, tr4(&[b"Programmes .exe", b".exe programs", b"Programas .exe", b".exe-Programme"]), 0);
            button(g, &c, 0, 1, tr4(&[b"Dossier personnel", b"Home folder", b"Carpeta personal", b"Pers\xF6nlicher Ordner"]), 1);
            button(g, &c, 0, 2, tr4(&[b"Mises \xE0 jour", b"Updates", b"Actualizaciones", b"Updates"]), 2);
            row(
                g,
                &c,
                170,
                tr4(&[b"Installer une application Windows", b"Install a Windows app", b"Instalar una aplicaci\xF3n de Windows", b"Windows-App installieren"]),
                tr4(&[b"Ouvrir son installateur .exe dans Explorer.", b"Open its .exe installer in File Explorer.", b"Abra su instalador .exe en el Explorador.", b"\xD6ffnen Sie das .exe-Installationsprogramm im Explorer."]),
            );
            row(
                g,
                &c,
                228,
                tr4(&[b"Linux et Windows", b"Linux and Windows", b"Linux y Windows", b"Linux und Windows"]),
                tr4(&[b"Les .exe partagent la session Linux et les fichiers.", b".exe programs share the Linux session and files.", b"Los .exe comparten la sesi\xF3n de Linux y los archivos.", b".exe-Programme teilen sich die Linux-Sitzung und die Dateien."]),
            );
            row(
                g,
                &c,
                286,
                tr4(&[b"Compatibilit\xE9", b"Compatibility", b"Compatibilidad", b"Kompatibilit\xE4t"]),
                tr4(&[b"Le support d\xE9pend des API et composants requis par le logiciel.", b"Support depends on the APIs and components the software needs.", b"La compatibilidad depende de las API y componentes que requiere el software.", b"Die Unterst\xFCtzung h\xE4ngt von den APIs und Komponenten ab, die die Software ben\xF6tigt."]),
            );
        }
        2 => {
            row(g, &c, 44, tr4(&[b"Connexion / adresse IPv4", b"Connection / IPv4 address", b"Conexi\xF3n / direcci\xF3n IPv4", b"Verbindung / IPv4-Adresse"]), value(b"network"));
            button(g, &c, 0, 0, tr4(&[b"Renouveler DHCP", b"Renew DHCP", b"Renovar DHCP", b"DHCP erneuern"]), 0);
            button(g, &c, 0, 1, tr4(&[b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"]), 1);
            row(
                g,
                &c,
                170,
                tr4(&[b"Machine virtuelle", b"Virtual machine", b"M\xE1quina virtual", b"Virtuelle Maschine"]),
                tr4(&[b"Ethernet virtuel ; le Wi-Fi reste g\xE9r\xE9 par l'h\xF4te.", b"Virtual Ethernet; Wi-Fi is managed by the host.", b"Ethernet virtual; el Wi-Fi lo gestiona el anfitri\xF3n.", b"Virtuelles Ethernet; WLAN wird vom Host verwaltet."]),
            );
            journal(g, &c, 236, LOG.get());
        }
        PAGE_POWER => draw_power(g, &c),
        3 => {
            row(g, &c, 44, tr4(&[b"Sortie audio / volume", b"Audio output / volume", b"Salida de audio / volumen", b"Audioausgabe / Lautst\xE4rke"]), value(b"audio"));
            button(g, &c, 0, 0, tr4(&[b"Volume -", b"Volume -", b"Volumen -", b"Lautst\xE4rke -"]), 0);
            button(g, &c, 0, 1, tr4(&[b"Muet / r\xE9tablir", b"Mute / unmute", b"Silenciar / restablecer", b"Stumm / Ton an"]), 1);
            button(g, &c, 0, 2, tr4(&[b"Volume +", b"Volume +", b"Volumen +", b"Lautst\xE4rke +"]), 2);
            row(
                g,
                &c,
                170,
                tr4(&[b"Batterie et alimentation", b"Battery and power", b"Bater\xEDa y energ\xEDa", b"Akku und Energie"]),
                tr4(&[b"Modes d'alimentation, veille et luminosit\xE9 : page Alimentation.", b"Power modes, sleep and brightness: Power page.", b"Modos de energ\xEDa, suspensi\xF3n y brillo: p\xE1gina Energ\xEDa.", b"Energiemodi, Energiesparen und Helligkeit: Seite Energie."]),
            );
            button(g, &c, 3, 0, tr4(&[b"Options d'alimentation", b"Power options", b"Opciones de energ\xEDa", b"Energieoptionen"]), 9);
            journal(g, &c, 280, LOG.get());
        }
        6 => {
            row(
                g,
                &c,
                44,
                tr4(&[b"Barre des t\xE2ches", b"Taskbar", b"Barra de tareas", b"Taskleiste"]),
                tr4(&[b"Pr\xE9f\xE9rences enregistr\xE9es pour la prochaine session.", b"Preferences saved for the next session.", b"Preferencias guardadas para la pr\xF3xima sesi\xF3n.", b"Einstellungen f\xFCr die n\xE4chste Sitzung gespeichert."]),
            );
            button(g, &c, 0, 0, tr4(&[b"Aligner \xE0 gauche", b"Align left", b"Alinear a la izquierda", b"Links ausrichten"]), 0);
            button(g, &c, 0, 1, tr4(&[b"Centrer", b"Center", b"Centrar", b"Zentrieren"]), 1);
            button(g, &c, 1, 0, tr4(&[b"Th\xE8me sombre", b"Dark theme", b"Tema oscuro", b"Dunkles Design"]), 3);
            button(g, &c, 1, 1, tr4(&[b"Th\xE8me clair", b"Light theme", b"Tema claro", b"Helles Design"]), 4);
            row(
                g,
                &c,
                164,
                tr4(&[b"Th\xE8me automatique", b"Automatic theme", b"Tema autom\xE1tico", b"Automatisches Design"]),
                if unsafe { AUTO_THEME } {
                    tr4(&[b"Jour clair / nuit sombre (8h\x9620h)", b"Light by day / dark by night (8am\x968pm)", b"Claro de d\xEDa / oscuro de noche (8\x9620 h)", b"Tags\xFCber hell / nachts dunkel (8\x9620 Uhr)"])
                } else {
                    tr4(&[b"D\xE9sactiv\xE9", b"Off", b"Desactivado", b"Aus"])
                },
            );
            button(g, &c, 2, 0, tr4(&[b"Activer", b"Turn on", b"Activar", b"Aktivieren"]), 5);
            button(g, &c, 2, 1, tr4(&[b"D\xE9sactiver", b"Turn off", b"Desactivar", b"Deaktivieren"]), 6);
            row(
                g,
                &c,
                254,
                tr4(&[b"Stockage", b"Storage", b"Almacenamiento", b"Speicher"]),
                if value(b"persistent") == b"1" {
                    tr4(&[b"Les pr\xE9f\xE9rences sont conserv\xE9es sur le disque.", b"Preferences are kept on disk.", b"Las preferencias se conservan en el disco.", b"Die Einstellungen werden auf dem Datentr\xE4ger gespeichert."])
                } else {
                    tr4(&[b"Session Live : les r\xE9glages durent jusqu'au red\xE9marrage.", b"Live session: settings last until restart.", b"Sesi\xF3n Live: los ajustes duran hasta el reinicio.", b"Live-Sitzung: Einstellungen gelten bis zum Neustart."])
                },
            );
            row(
                g,
                &c,
                300,
                tr4(&[b"Fond d'\xE9cran, couleurs, barre des t\xE2ches, ic\xF4nes et sons", b"Wallpaper, colors, taskbar, icons and sounds", b"Fondo de pantalla, colores, barra de tareas, iconos y sonidos", b"Hintergrund, Farben, Taskleiste, Symbole und Sounds"]),
                tr4(&[b"Galerie et aper\xE7u dans la page Personnalisation.", b"Gallery and preview on the Personalization page.", b"Galer\xEDa y vista previa en la p\xE1gina Personalizaci\xF3n.", b"Galerie und Vorschau auf der Seite Personalisierung."]),
            );
            button(g, &c, 5, 0, tr4(&[b"Personnaliser", b"Personalize", b"Personalizar", b"Anpassen"]), 12);
            button(g, &c, 5, 1, tr4(&[b"Assistant de bienvenue", b"Welcome assistant", b"Asistente de bienvenida", b"Willkommensassistent"]), 13);
            button(g, &c, 7, 0, tr4(&[b"R\xE9glages de l'\xE9cran", b"Display settings", b"Configuraci\xF3n de pantalla", b"Bildschirmeinstellungen"]), 18);
        }
        7 => {
            if updating() {
                draw_installation(g, &c);
                return;
            }
            let restart = value(b"update_action");
            if restart_required() {
                row(
                    g,
                    &c,
                    44,
                    tr4(&[b"Installation termin\xE9e", b"Installation complete", b"Instalaci\xF3n terminada", b"Installation abgeschlossen"]),
                    tr4(&[b"Un red\xE9marrage est n\xE9cessaire pour terminer la mise \xE0 jour.", b"A restart is needed to finish the update.", b"Es necesario reiniciar para terminar la actualizaci\xF3n.", b"Zum Abschlie\xDFen des Updates ist ein Neustart erforderlich."]),
                );
                let r = restart_rect(&c);
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    5,
                    if HOVER == 0 { p.hover } else { p.accent },
                );
                text_centered(
                    g.hdc,
                    g.f.body,
                    13,
                    rgb(18, 24, 34),
                    (r.left + r.right) / 2,
                    r.top + 15,
                    tr4(&[b"Red\xE9marrer pour terminer la mise \xE0 jour", b"Restart to finish the update", b"Reiniciar para terminar la actualizaci\xF3n", b"Neu starten, um das Update abzuschlie\xDFen"]),
                );
                journal(g, &c, 198, LOG.get());
                return;
            }
            if restart == b"reboot" {
                row(
                    g,
                    &c,
                    44,
                    tr4(&[b"Installation termin\xE9e - red\xE9marrage requis", b"Installation complete - restart required", b"Instalaci\xF3n terminada - es necesario reiniciar", b"Installation abgeschlossen - Neustart erforderlich"]),
                    tr4(&[b"Red\xE9marrez maintenant, ou \xE9teignez puis rallumez la machine.", b"Restart now, or turn the machine off and on again.", b"Reinicie ahora, o apague y vuelva a encender el equipo.", b"Starten Sie jetzt neu oder schalten Sie den Computer aus und wieder ein."]),
                );
            } else if field(PROGRESS.get(), b"phase") == b"error" {
                row(
                    g,
                    &c,
                    44,
                    tr4(&[b"L'installation n'a pas abouti", b"The installation did not complete", b"La instalaci\xF3n no se complet\xF3", b"Die Installation wurde nicht abgeschlossen"]),
                    tr4(&[b"Consultez le journal avant de r\xE9essayer.", b"Check the log before trying again.", b"Consulte el registro antes de volver a intentarlo.", b"Pr\xFCfen Sie das Protokoll, bevor Sie es erneut versuchen."]),
                );
            } else {
                row(
                    g,
                    &c,
                    44,
                    tr4(&[b"Mises \xE0 jour Winlux", b"Winlux updates", b"Actualizaciones de Winlux", b"Winlux-Updates"]),
                    if value(b"update_auto") == b"enabled" {
                        tr4(&[b"Recherche automatique activ\xE9e.", b"Automatic check is on.", b"B\xFAsqueda autom\xE1tica activada.", b"Automatische Suche aktiviert."])
                    } else {
                        tr4(&[b"Installation sur demande.", b"Installation on request.", b"Instalaci\xF3n a petici\xF3n.", b"Installation auf Anfrage."])
                    },
                );
            }
            button(g, &c, 0, 0, tr4(&[b"Rechercher", b"Check", b"Buscar", b"Suchen"]), 0);
            if !already_up_to_date() {
                button(g, &c, 0, 1, tr4(&[b"Tout installer", b"Install all", b"Instalar todo", b"Alle installieren"]), 1);
            }
            button(g, &c, 0, 2, tr4(&[b"Restaurer", b"Restore", b"Restaurar", b"Wiederherstellen"]), 2);
            button(
                g,
                &c,
                1,
                0,
                match value(b"sysupdate_mode") {
                    b"auto" => tr4(&[b"Mode : automatique", b"Mode: automatic", b"Modo: autom\xE1tico", b"Modus: automatisch"]),
                    b"scheduled" => tr4(&[b"Mode : planifi\xE9", b"Mode: scheduled", b"Modo: programado", b"Modus: geplant"]),
                    _ => tr4(&[b"Mode : manuel", b"Mode: manual", b"Modo: manual", b"Modus: manuell"]),
                },
                3,
            );
            button(
                g,
                &c,
                1,
                1,
                if SHOW_LOG { tr4(&[b"Masquer", b"Hide", b"Ocultar", b"Ausblenden"]) } else { tr4(&[b"Journal", b"Log", b"Registro", b"Protokoll"]) },
                4,
            );
            if restart == b"reboot" {
                button(g, &c, 1, 2, tr4(&[b"Red\xE9marrer", b"Restart", b"Reiniciar", b"Neu starten"]), 5);
            }
            text(g.hdc, g.f.small, p.text3, c.left, c.top + 208, tr4(&[b"Les mises \xE0 jour sont t\xE9l\xE9charg\xE9es depuis les releases publiques GitHub.", b"Updates are downloaded from the public GitHub releases.", b"Las actualizaciones se descargan de las releases p\xFAblicas de GitHub.", b"Updates werden aus den \xF6ffentlichen GitHub-Releases heruntergeladen."]));
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                c.left,
                c.top + 244,
                if SHOW_LOG {
                    tr4(&[b"Journal d'installation", b"Installation log", b"Registro de instalaci\xF3n", b"Installationsprotokoll"])
                } else {
                    tr4(&[b"Modifications apport\xE9es par la mise \xE0 jour", b"Changes in this update", b"Cambios incluidos en la actualizaci\xF3n", b"\xC4nderungen durch das Update"])
                },
            );
            journal(
                g,
                &c,
                270,
                if SHOW_LOG {
                    LOG.get()
                } else if UPDATES.len == 0 {
                    tr4(&[b"Cliquez Rechercher pour afficher les modifications.\x0AMolette ou touches Haut/Bas pour lire la suite.", b"Click Check to show the changes.\x0AUse the wheel or Up/Down keys to read on.", b"Haga clic en Buscar para mostrar los cambios.\x0ARueda o teclas Arriba/Abajo para seguir leyendo.", b"Klicken Sie auf Suchen, um die \xC4nderungen anzuzeigen.\x0AMausrad oder Pfeiltasten zum Weiterlesen."])
                } else {
                    UPDATES.get()
                },
            );
            text(g.hdc, g.f.small, p.text3, c.left, c.bottom - 30, tr4(&[b"Molette / Haut-Bas : d\xE9filer ; Page suivante/pr\xE9c\xE9dente : lecture rapide.", b"Wheel / Up-Down: scroll; Page Down/Up: fast reading.", b"Rueda / Arriba-Abajo: desplazar; Av P\xE1g/Re P\xE1g: lectura r\xE1pida.", b"Mausrad / Pfeiltasten: Bl\xE4ttern; Bild auf/ab: schnelles Lesen."]));
        }
        14 => {
            row(g, &c, 44, tr(S_LANGUAGE), tr(S_LANGUAGE_DESC));
            let cur_lang = unsafe { CURRENT_LANG } as usize;
            for i in 0..LANG_COUNT {
                button(g, &c, i as i32 / 3, i as i32 % 3, LANG_NAMES[i], i as i32);
            }
            row(
                g,
                &c,
                164,
                tr(S_KEYBOARD_LAYOUT),
                tr(S_KEYBOARD_LAYOUT_DESC),
            );
            let kb_start = unsafe { LOCALE_KB_SCROLL };
            let kb_vis = 6usize.min(KEYBOARD_COUNT);
            for i in 0..kb_vis {
                let idx = kb_start + i;
                if idx < KEYBOARD_COUNT {
                    button(
                        g,
                        &c,
                        2 + i as i32 / 3,
                        i as i32 % 3,
                        KEYBOARD_NAMES[idx],
                        (10 + idx) as i32,
                    );
                }
            }
            row(g, &c, 344, tr(S_TIMEZONE), tr(S_TIMEZONE_DESC));
            row(g, &c, 404, tr(S_CURRENT_TIMEZONE), value(b"timezone"));
            let tz_start = unsafe { LOCALE_TZ_SCROLL };
            let tz_vis = 6usize.min(TZ_COUNT);
            for i in 0..tz_vis {
                let idx = tz_start + i;
                if idx < TZ_COUNT {
                    button(
                        g,
                        &c,
                        7 + i as i32 / 3,
                        i as i32 % 3,
                        tr(TZ_STRINGS[idx]),
                        (30 + idx) as i32,
                    );
                }
            }
        }
        15 => {
            row(
                g,
                &c,
                44,
                tr4(&[b"Gestionnaire de fichiers", b"File manager", b"Administrador de archivos", b"Dateimanager"]),
                tr4(&[b"Pr\xE9f\xE9rences de l'explorateur de fichiers Winlux.", b"Winlux File Explorer preferences.", b"Preferencias del Explorador de archivos de Winlux.", b"Einstellungen des Winlux-Datei-Explorers."]),
            );
            row(
                g,
                &c,
                104,
                tr4(&[b"Affichage par d\xE9faut", b"Default view", b"Vista predeterminada", b"Standardansicht"]),
                match unsafe { crate::explorer::current_view() } {
                    0 => tr4(&[b"Tr\xE8s grandes ic\xF4nes", b"Extra large icons", b"Iconos muy grandes", b"Extra gro\xDFe Symbole"]),
                    1 => tr4(&[b"Grandes ic\xF4nes", b"Large icons", b"Iconos grandes", b"Gro\xDFe Symbole"]),
                    2 => tr4(&[b"Ic\xF4nes moyennes", b"Medium icons", b"Iconos medianos", b"Mittelgro\xDFe Symbole"]),
                    3 => tr4(&[b"Petites ic\xF4nes", b"Small icons", b"Iconos peque\xF1os", b"Kleine Symbole"]),
                    4 => tr4(&[b"Liste", b"List", b"Lista", b"Liste"]),
                    5 => tr4(&[b"D\xE9tails", b"Details", b"Detalles", b"Details"]),
                    _ => tr4(&[b"Mosa\xEFques", b"Tiles", b"Mosaicos", b"Kacheln"]),
                },
            );
            button(g, &c, 0, 0, tr4(&[b"Grandes ic\xF4nes", b"Large icons", b"Iconos grandes", b"Gro\xDFe Symbole"]), 0);
            button(g, &c, 0, 1, tr4(&[b"D\xE9tails", b"Details", b"Detalles", b"Details"]), 1);
            button(g, &c, 0, 2, tr4(&[b"Mosa\xEFques", b"Tiles", b"Mosaicos", b"Kacheln"]), 2);
            row(
                g,
                &c,
                230,
                tr4(&[b"\xC9l\xE9ments masqu\xE9s", b"Hidden items", b"Elementos ocultos", b"Ausgeblendete Elemente"]),
                if unsafe { crate::explorer::show_hidden() } {
                    tr4(&[b"Visibles", b"Shown", b"Visibles", b"Sichtbar"])
                } else {
                    tr4(&[b"Masqu\xE9s", b"Hidden", b"Ocultos", b"Ausgeblendet"])
                },
            );
            button(g, &c, 2, 0, tr4(&[b"Afficher", b"Show", b"Mostrar", b"Anzeigen"]), 3);
            button(g, &c, 2, 1, tr4(&[b"Masquer", b"Hide", b"Ocultar", b"Ausblenden"]), 4);
            row(
                g,
                &c,
                356,
                tr4(&[b"Extensions de noms de fichiers", b"File name extensions", b"Extensiones de nombre de archivo", b"Dateinamenerweiterungen"]),
                if unsafe { crate::explorer::show_ext() } {
                    tr4(&[b"Affich\xE9es", b"Shown", b"Visibles", b"Angezeigt"])
                } else {
                    tr4(&[b"Masqu\xE9es", b"Hidden", b"Ocultas", b"Ausgeblendet"])
                },
            );
            button(g, &c, 4, 0, tr4(&[b"Afficher", b"Show", b"Mostrar", b"Anzeigen"]), 6);
            button(g, &c, 4, 1, tr4(&[b"Masquer", b"Hide", b"Ocultar", b"Ausblenden"]), 7);
        }
        16 => {
            row(
                g,
                &c,
                44,
                tr4(&[b"P\xE9riph\xE9rique de sortie", b"Output device", b"Dispositivo de salida", b"Ausgabeger\xE4t"]),
                value(b"audio_output"),
            );
            button(g, &c, 0, 0, tr4(&[b"Haut-parleurs", b"Speakers", b"Altavoces", b"Lautsprecher"]), 0);
            button(g, &c, 0, 1, tr4(&[b"Casque / HDMI", b"Headphones / HDMI", b"Auriculares / HDMI", b"Kopfh\xF6rer / HDMI"]), 1);
            button(g, &c, 0, 2, tr4(&[b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"]), 2);
            row(
                g,
                &c,
                170,
                tr4(&[b"P\xE9riph\xE9rique d'entr\xE9e", b"Input device", b"Dispositivo de entrada", b"Eingabeger\xE4t"]),
                value(b"audio_input"),
            );
            button(g, &c, 1, 0, tr4(&[b"Micro int\xE9gr\xE9", b"Built-in microphone", b"Micr\xF3fono integrado", b"Integriertes Mikrofon"]), 3);
            button(g, &c, 1, 1, tr4(&[b"Micro externe", b"External microphone", b"Micr\xF3fono externo", b"Externes Mikrofon"]), 4);
            row(g, &c, 280, tr4(&[b"Volume principal", b"Main volume", b"Volumen principal", b"Hauptlautst\xE4rke"]), value(b"audio"));
            button(g, &c, 3, 0, tr4(&[b"Volume -", b"Volume -", b"Volumen -", b"Lautst\xE4rke -"]), 6);
            button(g, &c, 3, 1, tr4(&[b"Muet / r\xE9tablir", b"Mute / unmute", b"Silenciar / restablecer", b"Stumm / Ton an"]), 7);
            button(g, &c, 3, 2, tr4(&[b"Volume +", b"Volume +", b"Volumen +", b"Lautst\xE4rke +"]), 8);
            row(
                g,
                &c,
                370,
                tr4(&[b"Volume par application", b"Volume per app", b"Volumen por aplicaci\xF3n", b"Lautst\xE4rke pro App"]),
                tr4(&[b"Chaque fen\xEAtre ouverte a son propre niveau de volume.", b"Each open window has its own volume level.", b"Cada ventana abierta tiene su propio nivel de volumen.", b"Jedes ge\xF6ffnete Fenster hat eine eigene Lautst\xE4rke."]),
            );
            let mut ay = 430i32;
            for app in 0..crate::wm::APP_COUNT {
                if !crate::wm::is_open(app) || ay + 50 > c.bottom - c.top {
                    continue;
                }
                row(g, &c, ay, crate::wm::title_of(app), b"100 %");
                ay += 60;
            }
        }
        PAGE_REMOTE => draw_remote(g, &c),
        PAGE_SYSUPDATE => draw_sysupdate(g, &c),
        PAGE_PRIVACY => draw_privacy(g, &c),
        PAGE_CPU => crate::settings_cpu::draw(g, &c),
        PAGE_NETDRIVES => crate::netdrives::draw(g, &c),
        BLUETOOTH_PAGE => draw_bluetooth(g, &c),
        PAGE_REPORT => draw_report(g, &c),
        PAGE_PRINTERS => printers::draw(g, &c, SCROLL),
        PAGE_SHARING => crate::sharing::draw(g, &c),
        _ => {}
    }
    if PAGE == PAGE_NETDRIVES || PAGE == PAGE_SHARING {
        return;
    }
    let notice = if PAGE == 10 {
        tr4(EXE_NOTICE)
    } else if PAGE == PAGE_PRINTERS {
        printers::notice()
    } else if PAGE == PAGE_GRAPHICS {
        graphics::runtime_notice()
    } else if !available() {
        tr4(&[b"Service Winlux indisponible : les actions n\xE9cessitent Winlux.", b"Winlux service unavailable: these actions need Winlux.", b"Servicio de Winlux no disponible: las acciones requieren Winlux.", b"Winlux-Dienst nicht verf\xFCgbar: Die Aktionen erfordern Winlux."])
    } else {
        ACTION.get().split(|&v| v == b'\n').next().unwrap_or(b"")
    };
    let mut end = notice.len();
    while end > 0 && text_width(&notice[..end], 11) > c.right - c.left {
        end -= 1;
    }
    text(
        g.hdc,
        g.f.small,
        p.text2,
        c.left,
        c.bottom - 16,
        &notice[..end],
    );
}

unsafe fn remote(key: &[u8]) -> &'static [u8] {
    field(REMOTE.get(), key)
}
/// i-ème compte Winlux qui a un mot de passe : (nom, autorisé à distance).
unsafe fn remote_account(i: usize) -> Option<(&'static [u8], bool)> {
    let line = REMOTE
        .get()
        .split(|&v| v == b'\n')
        .filter_map(|l| l.strip_prefix(b"account="))
        .filter(|l| l.ends_with(b"|1"))
        .nth(i)?;
    let name = line.split(|&v| v == b'|').next()?;
    let allowed = remote(b"allowed").split(|&v| v == b',').any(|a| a == name);
    Some((name, allowed))
}
fn join(parts: &[&[u8]], out: &mut [u8]) -> usize {
    let mut n = 0;
    for part in parts {
        let k = part.len().min(out.len() - n);
        out[n..n + k].copy_from_slice(&part[..k]);
        n += k;
    }
    n
}
unsafe fn draw_remote(g: &mut Gfx, c: &Rect) {
    let address = match remote(b"address") {
        b"" => tr(S_REMOTE_THIS_COMPUTER),
        a => a,
    };
    let mut state = [0u8; 200];
    let clients = remote(b"clients");
    let n = if remote(b"enabled") != b"1" {
        join(&[tr(S_REMOTE_OFF)], &mut state)
    } else if remote(b"running") != b"1" {
        join(&[tr(S_REMOTE_NOT_RUNNING)], &mut state)
    } else if clients.is_empty() || clients == b"0" {
        join(&[tr(S_REMOTE_IDLE)], &mut state)
    } else {
        let who = REMOTE
            .get()
            .split(|&v| v == b'\n')
            .filter_map(|l| l.strip_prefix(b"client="))
            .next()
            .unwrap_or(b"");
        let mut parts = who.split(|&v| v == b'|');
        let (user, ip) = (parts.next().unwrap_or(b""), parts.next().unwrap_or(b""));
        join(
            &[tr(S_REMOTE_CONNECTED), clients, b" (", user, tr(S_REMOTE_FROM), ip, b")"],
            &mut state,
        )
    };
    row(g, c, 44, tr(S_REMOTE_DESKTOP), &state[..n]);
    button(g, c, 0, 0, tr(S_REMOTE_ENABLE), 0);
    button(g, c, 0, 1, tr(S_REMOTE_DISABLE), 1);
    button(g, c, 0, 2, tr(S_REMOTE_REFRESH), 2);
    button(g, c, 1, 0, tr(S_REMOTE_FULL_CONTROL), 3);
    button(g, c, 1, 1, tr(S_REMOTE_VIEW_ONLY), 4);
    button(g, c, 1, 2, tr(S_REMOTE_LEGACY), 5);
    let mut addr = [0u8; 200];
    let n = if remote(b"legacy") == b"1" {
        join(
            &[
                address,
                b":",
                remote(b"port"),
                tr(S_REMOTE_LEGACY_PASSWORD),
                remote(b"legacy_password"),
            ],
            &mut addr,
        )
    } else if remote(b"fingerprint").is_empty() {
        join(&[address, b":", remote(b"port")], &mut addr)
    } else {
        join(
            &[address, b":", remote(b"port"), tr(S_REMOTE_FINGERPRINT), remote(b"fingerprint")],
            &mut addr,
        )
    };
    row(g, c, 214, tr(S_REMOTE_ADDRESS), &addr[..n]);
    button(g, c, 4, 0, b"Port -", 12);
    button(g, c, 4, 1, b"Port 5900", 13);
    button(g, c, 4, 2, b"Port +", 14);
    row(
        g,
        c,
        346,
        tr(S_REMOTE_ACCOUNTS),
        tr(S_REMOTE_ACCOUNTS_HINT),
    );
    for i in 0..6 {
        if let Some((name, _)) = remote_account(i) {
            button(g, c, 7 + i as i32 / 3, i as i32 % 3, name, 21 + i as i32);
        }
    }
}
/// Petit tampon de texte Latin-1 pour composer les libellés dynamiques.
struct Line {
    bytes: [u8; 96],
    len: usize,
}
impl Line {
    fn new() -> Self {
        Self {
            bytes: [0; 96],
            len: 0,
        }
    }
    fn push(&mut self, s: &[u8]) -> &mut Self {
        for &c in s {
            if self.len < self.bytes.len() {
                self.bytes[self.len] = c;
                self.len += 1;
            }
        }
        self
    }
    fn num(&mut self, v: u32) -> &mut Self {
        let mut digits = [0u8; 10];
        let mut i = digits.len();
        let mut v = v;
        loop {
            i -= 1;
            digits[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.push(&digits[i..])
    }
    fn duration(&mut self, secs: u32) -> &mut Self {
        let m = secs / 60 % 60;
        if secs >= 3600 {
            self.num(secs / 3600).push(b" h ");
            if m < 10 {
                self.push(b"0");
            }
        }
        self.num(m).push(b" min")
    }
    /// Délai de la configuration : « 10 min », « 2 h » ou « Jamais ».
    fn delay(&mut self, raw: &[u8]) -> &mut Self {
        if raw.is_empty() {
            return self.push(b"?");
        }
        match number(raw) {
            0 => self.push(tr4(&[b"Jamais", b"Never", b"Nunca", b"Nie"])),
            s if s >= 3600 => self.num(s / 3600).push(b" h"),
            s => self.num(s / 60).push(b" min"),
        }
    }
    fn get(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

fn action_label(key: &[u8]) -> &'static [u8] {
    match key {
        b"nothing" => tr4(&[b"Ne rien faire", b"Do nothing", b"No hacer nada", b"Nichts unternehmen"]),
        b"sleep" => tr4(&[b"Veille", b"Sleep", b"Suspender", b"Energie sparen"]),
        b"hibernate" => tr4(&[b"Veille prolong\xE9e", b"Hibernate", b"Hibernar", b"Ruhezustand"]),
        b"shutdown" => tr4(&[b"Arr\xEAter", b"Shut down", b"Apagar", b"Herunterfahren"]),
        b"display-off" => tr4(&[b"\xC9teindre l'\xE9cran", b"Turn off the screen", b"Apagar la pantalla", b"Bildschirm ausschalten"]),
        _ => b"?",
    }
}

/// Page « Alimentation et batterie » : état, modes, délais, capot/bouton,
/// luminosité. Les valeurs viennent de `/run/winlux/power.txt`.
unsafe fn draw_power(g: &mut Gfx, c: &Rect) {
    let mut battery = Line::new();
    if pvalue(b"power_battery").is_empty() {
        battery.push(tr4(&[b"Service d'alimentation Winlux non d\xE9marr\xE9", b"Winlux power service not started", b"Servicio de energ\xEDa de Winlux no iniciado", b"Winlux-Energiedienst nicht gestartet"]));
    } else if pvalue(b"power_battery") == b"0" {
        battery.push(tr4(&[b"Alimentation secteur - aucune batterie d\xE9tect\xE9e", b"Plugged in - no battery found", b"Conectado a la corriente - no se detect\xF3 bater\xEDa", b"Netzbetrieb - kein Akku erkannt"]));
    } else {
        battery.num(number(pvalue(b"power_percent"))).push(b" % - ");
        let left = pvalue(b"power_seconds_left");
        let full = pvalue(b"power_seconds_to_full");
        match pvalue(b"power_state") {
            b"charging" => {
                battery.push(tr4(&[b"En charge", b"Charging", b"Cargando", b"Wird geladen"]));
                if !full.is_empty() {
                    battery.push(tr4(&[b", pleine dans ", b", full in ", b", completa en ", b", voll in "])).duration(number(full));
                }
            }
            b"discharging" => {
                battery.push(tr4(&[b"Sur batterie", b"On battery", b"Con bater\xEDa", b"Akkubetrieb"]));
                if !left.is_empty() {
                    battery.push(b", ").duration(number(left)).push(tr4(&[b" restantes", b" left", b" restantes", b" verbleibend"]));
                }
            }
            b"full" => {
                battery.push(tr4(&[b"Charg\xE9e", b"Charged", b"Cargada", b"Geladen"]));
            }
            b"not-charging" => {
                battery.push(tr4(&[b"Branch\xE9e, pas en charge", b"Plugged in, not charging", b"Conectada, sin cargar", b"Angeschlossen, wird nicht geladen"]));
            }
            _ => {
                battery.push(tr4(&[b"\xC9tat inconnu", b"Unknown state", b"Estado desconocido", b"Unbekannter Zustand"]));
            }
        }
        if !pvalue(b"power_health").is_empty() {
            battery
                .push(tr4(&[b" - sant\xE9 ", b" - health ", b" - salud ", b" - Zustand "]))
                .num(number(pvalue(b"power_health")))
                .push(b" %");
        }
    }
    row(g, c, 44, tr4(&[b"Batterie et mode d'alimentation", b"Battery and power mode", b"Bater\xEDa y modo de energ\xEDa", b"Akku und Energiemodus"]), battery.get());
    button(g, c, 0, 0, tr4(&[b"Performances maximales", b"Best performance", b"M\xE1ximo rendimiento", b"H\xF6chstleistung"]), 0);
    button(g, c, 0, 1, tr4(&[b"\xC9quilibr\xE9", b"Balanced", b"Equilibrado", b"Ausbalanciert"]), 1);
    button(g, c, 0, 2, tr4(&[b"\xC9conomie d'\xE9nergie", b"Power saver", b"Ahorro de energ\xEDa", b"Energiesparmodus"]), 2);

    let mut now = Line::new();
    now.push(tr4(&[b"Appliqu\xE9 maintenant : \xE9cran ", b"Applied now: screen ", b"Aplicado ahora: pantalla ", b"Jetzt angewendet: Bildschirm "]))
        .push(pvalue(b"power_screen_now"))
        .push(tr4(&[b", veille ", b", sleep ", b", suspensi\xF3n ", b", Energiesparen "]))
        .push(pvalue(b"power_sleep_now"));
    row(g, c, 176, tr4(&[b"\xC9cran et mise en veille (cliquer pour changer)", b"Screen and sleep (click to change)", b"Pantalla y suspensi\xF3n (haga clic para cambiar)", b"Bildschirm und Energiesparen (zum \xC4ndern klicken)"]), now.get());
    let mut l = Line::new();
    l.push(tr4(&[b"\xC9cran secteur : ", b"Screen plugged in: ", b"Pantalla con corriente: ", b"Bildschirm Netzbetrieb: "])).delay(pvalue(b"power_screen_ac"));
    button(g, c, 3, 0, l.get(), 9);
    let mut l = Line::new();
    l.push(tr4(&[b"\xC9cran batterie : ", b"Screen on battery: ", b"Pantalla con bater\xEDa: ", b"Bildschirm Akku: "])).delay(pvalue(b"power_screen_dc"));
    button(g, c, 3, 1, l.get(), 10);
    if pvalue(b"power_can_sleep") == b"1" {
        button(g, c, 3, 2, tr4(&[b"Mettre en veille", b"Sleep", b"Suspender", b"Energie sparen"]), 11);
    }
    let mut l = Line::new();
    l.push(tr4(&[b"Veille secteur : ", b"Sleep plugged in: ", b"Suspensi\xF3n con corriente: ", b"Energiesparen Netzbetrieb: "])).delay(pvalue(b"power_sleep_ac"));
    button(g, c, 4, 0, l.get(), 12);
    let mut l = Line::new();
    l.push(tr4(&[b"Veille batterie : ", b"Sleep on battery: ", b"Suspensi\xF3n con bater\xEDa: ", b"Energiesparen Akku: "])).delay(pvalue(b"power_sleep_dc"));
    button(g, c, 4, 1, l.get(), 13);

    let mut l = Line::new();
    l.push(tr4(&[b"Luminosit\xE9 ", b"Brightness ", b"Brillo ", b"Helligkeit "]));
    if pvalue(b"power_brightness").is_empty() {
        l.push(tr4(&[b"non r\xE9glable ici (VM ou \xE9cran externe)", b"not adjustable here (VM or external screen)", b"no ajustable aqu\xED (VM o pantalla externa)", b"hier nicht einstellbar (VM oder externer Bildschirm)"]));
    } else {
        l.num(number(pvalue(b"power_brightness"))).push(b" %");
        if pvalue(b"power_profile") == b"saver" {
            l.push(tr4(&[b" (mode \xE9conomie)", b" (power saver)", b" (modo ahorro)", b" (Energiesparmodus)"]));
        }
    }
    l.push(if pvalue(b"power_can_hibernate") == b"1" {
        tr4(&[b" - veille prolong\xE9e pr\xEAte", b" - hibernate ready", b" - hibernaci\xF3n lista", b" - Ruhezustand bereit"]) as &[u8]
    } else {
        tr4(&[b" - veille prolong\xE9e \xE0 pr\xE9parer", b" - hibernate needs setup", b" - hibernaci\xF3n por preparar", b" - Ruhezustand muss eingerichtet werden"])
    });
    row(g, c, 352, tr4(&[b"Capot, bouton d'alimentation et luminosit\xE9", b"Lid, power button and brightness", b"Tapa, bot\xF3n de encendido y brillo", b"Deckel, Netzschalter und Helligkeit"]), l.get());
    let mut l = Line::new();
    l.push(tr4(&[b"Capot : ", b"Lid: ", b"Tapa: ", b"Deckel: "])).push(action_label(pvalue(b"power_lid")));
    button(g, c, 7, 0, l.get(), 21);
    let mut l = Line::new();
    l.push(tr4(&[b"Bouton : ", b"Button: ", b"Bot\xF3n: ", b"Taste: "])).push(action_label(pvalue(b"power_button")));
    button(g, c, 7, 1, l.get(), 22);
    if pvalue(b"power_can_hibernate") == b"1" {
        button(g, c, 7, 2, tr4(&[b"Veille prolong\xE9e", b"Hibernate", b"Hibernar", b"Ruhezustand"]), 23);
    }
    button(g, c, 8, 0, tr4(&[b"Luminosit\xE9 -", b"Brightness -", b"Brillo -", b"Helligkeit -"]), 24);
    button(g, c, 8, 1, tr4(&[b"Luminosit\xE9 +", b"Brightness +", b"Brillo +", b"Helligkeit +"]), 25);
}

fn hit(f: &Frame, x: i32, y: i32) -> i32 {
    let c = content(f);
    if unsafe { PAGE == 7 && restart_required() } {
        return if inside(&restart_rect(&c), x, y) {
            0
        } else {
            -1
        };
    }
    // Douze rangées de trois boutons (Signaler un problème va jusqu'à 9, Bureau à distance jusqu'à 11).
    for i in 0..36 {
        let r = button_rect(&c, i / 3, i % 3);
        if r.bottom <= c.bottom - 24 && inside(&r, x, y) {
            return i;
        }
    }
    -1
}
pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if PAGE == 12 && !matches!(ev, Mouse::Down | Mouse::DoubleClick) {
        return crate::a11y::page_mouse(ev, x, y, &content(f));
    }
    if PAGE == PAGE_NETDRIVES && !matches!(ev, Mouse::Down | Mouse::DoubleClick) {
        return crate::netdrives::mouse(ev, x, y, &content(f));
    }
    if PAGE == PAGE_SHARING && !matches!(ev, Mouse::Down | Mouse::DoubleClick) {
        return crate::sharing::mouse(ev, x, y, &content(f));
    }
    if let Mouse::Wheel(delta) = ev {
        if PAGE == PAGE_CPU {
            return crate::settings_cpu::scroll(if delta < 0 { 3 } else { -3 });
        }
        if delta < 0 {
            SCROLL = SCROLL.saturating_add(3);
        } else {
            SCROLL = SCROLL.saturating_sub(3);
        }
        return true;
    }
    if ev == Mouse::Move && PAGE == PAGE_PRINTERS {
        return printers::mouse(ev, x, y);
    }
    if ev == Mouse::Move && PAGE == 8 {
        return screens::hover(f, x, y);
    }
    if ev == Mouse::Move && PAGE == 4 {
        return input::hover(f, x, y);
    }
    if ev == Mouse::Move && PAGE == PAGE_GRAPHICS {
        let h = graphics::hit(f, x, y);
        let changed = h != graphics::HOVER;
        graphics::HOVER = h;
        return changed;
    }
    if ev == Mouse::Move && PAGE == PAGE_CPU {
        return crate::settings_cpu::hover(&content(f), x, y);
    }
    if PAGE == PAGE_PERSONALIZE && matches!(ev, Mouse::Move | Mouse::Down | Mouse::DoubleClick) {
        let c = content(f);
        if ev == Mouse::Move {
            return crate::personalize::settings_move(&c, x, y);
        }
        if inside(&c, x, y) {
            return crate::personalize::settings_click(&c, x, y);
        }
    }
    if ev == Mouse::Move {
        let h = hit(f, x, y);
        let changed = h != HOVER;
        HOVER = h;
        return changed;
    }
    if ev != Mouse::Down && ev != Mouse::DoubleClick {
        return false;
    }
    for (pos, &i) in NAV_ORDER.iter().enumerate() {
        if inside(&nav_rect(f, pos), x, y) {
            if i == PAGE_GRAPHICS {
                graphics::RUNTIME_LOADED = false;
            }
            PAGE = i;
            SCROLL = 0;
            HOVER = -1;
            LAST_POLL = 0;
            if i == PAGE_REMOTE {
                control(c"remote-status");
            }
            system_tick();
            return true;
        }
    }
    if PAGE == PAGE_PRINTERS {
        return printers::mouse(ev, x, y);
    }
    if PAGE == PAGE_GRAPHICS {
        return graphics::click(f, x, y);
    }
    if PAGE == 8 {
        return screens::click(f, x, y);
    }
    if PAGE == 4 {
        return input::click(f, x, y);
    }
    if PAGE == 12 {
        return crate::a11y::page_mouse(ev, x, y, &content(f));
    }
    if PAGE == PAGE_CPU {
        return crate::settings_cpu::click(&content(f), x, y);
    }
    if PAGE == PAGE_NETDRIVES {
        return crate::netdrives::mouse(ev, x, y, &content(f));
    }
    if PAGE == PAGE_SHARING {
        return crate::sharing::mouse(ev, x, y, &content(f));
    }
    if PAGE == 7 {
        let c = content(f);
        if updating() {
            return true;
        }
        if restart_required() {
            if inside(&restart_rect(&c), x, y) {
                control(c"reboot");
            }
            return true;
        }
    }
    let action = match (PAGE, hit(f, x, y)) {
        (5, 0) => {
            unsafe { MOUSE_SPEED = 25 };
            Some(c"mouse-slow")
        }
        (5, 1) => {
            unsafe { MOUSE_SPEED = 50 };
            Some(c"mouse-normal")
        }
        (5, 2) => {
            unsafe { MOUSE_SPEED = 75 };
            Some(c"mouse-fast")
        }
        (6, 12) => {
            crate::personalize::open_settings(0);
            return true;
        }
        (6, 13) => {
            crate::personalize::start_oobe();
            return true;
        }
        (11, id) if (0..=8).contains(&id) => Some(
            [
                c"browser-install-edge",
                c"browser-default-edge",
                c"browser-launch-edge",
                c"browser-install-chrome",
                c"browser-default-chrome",
                c"browser-launch-chrome",
                c"browser-install-firefox",
                c"browser-default-firefox",
                c"browser-launch-firefox",
            ][id as usize],
        ),
        (10, id @ 12..=14) if EXE_LEN > 0 && graphics::exe_choices() => {
            EXE_GPU = (id - 12) as usize;
            EXE_NOTICE = &[b"Cliquez sur Enregistrer puis relancez le programme.", b"Click Save, then restart the program.", b"Haga clic en Guardar y vuelva a iniciar el programa.", b"Klicken Sie auf Speichern und starten Sie das Programm neu."];
            return true;
        }
        (10, 9) if EXE_LEN > 0 && crate::settings_cpu::available() => {
            open_page(PAGE_CPU);
            crate::settings_cpu::focus_program();
            return true;
        }
        (10, id) if EXE_LEN > 0 && (0..=8).contains(&id) => {
            match id {
                0..=4 => EXE_SCALE = id as usize,
                5 => EXE_TRACE = !EXE_TRACE,
                6 => save_exe_settings(false),
                7 => save_exe_settings(true),
                8 => launch_exe(),
                _ => {}
            }
            return true;
        }
        (0, 0) => Some(c"restart-session"),
        (0, 1) => Some(c"reboot"),
        (0, 2) => Some(c"poweroff"),
        (1, 0) => {
            crate::explorer::open_pc();
            return true;
        }
        (1, 1) => {
            crate::explorer::open_home();
            return true;
        }
        (1, 2) => {
            open_page(7);
            return true;
        }
        (2, 0) => Some(c"network-renew"),
        (2, 1) => Some(c"status"),
        (3, 0) => Some(c"volume-down"),
        (3, 1) => Some(c"volume-mute"),
        (3, 2) => Some(c"volume-up"),
        (3, 9) => {
            open_page(PAGE_POWER);
            return true;
        }
        (PAGE_POWER, 0) => Some(c"power-profile-performance"),
        (PAGE_POWER, 1) => Some(c"power-profile-balanced"),
        (PAGE_POWER, 2) => Some(c"power-profile-saver"),
        (PAGE_POWER, 9) => Some(c"power-screen-ac"),
        (PAGE_POWER, 10) => Some(c"power-screen-dc"),
        (PAGE_POWER, 11) if pvalue(b"power_can_sleep") == b"1" => Some(c"power-suspend"),
        (PAGE_POWER, 12) => Some(c"power-sleep-ac"),
        (PAGE_POWER, 13) => Some(c"power-sleep-dc"),
        (PAGE_POWER, 21) => Some(c"power-lid"),
        (PAGE_POWER, 22) => Some(c"power-button"),
        (PAGE_POWER, 23) if pvalue(b"power_can_hibernate") == b"1" => Some(c"power-hibernate"),
        (PAGE_POWER, 24) => Some(c"power-brightness-down"),
        (PAGE_POWER, 25) => Some(c"power-brightness-up"),
        (6, 0) => Some(c"align-left"),
        (6, 1) => Some(c"align-center"),
        (6, 3) => {
            unsafe { crate::gfx::set_theme(true) };
            Some(c"theme-dark")
        }
        (6, 4) => {
            unsafe { crate::gfx::set_theme(false) };
            Some(c"theme-light")
        }
        (6, 5) => {
            unsafe { AUTO_THEME = true };
            unsafe { crate::gfx::check_auto_theme() };
            Some(c"auto-theme-on")
        }
        (6, 6) => {
            unsafe { AUTO_THEME = false };
            Some(c"auto-theme-off")
        }
        (6, 18) => {
            open_page(8);
            return true;
        }
        (13, 0) => {
            unsafe { ENABLE_BLUR = true };
            Some(c"blur-on")
        }
        (13, 1) => {
            unsafe { ENABLE_BLUR = false };
            Some(c"blur-off")
        }
        (13, id @ (3 | 4)) => {
            // Même réglage que Accessibilité > Effets d'animation.
            crate::gfx::A11Y.animations = id == 3;
            ENABLE_SMOOTH_ANIM = id == 3;
            crate::gfx::a11y_save();
            return true;
        }
        (13, 6) => {
            unsafe { ENABLE_THUMBNAILS = true };
            Some(c"thumb-on")
        }
        (13, 7) => {
            unsafe { ENABLE_THUMBNAILS = false };
            Some(c"thumb-off")
        }
        (13, 9) => {
            unsafe { AUTO_THEME = true };
            Some(c"auto-theme-on")
        }
        (13, 10) => {
            unsafe { AUTO_THEME = false };
            Some(c"auto-theme-off")
        }
        (7, 0) => {
            SHOW_LOG = false;
            SCROLL = 0;
            Some(c"update-check")
        }
        (7, 1) => {
            if already_up_to_date() {
                return true;
            }
            SHOW_LOG = true;
            SCROLL = 0;
            Some(if value(b"update_action") == b"reboot" {
                c"reboot"
            } else {
                c"update-apply"
            })
        }
        (7, 2) => {
            SHOW_LOG = true;
            SCROLL = 0;
            Some(c"update-rollback")
        }
        (7, 3) => {
            open_page(PAGE_SYSUPDATE);
            return true;
        }
        (7, 4) => {
            SHOW_LOG = !SHOW_LOG;
            SCROLL = 0;
            return true;
        }
        (7, 5) => Some(if value(b"update_action") == b"reboot" {
            c"reboot"
        } else {
            c"restart-session"
        }),
        // Son avancé (page 16)
        (16, 0) => Some(c"audio-output-speakers"),
        (16, 1) => Some(c"audio-output-headphones"),
        (16, 2) => Some(c"status"),
        (16, 3) => Some(c"audio-input-builtin"),
        (16, 4) => Some(c"audio-input-external"),
        (16, 6) => Some(c"volume-down"),
        (16, 7) => Some(c"volume-mute"),
        (16, 8) => Some(c"volume-up"),
        (PAGE_REMOTE, 0) => Some(c"remote-enable"),
        (PAGE_REMOTE, 1) => Some(c"remote-disable"),
        (PAGE_REMOTE, 2) => Some(c"remote-status"),
        (PAGE_REMOTE, 3) => Some(c"remote-full-control"),
        (PAGE_REMOTE, 4) => Some(c"remote-view-only"),
        (PAGE_REMOTE, 5) => Some(if remote(b"legacy") == b"1" {
            c"remote-legacy-off"
        } else {
            c"remote-legacy-on"
        }),
        (PAGE_REMOTE, 12) => Some(c"remote-port-down"),
        (PAGE_REMOTE, 13) => Some(c"remote-port-default"),
        (PAGE_REMOTE, 14) => Some(c"remote-port-up"),
        (PAGE_REMOTE, id @ 21..=26) => {
            if let Some((name, allowed)) = remote_account(id as usize - 21) {
                let mut cmd = [0u8; 64];
                let verb: &[u8] = if allowed { b"remote-deny " } else { b"remote-allow " };
                let n = name.len().min(40);
                cmd[..verb.len()].copy_from_slice(verb);
                cmd[verb.len()..verb.len() + n].copy_from_slice(&name[..n]);
                if let Ok(c) = core::ffi::CStr::from_bytes_until_nul(&cmd) {
                    control(c);
                }
            }
            return true;
        }
        (14, 0..=3) => {
            unsafe {
                CURRENT_LANG = match hit(f, x, y) {
                    0 => Lang::Fr,
                    1 => Lang::En,
                    2 => Lang::Es,
                    3 => Lang::De,
                    _ => CURRENT_LANG,
                }
            };
            set_ui_lang(unsafe { CURRENT_LANG } as usize);
            let code = LANG_CODES[unsafe { CURRENT_LANG } as usize];
            let mut cmd = [0u8; 16];
            cmd[..5].copy_from_slice(b"lang-");
            cmd[5..5 + code.len()].copy_from_slice(code);
            if let Ok(cstr) = core::ffi::CStr::from_bytes_until_nul(&cmd) {
                control(cstr);
            }
            return true;
        }
        (14, id @ 10..=17) => {
            let idx = (id as usize).wrapping_sub(10);
            if idx < KEYBOARD_COUNT {
                let code = KEYBOARD_CODES[idx];
                let mut cmd = [0u8; 16];
                cmd[..9].copy_from_slice(b"keyboard-");
                cmd[9..9 + code.len()].copy_from_slice(code);
                if let Ok(cstr) = core::ffi::CStr::from_bytes_until_nul(&cmd) {
                    control(cstr);
                }
            }
            return true;
        }
        (14, id @ 30..=41) => {
            let idx = (id as usize).wrapping_sub(30);
            if idx < TZ_COUNT {
                let tz = TZ_IDS[idx];
                let mut cmd = [0u8; 48];
                cmd[..3].copy_from_slice(b"tz-");
                let n = tz.len().min(cmd.len() - 4);
                cmd[3..3 + n].copy_from_slice(&tz[..n]);
                if let Ok(cstr) = core::ffi::CStr::from_bytes_until_nul(&cmd) {
                    control(cstr);
                }
            }
            return true;
        }
        (15, 0) => {
            crate::explorer::set_view(1);
            return true;
        }
        (15, 1) => {
            crate::explorer::set_view(5);
            return true;
        }
        (15, 2) => {
            crate::explorer::set_view(6);
            return true;
        }
        (15, 3) => {
            crate::explorer::set_show_hidden(true);
            return true;
        }
        (15, 4) => {
            crate::explorer::set_show_hidden(false);
            return true;
        }
        (15, 6) => {
            crate::explorer::set_show_ext(true);
            return true;
        }
        (15, 7) => {
            crate::explorer::set_show_ext(false);
            return true;
        }
        (PAGE_SYSUPDATE, 0) => Some(c"sysupdate-mode-auto"),
        (PAGE_SYSUPDATE, 1) => Some(c"sysupdate-mode-manual"),
        (PAGE_SYSUPDATE, 2) => Some(c"sysupdate-mode-scheduled"),
        (PAGE_SYSUPDATE, 9) => Some(c"sysupdate-hour-down"),
        (PAGE_SYSUPDATE, 10) => Some(c"sysupdate-hour-up"),
        (PAGE_SYSUPDATE, 11) => Some(c"sysupdate-day-next"),
        (PAGE_SYSUPDATE, 18) => {
            SHOW_LOG = false;
            SCROLL = 0;
            Some(c"sysupdate-check")
        }
        (PAGE_SYSUPDATE, 19) => {
            if sysupdate_busy() || number(value(b"sysupdate_count")) == 0 {
                return true;
            }
            SHOW_LOG = true;
            SCROLL = 0;
            Some(c"sysupdate-apply")
        }
        (PAGE_SYSUPDATE, 20) => {
            SHOW_LOG = !SHOW_LOG;
            SCROLL = 0;
            return true;
        }
        (PAGE_PRIVACY, 0) => Some(c"privacy-telemetry-off"),
        (PAGE_PRIVACY, 1) => Some(c"privacy-telemetry-on"),
        (PAGE_PRIVACY, 9) => Some(if value(b"privacy_microphone") == b"deny" {
            c"privacy-microphone-allow"
        } else {
            c"privacy-microphone-deny"
        }),
        (PAGE_PRIVACY, 10) => Some(if value(b"privacy_camera") == b"deny" {
            c"privacy-camera-allow"
        } else {
            c"privacy-camera-deny"
        }),
        (PAGE_PRIVACY, 11) => Some(if value(b"privacy_location") == b"allow" {
            c"privacy-location-deny"
        } else {
            c"privacy-location-allow"
        }),
        (PAGE_REPORT, id @ 0..=2) => {
            report(match id {
                0 => c"",
                1 => c"--crash --force",
                _ => c"--open-saved",
            });
            return true;
        }
        (PAGE_REPORT, 9) => Some(c"report-offer-on"),
        (PAGE_REPORT, 10) => Some(c"report-offer-off"),
        (PAGE_REPORT, 18) => Some(c"report-redact-on"),
        (PAGE_REPORT, 19) => Some(c"report-redact-off"),
        (PAGE_REPORT, 27) => Some(c"report-mode-auto"),
        (PAGE_REPORT, 28) => Some(c"report-mode-browser"),
        (PAGE_REPORT, 29) => Some(c"report-mode-file"),
        (PAGE_PRIVACY, 18) => Some(c"privacy-history-off"),
        (PAGE_PRIVACY, 19) => Some(c"privacy-history-on"),
        (BLUETOOTH_PAGE, 0) => Some(c"bluetooth-on"),
        (BLUETOOTH_PAGE, 1) => Some(c"bluetooth-off"),
        (BLUETOOTH_PAGE, 2) => Some(c"bluetooth-scan"),
        (BLUETOOTH_PAGE, id) if id >= 3 => {
            let k = (id - 3) as usize / 3;
            let Some(dev) = bt_device(BLUETOOTH.get(), k) else {
                return true;
            };
            let mac = bt_field(dev, 0);
            match (id - 3) % 3 {
                1 => bt_command(bt_primary(dev).0, mac),
                2 if bt_field(dev, 3) == b"1" => bt_command(b"bluetooth-remove", mac),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(a) = action {
        control(a);
        return true;
    }
    false
}
pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if PAGE == PAGE_PRINTERS && printers::key(vk, ch) {
        return true;
    }
    if PAGE == 12 && ch == 0 && crate::a11y::page_key(vk) {
        return true;
    }
    if PAGE == PAGE_NETDRIVES && crate::netdrives::key(vk, ch) {
        return true;
    }
    if PAGE == 7 && restart_required() {
        if !updating() && ch == 13 {
            control(c"reboot");
        }
        return true;
    }
    if PAGE == PAGE_CPU {
        match vk {
            0x26 => return crate::settings_cpu::scroll(-1),
            0x28 => return crate::settings_cpu::scroll(1),
            0x21 => return crate::settings_cpu::scroll(-10),
            0x22 => return crate::settings_cpu::scroll(10),
            _ => {}
        }
    }
    match vk {
        0x26 => {
            SCROLL = SCROLL.saturating_sub(1);
            true
        }
        0x28 => {
            SCROLL = SCROLL.saturating_add(1);
            true
        }
        0x21 => {
            SCROLL = SCROLL.saturating_sub(10);
            true
        }
        0x22 => {
            SCROLL = SCROLL.saturating_add(10);
            true
        }
        0x24 => {
            SCROLL = 0;
            true
        }
        0x23 => {
            SCROLL = usize::MAX;
            true
        }
        0x74 => {
            control(c"status");
            true
        }
        _ => false,
    }
}
unsafe fn draw_sysupdate(g: &mut Gfx, c: &Rect) {
    let p = pal();
    let mode = value(b"sysupdate_mode");
    row(
        g,
        c,
        44,
        tr4(&[b"Mode de mise \xE0 jour", b"Update mode", b"Modo de actualizaci\xF3n", b"Updatemodus"]),
        match mode {
            b"auto" => tr4(&[b"Automatique : recherche et installation chaque jour, sans intervention.", b"Automatic: check and install every day, hands-free.", b"Autom\xE1tico: b\xFAsqueda e instalaci\xF3n cada d\xEDa, sin intervenci\xF3n.", b"Automatisch: t\xE4gliche Suche und Installation ohne Eingriff."]),
            b"scheduled" => tr4(&[b"Planifi\xE9 : installation au cr\xE9neau choisi ci-dessous.", b"Scheduled: install at the time slot chosen below.", b"Programado: instalaci\xF3n en la franja elegida abajo.", b"Geplant: Installation im unten gew\xE4hlten Zeitfenster."]),
            _ => tr4(&[b"Manuel : recherche chaque jour et notification ; vous installez quand vous voulez.", b"Manual: daily check and notification; you install when you want.", b"Manual: b\xFAsqueda diaria y notificaci\xF3n; usted instala cuando quiera.", b"Manuell: t\xE4gliche Suche und Benachrichtigung; Sie installieren, wann Sie m\xF6chten."]),
        },
    );
    button(g, c, 0, 0, tr4(&[b"Automatique", b"Automatic", b"Autom\xE1tica", b"Automatisch"]), 0);
    button(g, c, 0, 1, tr4(&[b"Manuel", b"Manual", b"Manual", b"Manuell"]), 1);
    button(g, c, 0, 2, tr4(&[b"Planifi\xE9", b"Scheduled", b"Programado", b"Geplant"]), 2);
    let mut slot = [0u8; 64];
    let mut n = push(
        &mut slot,
        0,
        match value(b"sysupdate_day") {
            b"mon" => tr4(&[b"Chaque lundi", b"Every Monday", b"Cada lunes", b"Jeden Montag"]),
            b"tue" => tr4(&[b"Chaque mardi", b"Every Tuesday", b"Cada martes", b"Jeden Dienstag"]),
            b"wed" => tr4(&[b"Chaque mercredi", b"Every Wednesday", b"Cada mi\xE9rcoles", b"Jeden Mittwoch"]),
            b"thu" => tr4(&[b"Chaque jeudi", b"Every Thursday", b"Cada jueves", b"Jeden Donnerstag"]),
            b"fri" => tr4(&[b"Chaque vendredi", b"Every Friday", b"Cada viernes", b"Jeden Freitag"]),
            b"sat" => tr4(&[b"Chaque samedi", b"Every Saturday", b"Cada s\xE1bado", b"Jeden Samstag"]),
            b"sun" => tr4(&[b"Chaque dimanche", b"Every Sunday", b"Cada domingo", b"Jeden Sonntag"]),
            _ => tr4(&[b"Tous les jours", b"Every day", b"Todos los d\xEDas", b"T\xE4glich"]),
        },
    );
    n = push(&mut slot, n, tr4(&[b" \xE0 ", b" at ", b" a las ", b" um "]));
    let hour = number(value(b"sysupdate_hour")).min(23);
    if hour < 10 {
        n = push(&mut slot, n, b"0");
    }
    n = push_number(&mut slot, n, hour);
    n = push(&mut slot, n, b":00");
    if mode != b"scheduled" {
        n = push(&mut slot, n, tr4(&[b" (utilis\xE9 en mode Planifi\xE9)", b" (used in Scheduled mode)", b" (se usa en modo Programado)", b" (im Modus Geplant verwendet)"]));
    }
    row(g, c, 166, tr4(&[b"Cr\xE9neau planifi\xE9", b"Scheduled slot", b"Franja programada", b"Geplantes Zeitfenster"]), &slot[..n]);
    button(g, c, 3, 0, tr4(&[b"Heure -", b"Hour -", b"Hora -", b"Stunde -"]), 9);
    button(g, c, 3, 1, tr4(&[b"Heure +", b"Hour +", b"Hora +", b"Stunde +"]), 10);
    button(g, c, 3, 2, tr4(&[b"Changer le jour", b"Change the day", b"Cambiar el d\xEDa", b"Tag \xE4ndern"]), 11);
    let count = number(value(b"sysupdate_count"));
    let mut summary = [0u8; 128];
    let mut n = 0;
    if value(b"sysupdate_apt") != b"1" {
        n = push(&mut summary, n, tr4(&[b"APT absent de cette session Live : seul Winlux est mis \xE0 jour (page Mises \xE0 jour).", b"APT is not in this Live session: only Winlux is updated (Updates page).", b"APT no est\xE1 en esta sesi\xF3n Live: solo se actualiza Winlux (p\xE1gina Actualizaciones).", b"APT fehlt in dieser Live-Sitzung: Nur Winlux wird aktualisiert (Seite Updates)."]));
    } else if sysupdate_busy() {
        n = push(
            &mut summary,
            n,
            tr4(&[b"Installation des paquets en cours : n'\xE9teignez pas la machine.", b"Installing packages: don't turn off the machine.", b"Instalando paquetes: no apague el equipo.", b"Pakete werden installiert: Schalten Sie den Computer nicht aus."]),
        );
    } else {
        if count == 0 {
            n = push(&mut summary, n, tr4(&[b"Syst\xE8me \xE0 jour", b"System up to date", b"Sistema actualizado", b"System auf dem neuesten Stand"]));
        } else {
            n = push_number(&mut summary, n, count);
            n = push(
                &mut summary,
                n,
                if count == 1 {
                    tr4(&[b" paquet \xE0 mettre \xE0 jour", b" package to update", b" paquete por actualizar", b" Paket zu aktualisieren"])
                } else {
                    tr4(&[b" paquets \xE0 mettre \xE0 jour", b" packages to update", b" paquetes por actualizar", b" Pakete zu aktualisieren"])
                },
            );
        }
        n = push(&mut summary, n, tr4(&[b" - derni\xE8re recherche : ", b" - last check: ", b" - \xFAltima b\xFAsqueda: ", b" - letzte Suche: "]));
        n = push(&mut summary, n, value(b"sysupdate_last"));
    }
    row(
        g,
        c,
        300,
        tr4(&[b"Paquets du syst\xE8me Linux (APT)", b"Linux system packages (APT)", b"Paquetes del sistema Linux (APT)", b"Pakete des Linux-Systems (APT)"]),
        &summary[..n],
    );
    button(g, c, 6, 0, tr4(&[b"Rechercher", b"Check", b"Buscar", b"Suchen"]), 18);
    button(g, c, 6, 1, tr4(&[b"Tout installer", b"Install all", b"Instalar todo", b"Alle installieren"]), 19);
    button(
        g,
        c,
        6,
        2,
        if SHOW_LOG {
            tr4(&[b"Liste des paquets", b"Package list", b"Lista de paquetes", b"Paketliste"])
        } else {
            tr4(&[b"Journal", b"Log", b"Registro", b"Protokoll"])
        },
        20,
    );
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        c.left,
        c.top + 420,
        if SHOW_LOG {
            tr4(&[b"Journal APT", b"APT log", b"Registro de APT", b"APT-Protokoll"])
        } else {
            tr4(&[b"Paquets concern\xE9s", b"Affected packages", b"Paquetes afectados", b"Betroffene Pakete"])
        },
    );
    journal(
        g,
        c,
        444,
        if SHOW_LOG {
            LOG.get()
        } else if SYSUPDATES.len == 0 {
            tr4(&[b"Aucun paquet en attente. Cliquez Rechercher pour interroger les d\xE9p\xF4ts APT.", b"No pending packages. Click Check to query the APT repositories.", b"Ning\xFAn paquete pendiente. Haga clic en Buscar para consultar los repositorios APT.", b"Keine ausstehenden Pakete. Klicken Sie auf Suchen, um die APT-Quellen abzufragen."])
        } else {
            SYSUPDATES.get()
        },
    );
}
unsafe fn draw_privacy(g: &mut Gfx, c: &Rect) {
    row(
        g,
        c,
        44,
        tr4(&[b"T\xE9l\xE9m\xE9trie et donn\xE9es de diagnostic", b"Telemetry and diagnostic data", b"Telemetr\xEDa y datos de diagn\xF3stico", b"Telemetrie und Diagnosedaten"]),
        if value(b"privacy_telemetry") == b"on" {
            tr4(&[b"Autoris\xE9e : aucun composant actuel de Winlux n'envoie pourtant de donn\xE9es.", b"Allowed: no current Winlux component actually sends data.", b"Permitida: aun as\xED, ning\xFAn componente actual de Winlux env\xEDa datos.", b"Erlaubt: Derzeit sendet trotzdem keine Winlux-Komponente Daten."])
        } else {
            tr4(&[b"D\xE9sactiv\xE9e : Winlux n'envoie aucune donn\xE9e d'utilisation.", b"Off: Winlux sends no usage data.", b"Desactivada: Winlux no env\xEDa ning\xFAn dato de uso.", b"Aus: Winlux sendet keine Nutzungsdaten."])
        },
    );
    button(g, c, 0, 0, tr4(&[b"D\xE9sactiv\xE9e", b"Off", b"Desactivada", b"Aus"]), 0);
    button(g, c, 0, 1, tr4(&[b"Autoris\xE9e", b"Allowed", b"Permitida", b"Erlaubt"]), 1);
    row(
        g,
        c,
        166,
        tr4(&[b"Autorisations des applications Windows (.exe)", b"Windows app (.exe) permissions", b"Permisos de las aplicaciones de Windows (.exe)", b"Berechtigungen der Windows-Apps (.exe)"]),
        tr4(&[b"Bouton en surbrillance : acc\xE8s autoris\xE9. Cliquez pour bloquer ou autoriser.", b"Highlighted button: access allowed. Click to block or allow.", b"Bot\xF3n resaltado: acceso permitido. Haga clic para bloquear o permitir.", b"Hervorgehobene Schaltfl\xE4che: Zugriff erlaubt. Klicken zum Sperren oder Erlauben."]),
    );
    let state = |key: &[u8], allowed_default: bool| {
        let v = value(key);
        if allowed_default {
            v != b"deny"
        } else {
            v == b"allow"
        }
    };
    button(
        g,
        c,
        3,
        0,
        if state(b"privacy_microphone", true) {
            tr4(&[b"Microphone : autoris\xE9", b"Microphone: allowed", b"Micr\xF3fono: permitido", b"Mikrofon: erlaubt"])
        } else {
            tr4(&[b"Microphone : bloqu\xE9", b"Microphone: blocked", b"Micr\xF3fono: bloqueado", b"Mikrofon: gesperrt"])
        },
        9,
    );
    button(
        g,
        c,
        3,
        1,
        if state(b"privacy_camera", true) {
            tr4(&[b"Cam\xE9ra : autoris\xE9e", b"Camera: allowed", b"C\xE1mara: permitida", b"Kamera: erlaubt"])
        } else {
            tr4(&[b"Cam\xE9ra : bloqu\xE9e", b"Camera: blocked", b"C\xE1mara: bloqueada", b"Kamera: gesperrt"])
        },
        10,
    );
    button(
        g,
        c,
        3,
        2,
        if state(b"privacy_location", false) {
            tr4(&[b"Localisation : autoris\xE9e", b"Location: allowed", b"Ubicaci\xF3n: permitida", b"Standort: erlaubt"])
        } else {
            tr4(&[b"Localisation : bloqu\xE9e", b"Location: blocked", b"Ubicaci\xF3n: bloqueada", b"Standort: gesperrt"])
        },
        11,
    );
    row(
        g,
        c,
        300,
        tr4(&[b"Historique de localisation", b"Location history", b"Historial de ubicaciones", b"Standortverlauf"]),
        if value(b"privacy_location_history") == b"on" {
            tr4(&[b"Activ\xE9 : conserv\xE9 uniquement sur cet ordinateur.", b"On: kept only on this computer.", b"Activado: se conserva solo en este equipo.", b"Ein: nur auf diesem Computer gespeichert."])
        } else {
            tr4(&[b"D\xE9sactiv\xE9 : aucune position n'est enregistr\xE9e.", b"Off: no location is recorded.", b"Desactivado: no se registra ninguna ubicaci\xF3n.", b"Aus: Es wird kein Standort gespeichert."])
        },
    );
    button(g, c, 6, 0, tr4(&[b"D\xE9sactiv\xE9", b"Off", b"Desactivado", b"Aus"]), 18);
    button(g, c, 6, 1, tr4(&[b"Activ\xE9", b"On", b"Activado", b"Ein"]), 19);
    row(
        g,
        c,
        430,
        tr4(&[b"Vos donn\xE9es restent chez vous", b"Your data stays with you", b"Sus datos se quedan con usted", b"Ihre Daten bleiben bei Ihnen"]),
        tr4(&[b"Un micro refus\xE9 est bloqu\xE9 par le runtime ; cam\xE9ra et localisation ne sont pas encore expos\xE9es aux .exe.", b"A denied microphone is blocked by the runtime; camera and location are not exposed to .exe programs yet.", b"Un micr\xF3fono denegado lo bloquea el runtime; la c\xE1mara y la ubicaci\xF3n a\xFAn no se exponen a los .exe.", b"Ein verweigertes Mikrofon wird von der Runtime gesperrt; Kamera und Standort sind f\xFCr .exe-Programme noch nicht verf\xFCgbar."]),
    );
}
/// Lance « Signaler un problème » (winlux-report) : le rapport s'affiche en
/// entier, modifiable, avant tout envoi.
pub unsafe fn report(args: &core::ffi::CStr) {
    if ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-report".as_ptr(),
        args.as_ptr(),
        core::ptr::null(),
        1,
    ) as isize
        <= 32
    {
        crate::desktop::toast(tr(S_INFO_UNAVAILABLE));
    }
}
unsafe fn draw_report(g: &mut Gfx, c: &Rect) {
    row(g, c, 44, tr(S_REPORT_PAGE), tr(S_REPORT_DESC));
    button(g, c, 0, 0, tr(S_REPORT_NEW), 0);
    button(g, c, 0, 1, tr(S_REPORT_LAST), 1);
    button(g, c, 0, 2, tr(S_REPORT_SAVED), 2);
    let offer = value(b"report_offer") != b"off";
    let offer_desc = if offer { S_REPORT_OFFER_ON_DESC } else { S_REPORT_OFFER_OFF_DESC };
    row(g, c, 166, tr(S_REPORT_OFFER), tr(offer_desc));
    button(g, c, 3, 0, tr(S_REPORT_PROPOSE), 9);
    button(g, c, 3, 1, tr(S_REPORT_SILENT), 10);
    let mask = value(b"report_redact") != b"off";
    let mask_desc = if mask { S_REPORT_MASK_DESC } else { S_REPORT_NOMASK_DESC };
    row(g, c, 300, tr(S_REPORT_PRIVATE), tr(mask_desc));
    button(g, c, 6, 0, tr(S_REPORT_MASK), 18);
    button(g, c, 6, 1, tr(S_REPORT_NOMASK), 19);
    let mode = match (value(b"report_mode"), value(b"report_transport")) {
        (b"browser", _) => S_REPORT_MODE_BROWSER,
        (b"file", _) => S_REPORT_MODE_FILE,
        (_, b"browser") => S_REPORT_MODE_AUTO_BROWSER,
        _ => S_REPORT_MODE_RELAY,
    };
    row(g, c, 430, tr(S_REPORT_MODE), tr(mode));
    button(g, c, 9, 0, tr(S_REPORT_AUTO), 27);
    button(g, c, 9, 1, tr(S_REPORT_BROWSER), 28);
    button(g, c, 9, 2, tr(S_REPORT_FILE), 29);
}
unsafe fn draw_installation(g: &mut Gfx, c: &Rect) {
    let p = pal();
    row(
        g,
        c,
        54,
        tr4(&[b"Installation en cours", b"Installing", b"Instalaci\xF3n en curso", b"Installation l\xE4uft"]),
        tr4(&[b"N'\xE9teignez pas la machine. L'arr\xEAt est temporairement verrouill\xE9.", b"Don't turn off the machine. Shutdown is temporarily locked.", b"No apague el equipo. El apagado est\xE1 bloqueado temporalmente.", b"Schalten Sie den Computer nicht aus. Das Herunterfahren ist vor\xFCbergehend gesperrt."]),
    );
    let phase = field(PROGRESS.get(), b"phase");
    let title: &[u8] = match phase {
        b"downloading" => tr4(&[b"T\xE9l\xE9chargement du paquet", b"Downloading the package", b"Descargando el paquete", b"Paket wird heruntergeladen"]),
        b"verifying" => tr4(&[b"V\xE9rification de l'int\xE9grit\xE9", b"Checking integrity", b"Comprobando la integridad", b"Integrit\xE4t wird gepr\xFCft"]),
        b"backing-up" => tr4(&[b"Sauvegarde des fichiers pr\xE9c\xE9dents", b"Backing up previous files", b"Copia de seguridad de los archivos anteriores", b"Vorherige Dateien werden gesichert"]),
        b"installing" => tr4(&[b"Installation des composants", b"Installing components", b"Instalando componentes", b"Komponenten werden installiert"]),
        b"restoring" => tr4(&[b"Restauration de la sauvegarde", b"Restoring the backup", b"Restaurando la copia de seguridad", b"Sicherung wird wiederhergestellt"]),
        b"finalizing" => tr4(&[b"Finalisation et synchronisation", b"Finishing and syncing", b"Finalizando y sincronizando", b"Abschluss und Synchronisierung"]),
        _ => tr4(&[b"Pr\xE9paration de la mise \xE0 jour", b"Preparing the update", b"Preparando la actualizaci\xF3n", b"Update wird vorbereitet"]),
    };
    text(g.hdc, g.f.large, p.text, c.left + 12, c.top + 140, title);
    let bar = rect(c.left, c.top + 188, c.right, c.top + 198);
    round_rect(
        g.hdc, g.b, bar.left, bar.top, bar.right, bar.bottom, 5, p.surface2,
    );
    let percent = number(field(PROGRESS.get(), b"percent")).min(100);
    if percent > 0 {
        round_rect(
            g.hdc,
            g.b,
            bar.left,
            bar.top,
            bar.left + (bar.right - bar.left) * percent as i32 / 100,
            bar.bottom,
            5,
            p.accent,
        );
    } else {
        let width = (bar.right - bar.left) / 5;
        let x =
            bar.left + (GetTickCount() / 12 % (bar.right - bar.left - width).max(1) as u32) as i32;
        round_rect(g.hdc, g.b, x, bar.top, x + width, bar.bottom, 5, p.accent);
    }
    row(
        g,
        c,
        238,
        tr4(&[b"\xC9l\xE9ment trait\xE9 en temps r\xE9el", b"Item being processed", b"Elemento procesado en tiempo real", b"Aktuell verarbeitetes Element"]),
        field(PROGRESS.get(), b"item"),
    );
    row(
        g,
        c,
        300,
        tr4(&[b"Fichiers termin\xE9s dans cette \xE9tape", b"Files completed in this step", b"Archivos terminados en esta etapa", b"In diesem Schritt abgeschlossene Dateien"]),
        field(PROGRESS.get(), b"done"),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        c.right - 140,
        c.top + 300,
        tr4(&[b"Nombre total", b"Total count", b"N\xFAmero total", b"Gesamtzahl"]),
    );
    text(
        g.hdc,
        g.f.body,
        p.text,
        c.right - 140,
        c.top + 322,
        field(PROGRESS.get(), b"total"),
    );
    journal(g, c, 366, LOG.get());
}
pub unsafe fn tick(_f: &Frame) -> bool {
    let mut changed = system_tick() | crate::settings_cpu::tick(PAGE == PAGE_CPU);
    if PAGE == PAGE_PRINTERS {
        changed |= printers::tick();
    }
    if updating() && GetTickCount().wrapping_sub(LAST_ANIMATION) > 90 {
        LAST_ANIMATION = GetTickCount();
        return true;
    }
    changed
}

pub unsafe fn regression_checks() {
    let saved = (STATUS, PAGE);
    PAGE = 7;
    STATUS = EMPTY;
    let status = b"update_action=reboot\nupdate_busy=0\n";
    STATUS.bytes[..status.len()].copy_from_slice(status);
    STATUS.len = status.len();
    let f = Frame {
        outer: rect(0, 0, 1100, 800),
        title: rect(0, 0, 1100, 40),
        client: rect(0, 40, 1100, 800),
    };
    let c = content(&f);
    let r = restart_rect(&c);
    check!(
        restart_required() && hit(&f, r.right - 10, r.top + 10) == 0,
        "Mise a jour terminee : le bouton Redemarrer occupe toute la ligne"
    );
    let hidden = button_rect(&c, 1, 2);
    check!(
        hit(&f, hidden.left + 5, hidden.top + 5) == -1,
        "Les anciens boutons de recherche et reinstallation ne restent pas cliquables"
    );
    let status = b"update_action=restart-session\nupdate_busy=0\n";
    STATUS = EMPTY;
    STATUS.bytes[..status.len()].copy_from_slice(status);
    STATUS.len = status.len();
    check!(
        restart_required(),
        "Une mise a jour du runtime propose aussi de redemarrer"
    );
    STATUS = EMPTY;
    check!(
        !restart_required(),
        "Sans redemarrage requis les actions normales sont conservees"
    );
    printers::regression_checks();
    (STATUS, PAGE) = saved;
    graphics::regression_checks();
    crate::settings_cpu::regression_checks();
}

pub unsafe fn regression_exe_settings(path: &[u16]) {
    open_exe_settings(path);
    check!(
        EXE_SCALE == 0 && !EXE_TRACE,
        "EXE sans profil : valeurs automatiques sans configuration obligatoire"
    );
    EXE_SCALE = 3;
    EXE_TRACE = true;
    EXE_GPU = 2;
    save_exe_settings(false);
    EXE_SCALE = 0;
    EXE_TRACE = false;
    EXE_GPU = 0;
    open_exe_settings(path);
    check!(
        EXE_SCALE == 3 && EXE_TRACE,
        "Les reglages 150 % et diagnostic sont relus pour le bon EXE"
    );
    check!(
        EXE_GPU == 2,
        "Le GPU hautes performances choisi pour un EXE est relu depuis son profil"
    );
    save_exe_settings(true);
    let mut sidecar = [0u16; 2060];
    exe_profile_path(&mut sidecar);
    check!(
        EXE_SCALE == 0 && !EXE_TRACE && GetFileAttributesW(sidecar.as_ptr()) == u32::MAX,
        "Restaurer les valeurs par defaut retire seulement le profil du programme"
    );
}
