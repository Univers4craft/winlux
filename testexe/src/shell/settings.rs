//! Paramètres Linux/Windows : navigation Mica et commandes système réelles.
//! Les actions passent par winlux-control et les états viennent du système.
#![deny(unreachable_patterns)]

use crate::gfx::*;
use crate::locale::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

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
/// Page « Graphismes et vidéo » (pilotes 3D, GPU, décodage matériel).
const PAGE_GRAPHICS: usize = 24;
const PAGE_COUNT: usize = 25;
/// Appareils Bluetooth affichés (une ligne de boutons chacun).
const BT_ROWS: usize = 5;
/// Ordre d'affichage du menu : la personnalisation suit l'apparence,
/// l'alimentation suit le son et la batterie.
const NAV_ORDER: [usize; PAGE_COUNT] = [
    0, 1, 2, 3, 23, 4, 5, 6, 21, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 24,
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
        tr(S_GRAPHICS_VIDEO),
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
static mut BUILD_INFO: Buffer = EMPTY;
/// Rapport `clé=valeur` du runtime (pilotes et préférences graphiques).
static mut GFX: Buffer = EMPTY;
static mut GFX_LOADED: bool = false;
static mut GFX_NOTICE: [u8; 256] = [0; 256];
static mut GFX_NOTICE_LEN: usize = 0;
static mut SYSUPDATES: Buffer = EMPTY;
static mut BLUETOOTH: Buffer = EMPTY;
static mut BT_CMD: [u8; 64] = [0; 64];
/// `/run/winlux/power.txt`, écrit par `winlux --power` (démon et commandes).
static mut POWER: Buffer = EMPTY;
/// Nombre de mises à jour système déjà signalé (u32::MAX : pas encore lu).
static mut NOTIFIED_UPDATES: u32 = u32::MAX;
static mut SELECTED_MODE: usize = 0;
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
static mut EXE_NOTICE: &[u8] = b"Les r\xE9glages sont facultatifs et propres \xE0 ce programme.";
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
    }
    EXE_NOTICE = b"Un double-clic lance le programme, m\xEAme sans r\xE9glages enregistr\xE9s.";
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
            EXE_NOTICE = b"Impossible de supprimer les r\xE9glages : v\xE9rifiez les permissions.";
            return;
        }
        EXE_SCALE = 0;
        EXE_TRACE = false;
        crate::settings_cpu::reset_program();
        EXE_NOTICE = b"Valeurs par d\xE9faut restaur\xE9es.";
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
        EXE_NOTICE = b"Enregistrement impossible : le dossier du programme doit \xEAtre accessible en \xE9criture.";
        return;
    }
    let mut data = [0u8; 512];
    let scale = EXE_SCALES[EXE_SCALE];
    data[..9].copy_from_slice(b"ui_scale=");
    data[9..9 + scale.len()].copy_from_slice(scale);
    let n = 9 + scale.len();
    data[n..n + 7].copy_from_slice(b"\ntrace=");
    data[n + 7] = if EXE_TRACE { b'1' } else { b'0' };
    data[n + 8] = b'\n';
    let total = n + 9 + crate::settings_cpu::write_program(&mut data[n + 9..]);
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
        b"R\xE9glages enregistr\xE9s pour le prochain lancement."
    } else {
        b"Enregistrement incomplet : r\xE9essayez."
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
    EXE_NOTICE
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
        EXE_NOTICE = b"Lancement impossible : consultez le journal de lancement Winlux.";
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
// ── Graphismes et vidéo ─────────────────────────────────────────────────
// Les réglages ne s'affichent que si le matériel les rend utiles : sans GPU
// Vulkan matériel, pas de choix de moteur ; sans décodage matériel, pas de
// choix de décodage. Hors Winlux (fonctions absentes), la page le dit.

#[derive(Clone, Copy, PartialEq, Eq)]
enum GfxSection {
    Backend,
    Gpu,
    Video,
    Vsync,
}

unsafe fn gfx_proc(name: &core::ffi::CStr) -> *const core::ffi::c_void {
    let k = LoadLibraryA(c"kernel32.dll".as_ptr());
    if k.is_null() {
        return core::ptr::null();
    }
    GetProcAddress(k, name.as_ptr())
}

unsafe fn gfx_refresh() {
    GFX_LOADED = true;
    GFX.len = 0;
    let p = gfx_proc(c"WinluxGraphicsInfo");
    if p.is_null() {
        return;
    }
    let f: unsafe extern "win64" fn(*mut u8, u32) -> u32 = core::mem::transmute(p);
    let n = f(GFX.bytes.as_mut_ptr(), CAPACITY as u32);
    GFX.len = (n as usize).min(CAPACITY - 1);
}

unsafe fn gfx(key: &[u8]) -> &'static [u8] {
    let data: &'static Buffer = &*core::ptr::addr_of!(GFX);
    field(data.get(), key)
}

unsafe fn gfx_set(key: &core::ffi::CStr, value: &[u8]) -> bool {
    let p = gfx_proc(c"WinluxSetGraphicsOption");
    if p.is_null() || value.len() > 30 {
        return false;
    }
    let mut v = [0u8; 32];
    v[..value.len()].copy_from_slice(value);
    let f: unsafe extern "win64" fn(*const core::ffi::c_char, *const core::ffi::c_char) -> i32 =
        core::mem::transmute(p);
    let ok = f(key.as_ptr(), v.as_ptr().cast()) != 0;
    gfx_refresh();
    ok
}

/// GPU Vulkan matériels : (index, nom).
unsafe fn gfx_gpus(out: &mut [(&'static [u8], &'static [u8]); 8]) -> usize {
    let mut n = 0;
    for item in gfx(b"vulkan_hw").split(|&c| c == b';') {
        let Some(colon) = item.iter().position(|&c| c == b':') else {
            continue;
        };
        if n < out.len() {
            out[n] = (&item[..colon], &item[colon + 1..]);
            n += 1;
        }
    }
    n
}

unsafe fn gfx_sections(out: &mut [GfxSection; 4]) -> usize {
    let mut gpus = [(&b""[..], &b""[..]); 8];
    let hw = gfx_gpus(&mut gpus);
    let mut n = 0;
    let mut push = |s: GfxSection| {
        out[n] = s;
        n += 1;
    };
    if hw >= 1 && gfx(b"opengl") == b"1" {
        push(GfxSection::Backend);
    }
    if hw >= 2 {
        push(GfxSection::Gpu);
    }
    if !gfx(b"video_hw_available").is_empty() {
        push(GfxSection::Video);
    }
    if hw >= 1 {
        push(GfxSection::Vsync);
    }
    n
}

fn join<'a>(out: &'a mut [u8], parts: &[&[u8]]) -> &'a [u8] {
    let mut n = 0;
    for p in parts {
        let take = p.len().min(out.len() - n);
        out[n..n + take].copy_from_slice(&p[..take]);
        n += take;
    }
    &out[..n]
}

fn pref_label(v: &[u8]) -> &'static [u8] {
    match v {
        b"vulkan" => b"Vulkan",
        b"opengl" => b"OpenGL",
        b"hardware" => b"Mat\xE9riel",
        b"software" => b"Logiciel",
        _ => b"Automatique",
    }
}

unsafe fn gfx_notice() -> &'static [u8] {
    let buf: &'static mut [u8; 256] = &mut *core::ptr::addr_of_mut!(GFX_NOTICE);
    if GFX.len == 0 {
        GFX_NOTICE_LEN = join(
            buf,
            &[b"Informations graphiques indisponibles hors de Winlux."],
        )
        .len();
        return &buf[..GFX_NOTICE_LEN];
    }
    let vk = gfx(b"vulkan") == b"1";
    let va = gfx(b"vaapi") == b"1";
    let mut gpus = [(&b""[..], &b""[..]); 8];
    let hw = gfx_gpus(&mut gpus);
    let len = join(
        buf,
        &[
            b"D\xE9tect\xE9 : OpenGL ",
            if gfx(b"opengl") == b"1" {
                b"oui"
            } else {
                b"non"
            },
            b" \xB7 Vulkan ",
            if vk {
                gfx(b"vulkan_version")
            } else {
                b"absent"
            },
            if vk && hw == 0 {
                b" (logiciel seulement)"
            } else {
                b""
            },
            b" \xB7 VA-API ",
            if va { gfx(b"vaapi_vendor") } else { b"absent" },
        ],
    )
    .len();
    GFX_NOTICE_LEN = len;
    &buf[..len]
}

unsafe fn draw_graphics(g: &mut Gfx, c: &Rect) {
    if !GFX_LOADED {
        gfx_refresh();
    }
    if GFX.len == 0 {
        row(
            g,
            c,
            44,
            b"Graphismes et vid\xE9o",
            b"Ces r\xE9glages n\xE9cessitent le runtime Winlux.",
        );
        return;
    }
    let mut sections = [GfxSection::Backend; 4];
    let n = gfx_sections(&mut sections);
    if n == 0 {
        row(
            g,
            c,
            44,
            b"Rendu et d\xE9codage vid\xE9o",
            b"Un seul mode convient \xE0 ce mat\xE9riel : Winlux l'utilise automatiquement.",
        );
        return;
    }
    let mut gpus = [(&b""[..], &b""[..]); 8];
    let hw = gfx_gpus(&mut gpus);
    for (k, s) in sections[..n].iter().enumerate() {
        let y = 44 + 132 * k as i32;
        let r = 3 * k as i32;
        let id = 9 * k as i32;
        let mut tmp = [0u8; 192];
        match s {
            GfxSection::Backend => {
                let v = join(
                    &mut tmp,
                    &[
                        pref_label(gfx(b"backend_pref")),
                        b" \x97 Direct3D 12 utilise ",
                        pref_label(gfx(b"backend_d3d12")),
                        b" (Direct3D 9 et 11 : OpenGL)",
                    ],
                );
                row(g, c, y, b"Moteur de rendu", v);
                button(g, c, r, 0, b"Automatique", id);
                button(g, c, r, 1, b"Vulkan", id + 1);
                button(g, c, r, 2, b"OpenGL", id + 2);
            }
            GfxSection::Gpu => {
                let sel = gfx(b"vulkan_selected");
                let name = gpus[..hw]
                    .iter()
                    .find(|g| g.0 == sel)
                    .map(|g| g.1)
                    .unwrap_or(b"");
                let auto = gfx(b"vulkan_device_pref") == b"auto";
                let v = join(
                    &mut tmp,
                    &[name, if auto { b" (automatique)" } else { b"" }],
                );
                row(g, c, y, b"Carte graphique (Vulkan)", v);
                button(g, c, r, 0, b"Automatique", id);
                button(g, c, r, 1, b"Pr\xE9c\xE9dente", id + 1);
                button(g, c, r, 2, b"Suivante", id + 2);
            }
            GfxSection::Video => {
                let active = gfx(b"video_active") == b"vaapi";
                let v = join(
                    &mut tmp,
                    &[
                        pref_label(gfx(b"video_pref")),
                        b" \x97 ",
                        if active {
                            b"mat\xE9riel : "
                        } else {
                            b"logiciel (mat\xE9riel possible : "
                        },
                        if active {
                            gfx(b"video_hw_codecs")
                        } else {
                            gfx(b"video_hw_available")
                        },
                        if active { b"" } else { b")" },
                    ],
                );
                row(g, c, y, b"D\xE9codage vid\xE9o", v);
                button(g, c, r, 0, b"Automatique", id);
                button(g, c, r, 1, b"Mat\xE9riel", id + 1);
                button(g, c, r, 2, b"Logiciel", id + 2);
            }
            GfxSection::Vsync => {
                let on = gfx(b"vsync") == b"1";
                row(
                    g,
                    c,
                    y,
                    b"Synchronisation verticale",
                    if on {
                        b"Activ\xE9e \x97 pas de d\xE9chirure d'image"
                    } else {
                        b"D\xE9sactiv\xE9e \x97 latence minimale"
                    },
                );
                button(g, c, r, 0, b"Activer", id);
                button(g, c, r, 1, b"D\xE9sactiver", id + 1);
            }
        }
    }
}

/// Clic sur un bouton de la page ; `true` si l'affichage doit changer.
unsafe fn click_graphics(id: i32) -> bool {
    if !(0..36).contains(&id) || id % 9 > 2 {
        return false;
    }
    let mut sections = [GfxSection::Backend; 4];
    let n = gfx_sections(&mut sections);
    let (k, col) = ((id / 9) as usize, id % 9);
    if k >= n {
        return false;
    }
    match (sections[k], col) {
        (GfxSection::Backend, _) => {
            gfx_set(
                c"backend",
                [&b"auto"[..], b"vulkan", b"opengl"][col as usize],
            );
        }
        (GfxSection::Gpu, 0) => {
            gfx_set(c"vulkan_device", b"auto");
        }
        (GfxSection::Gpu, _) => {
            let mut gpus = [(&b""[..], &b""[..]); 8];
            let hw = gfx_gpus(&mut gpus);
            let sel = gfx(b"vulkan_selected");
            let cur = gpus[..hw].iter().position(|g| g.0 == sel).unwrap_or(0);
            let next = if col == 1 {
                (cur + hw - 1) % hw
            } else {
                (cur + 1) % hw
            };
            let mut idx = [0u8; 8];
            let len = gpus[next].0.len().min(8);
            idx[..len].copy_from_slice(&gpus[next].0[..len]);
            gfx_set(c"vulkan_device", &idx[..len]);
        }
        (GfxSection::Video, _) => {
            gfx_set(
                c"video_decode",
                [&b"auto"[..], b"hardware", b"software"][col as usize],
            );
        }
        (GfxSection::Vsync, 0) => {
            gfx_set(c"vsync", b"1");
        }
        (GfxSection::Vsync, 1) => {
            gfx_set(c"vsync", b"0");
        }
        _ => return false,
    }
    true
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
            b" mise \xE0 jour syst\xE8me disponible - Param\xE8tres > Mises \xE0 jour syst\xE8me"
        } else {
            b" mises \xE0 jour syst\xE8me disponibles - Param\xE8tres > Mises \xE0 jour syst\xE8me"
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
        (b"1", b"1") => (b"bluetooth-disconnect", b"D\xE9connecter"),
        (b"1", _) => (b"bluetooth-connect", b"Connecter"),
        _ => (b"bluetooth-pair", b"Appairer"),
    }
}
unsafe fn draw_bluetooth(g: &mut Gfx, c: &Rect) {
    let bt = BLUETOOTH.get();
    let state: &[u8] = if field(bt, b"present") != b"1" {
        b"Aucun adaptateur Bluetooth d\xE9tect\xE9 (BlueZ)."
    } else if field(bt, b"powered") != b"1" {
        b"D\xE9sactiv\xE9"
    } else if field(bt, b"discovering") == b"1" {
        b"Activ\xE9 - recherche d'appareils en cours..."
    } else {
        b"Activ\xE9 - visible pour les appareils \xE0 proximit\xE9"
    };
    row(g, c, 44, b"Bluetooth", state);
    button(g, c, 0, 0, b"Activer", 0);
    button(g, c, 0, 1, b"D\xE9sactiver", 1);
    button(g, c, 0, 2, b"Rechercher des appareils", 2);
    let mut shown = 0;
    for k in 0..BT_ROWS {
        let Some(dev) = bt_device(bt, k) else { break };
        let mut label = [0u8; 96];
        let mut n = 0;
        let kind: &[u8] = match bt_field(dev, 2) {
            b"audio" => b" (audio)",
            b"clavier" => b" (clavier)",
            b"souris" => b" (souris)",
            b"manette" => b" (manette)",
            b"telephone" => b" (t\xE9l\xE9phone)",
            _ => b"",
        };
        let status: &[u8] = if bt_field(dev, 4) == b"1" {
            b" - connect\xE9"
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
            button(g, c, row_i, 2, b"Oublier", id + 2);
        }
        shown += 1;
    }
    let y = 108 + (shown.max(1) + 1) * 44 + 16;
    if shown == 0 && field(bt, b"powered") == b"1" {
        row(
            g,
            c,
            108 + 44,
            b"Appareils",
            b"Aucun appareil : lancez une recherche et mettez l'appareil en mode appairage.",
        );
    }
    let mut summary = [0u8; 96];
    let mut n = 0;
    for part in [
        b"Manettes : ".as_slice(),
        value(b"gamepads"),
        b"   Webcams : ".as_slice(),
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
        b"P\xE9riph\xE9riques des programmes Windows",
        &summary[..n],
    );
    row(g, c, y + 60, b"Micro par d\xE9faut", value(b"microphone"));
    journal(g, c, y + 124, LOG.get());
}
pub unsafe fn control(action: &core::ffi::CStr) -> bool {
    if (updating() || sysupdate_busy())
        && matches!(
            action.to_bytes(),
            b"poweroff" | b"reboot" | b"restart-session" | b"update-apply" | b"update-rollback"
        )
    {
        crate::desktop::toast(b"Installation en cours : attendez la fin avant de red\xE9marrer.");
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
        let s = b"Operation en cours...";
        ACTION.bytes[..s.len()].copy_from_slice(s);
        ACTION.len = s.len();
        LAST_POLL = 0;
        true
    } else {
        crate::desktop::toast(b"Le service de contr\xF4le Winlux n'est pas disponible.");
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
    let step = ((f.client.bottom - f.client.top - 75) / PAGE_COUNT as i32).clamp(26, 42);
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
        (4, 0) => value(b"layout") == b"fr",
        (4, 1) => value(b"layout") == b"us",
        (4, 3) => value(b"tap") == b"1",
        (4, 4) => value(b"tap") == b"0",
        (5, 0) => unsafe { MOUSE_SPEED == 25 },
        (5, 1) => unsafe { MOUSE_SPEED == 50 },
        (5, 2) => unsafe { MOUSE_SPEED == 75 },
        (6, 0) => value(b"align") == b"left",
        (6, 1) => value(b"align") == b"center",
        (6, 3) => value(b"theme") == b"dark",
        (6, 4) => value(b"theme") == b"light",
        (8, 3) => value(b"display_policy") == b"fixed",
        (8, 4) => value(b"display_policy") != b"fixed",
        (10, 0..=4) => EXE_SCALE == id as usize,
        (10, 5) => EXE_TRACE,
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
            b"Information indisponible"
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
        b"Param\xE8tres",
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
        text(g.hdc, g.f.body, p.text, r.left + 8, r.top + 9, &name[..end]);
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
            row(g, &c, 44, b"Rapidit\xE9 de la souris", speed_str.as_bytes());
            button(g, &c, 0, 0, b"Lente", 0);
            button(g, &c, 0, 1, b"Normale", 1);
            button(g, &c, 0, 2, b"Rapide", 2);
        }
        12 => crate::a11y::page_draw(g, &c),
        13 => {
            row(g, &c, 44, b"Effets visuels", b"Options d'effet visuels.");
            row(
                g,
                &c,
                84,
                b"Effet de flou",
                if unsafe { ENABLE_BLUR } {
                    b"Activ\xE9"
                } else {
                    b"D\xE9sactiv\xE9"
                },
            );
            button(g, &c, 0, 0, b"Activer", 0);
            button(g, &c, 0, 1, b"D\xE9sactiver", 1);
            row(
                g,
                &c,
                124,
                b"Animations fluides",
                if crate::gfx::animations() {
                    b"Activ\xE9es"
                } else {
                    b"D\xE9sactiv\xE9es"
                },
            );
            button(g, &c, 1, 0, b"Activer", 3);
            button(g, &c, 1, 1, b"D\xE9sactiver", 4);
            row(
                g,
                &c,
                164,
                b"Miniatures des fen\xEAtres",
                if unsafe { ENABLE_THUMBNAILS } {
                    b"Activ\xE9es"
                } else {
                    b"D\xE9sactiv\xE9es"
                },
            );
            button(g, &c, 2, 0, b"Activer", 6);
            button(g, &c, 2, 1, b"D\xE9sactiver", 7);
            row(
                g,
                &c,
                204,
                b"Th\xE8me automatique",
                if unsafe { AUTO_THEME } {
                    b"Activ\xE9"
                } else {
                    b"D\xE9sactiv\xE9"
                },
            );
            button(g, &c, 3, 0, b"Activer", 9);
            button(g, &c, 3, 1, b"D\xE9sactiver", 10);
        }
        11 => {
            row(
                g,
                &c,
                44,
                b"Navigateur par d\xE9faut",
                match value(b"browser_default") {
                    b"chrome" => b"Google Chrome",
                    b"firefox" => b"Mozilla Firefox",
                    _ => b"Microsoft Edge",
                },
            );
            for (i, labels) in [
                [
                    b"Installer Edge".as_slice(),
                    b"Choisir Edge",
                    b"Ouvrir Edge",
                ],
                [
                    b"Installer Chrome".as_slice(),
                    b"Choisir Chrome",
                    b"Ouvrir Chrome",
                ],
                [
                    b"Installer Firefox".as_slice(),
                    b"Choisir Firefox",
                    b"Ouvrir Firefox",
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
                        b"installed" => b"Install\xE9",
                        b"available" => b"Paquet pr\xEAt - installation hors ligne",
                        b"update" => b"Mise \xE0 jour disponible",
                        _ => b"Paquet absent de cette image",
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
                    b"Choisir un programme",
                    b"Clic droit sur un .exe dans Explorer, puis R\xE9glages de l'ex\xE9cutable.",
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
                b"\xC9chelle auto".as_slice(),
                b"100 %",
                b"125 %",
                b"150 %",
                b"200 %",
                b"Journal diagnostic",
                b"Enregistrer",
                b"Valeurs par d\xE9faut",
                b"Lancer le .exe",
                b"Processeur et priorit\xE9\x85",
            ]
            .iter()
            .enumerate()
            {
                if i == 9 && !crate::settings_cpu::available() {
                    continue;
                }
                button(g, &c, i as i32 / 3, i as i32 % 3, label, i as i32);
            }
            row(
                g,
                &c,
                300,
                b"Application des r\xE9glages",
                b"Enregistrer puis relancer le programme. Aucun profil n'est obligatoire.",
            );
            row(g, &c, 366, b"P\xE9rim\xE8tre", b"\xC9chelle du rendu Winlux et traces ; les API manquantes restent \xE0 impl\xE9menter.");
        }
        9 => {
            let info = BUILD_INFO.get();
            let version = field(info, b"version");
            row(
                g,
                &c,
                44,
                b"Version de Winlux",
                if version.is_empty() {
                    value(b"version")
                } else {
                    version
                },
            );
            row(g, &c, 104, b"Commit GitHub", field(info, b"commit"));
            row(
                g,
                &c,
                164,
                b"Construction (UTC)",
                field(info, b"built_at_utc"),
            );
            row(
                g,
                &c,
                224,
                b"Type de livrable",
                match field(info, b"scope") {
                    b"system" => b"Syst\xE8me complet",
                    b"runtime" => b"Runtime uniquement",
                    _ => b"Information indisponible",
                },
            );
            row(
                g,
                &c,
                284,
                b"Sources suivies lors de la construction",
                match field(info, b"tracked_sources") {
                    b"clean" => b"Sans modification locale suivie",
                    b"modified" => b"Modifications locales non valid\xE9es",
                    _ => b"Information indisponible",
                },
            );
            row(
                g,
                &c,
                344,
                b"Provenance",
                if info.is_empty() {
                    b"BUILD-INFO absent : ancienne image ou installation sans m\xE9tadonn\xE9es."
                } else {
                    b"Informations du livrable install\xE9, pas de la derni\xE8re release GitHub."
                },
            );
        }
        0 => {
            row(g, &c, 44, b"Version de Winlux", value(b"version"));
            button(g, &c, 0, 0, b"Relancer le bureau", 0);
            button(g, &c, 0, 1, b"Red\xE9marrer", 1);
            button(g, &c, 0, 2, b"Arr\xEAter", 2);
            row(g, &c, 170, b"Noyau Linux", value(b"kernel"));
            row(
                g,
                &c,
                230,
                b"P\xE9riph\xE9riques d'entr\xE9e d\xE9tect\xE9s",
                value(b"input_devices"),
            );
            row(
                g,
                &c,
                290,
                b"Stockage du syst\xE8me",
                if value(b"persistent") == b"1" {
                    b"Disque persistant : r\xE9glages et mises \xE0 jour conserv\xE9s."
                } else {
                    b"Session Live en RAM : modifications perdues au red\xE9marrage."
                },
            );
            journal(g, &c, 356, LOG.get());
        }
        1 => {
            row(
                g,
                &c,
                44,
                b"Couche Windows / Win32",
                b"Runtime Winlux - .exe via Explorer",
            );
            button(g, &c, 0, 0, b"Programmes .exe", 0);
            button(g, &c, 0, 1, b"Dossier personnel", 1);
            button(g, &c, 0, 2, b"Mises \xE0 jour", 2);
            row(
                g,
                &c,
                170,
                b"Installer une application Windows",
                b"Ouvrir son installateur .exe dans Explorer.",
            );
            row(
                g,
                &c,
                228,
                b"Linux et Windows",
                b"Les .exe partagent la session Linux et les fichiers.",
            );
            row(
                g,
                &c,
                286,
                b"Compatibilit\xE9",
                b"Le support d\xE9pend des API et composants requis par le logiciel.",
            );
        }
        2 => {
            row(g, &c, 44, b"Connexion / adresse IPv4", value(b"network"));
            button(g, &c, 0, 0, b"Renouveler DHCP", 0);
            button(g, &c, 0, 1, b"Actualiser", 1);
            row(
                g,
                &c,
                170,
                b"Machine virtuelle",
                b"Ethernet virtuel ; le Wi-Fi reste g\xE9r\xE9 par l'h\xF4te.",
            );
            journal(g, &c, 236, LOG.get());
        }
        PAGE_POWER => draw_power(g, &c),
        3 => {
            row(g, &c, 44, b"Sortie audio / volume", value(b"audio"));
            button(g, &c, 0, 0, b"Volume -", 0);
            button(g, &c, 0, 1, b"Muet / r\xE9tablir", 1);
            button(g, &c, 0, 2, b"Volume +", 2);
            row(
                g,
                &c,
                170,
                b"Batterie et alimentation",
                b"Modes d'alimentation, veille et luminosit\xE9 : page Alimentation.",
            );
            button(g, &c, 3, 0, b"Options d'alimentation", 9);
            journal(g, &c, 280, LOG.get());
        }
        4 => {
            row(g, &c, 44, b"Disposition actuelle", value(b"layout"));
            button(g, &c, 0, 0, b"AZERTY fran\xE7ais", 0);
            button(g, &c, 0, 1, b"QWERTY US", 1);
            button(g, &c, 1, 0, b"Activer toucher-clic", 3);
            button(g, &c, 1, 1, b"D\xE9sactiver", 4);
            row(
                g,
                &c,
                214,
                b"Toucher-clic (0 = non, 1 = oui)",
                value(b"tap"),
            );
            row(
                g,
                &c,
                274,
                b"Pav\xE9 tactile en VM",
                b"SPICE expose souvent une tablette, pas le pav\xE9 physique.",
            );
            journal(g, &c, 336, LOG.get());
        }
        6 => {
            row(
                g,
                &c,
                44,
                b"Barre des t\xE2ches",
                b"Pr\xE9f\xE9rences enregistr\xE9es pour la prochaine session.",
            );
            button(g, &c, 0, 0, b"Aligner \xE0 gauche", 0);
            button(g, &c, 0, 1, b"Centrer", 1);
            button(g, &c, 1, 0, b"Th\xE8me sombre", 3);
            button(g, &c, 1, 1, b"Th\xE8me clair", 4);
            row(
                g,
                &c,
                164,
                b"Th\xE8me automatique",
                if unsafe { AUTO_THEME } {
                    b"Jour clair / nuit sombre (8h\x9620h)"
                } else {
                    b"D\xE9sactiv\xE9"
                },
            );
            button(g, &c, 2, 0, b"Activer", 5);
            button(g, &c, 2, 1, b"D\xE9sactiver", 6);
            row(
                g,
                &c,
                254,
                b"Stockage",
                if value(b"persistent") == b"1" {
                    b"Les pr\xE9f\xE9rences sont conserv\xE9es sur le disque."
                } else {
                    b"Session Live : les r\xE9glages durent jusqu'au red\xE9marrage."
                },
            );
            row(
                g,
                &c,
                300,
                b"Fond d'\xE9\x63ran, couleurs, barre des t\xE2\x63hes, ic\xF4nes et sons",
                b"Galerie et aper\xE7u dans la page Personnalisation.",
            );
            button(g, &c, 5, 0, b"Personnaliser", 12);
            button(g, &c, 5, 1, b"Assistant de bienvenue", 13);
            button(g, &c, 7, 0, b"R\xE9glages de l'\xE9cran", 18);
        }
        8 => {
            row(
                g,
                &c,
                44,
                b"D\xE9finition actuellement appliqu\xE9e",
                value(b"display_current"),
            );
            let modes = value(b"display_modes");
            let count = modes
                .split(|&v| v == b',')
                .filter(|v| !v.is_empty())
                .count();
            if count > 0 {
                SELECTED_MODE = SELECTED_MODE.min(count - 1);
            }
            let chosen = modes
                .split(|&v| v == b',')
                .nth(SELECTED_MODE)
                .unwrap_or(b"Indisponible");
            button(g, &c, 0, 0, b"D\xE9finition pr\xE9c\xE9dente", 0);
            button(g, &c, 0, 1, b"D\xE9finition suivante", 1);
            button(g, &c, 0, 2, b"Appliquer", 2);
            button(g, &c, 1, 0, b"Image fixe", 3);
            button(g, &c, 1, 1, b"Suivre la fen\xEAtre VM", 4);
            button(g, &c, 1, 2, b"Actualiser", 5);
            row(
                g,
                &c,
                214,
                b"D\xE9finition s\xE9lectionn\xE9e (modes r\xE9els de l'\xE9cran)",
                chosen,
            );
            row(
                g,
                &c,
                274,
                b"\xC9chelle du texte et des contr\xF4les",
                match value(b"display_scale") {
                    b"1" => b"100 %",
                    b"1.25" => b"125 %",
                    b"1.5" => b"150 %",
                    b"2" => b"200 %",
                    _ => b"Automatique",
                },
            );
            button(g, &c, 5, 0, b"\xC9chelle -", 15);
            button(g, &c, 5, 1, b"Automatique", 16);
            button(g, &c, 5, 2, b"\xC9chelle +", 17);
            row(
                g,
                &c,
                420,
                b"Application de l'\xE9chelle",
                b"Relancez le bureau apr\xE8s modification de l'\xE9chelle.",
            );
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
                    b"Installation termin\xE9e",
                    b"Un red\xE9marrage est n\xE9cessaire pour terminer la mise \xE0 jour.",
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
                    b"Red\xE9marrer pour terminer la mise \xE0 jour",
                );
                journal(g, &c, 198, LOG.get());
                return;
            }
            if restart == b"reboot" {
                row(
                    g,
                    &c,
                    44,
                    b"Installation termin\xE9e - red\xE9marrage requis",
                    b"Red\xE9marrez maintenant, ou \xE9teignez puis rallumez la machine.",
                );
            } else if field(PROGRESS.get(), b"phase") == b"error" {
                row(
                    g,
                    &c,
                    44,
                    b"L'installation n'a pas abouti",
                    b"Consultez le journal avant de r\xE9essayer.",
                );
            } else {
                row(
                    g,
                    &c,
                    44,
                    b"Mises \xE0 jour Winlux",
                    if value(b"update_auto") == b"enabled" {
                        b"Recherche automatique activ\xE9e."
                    } else {
                        b"Installation sur demande."
                    },
                );
            }
            button(g, &c, 0, 0, b"Rechercher", 0);
            if !already_up_to_date() {
                button(g, &c, 0, 1, b"Tout installer", 1);
            }
            button(g, &c, 0, 2, b"Restaurer", 2);
            button(
                g,
                &c,
                1,
                0,
                match value(b"sysupdate_mode") {
                    b"auto" => b"Mode : automatique",
                    b"scheduled" => b"Mode : planifi\xE9",
                    _ => b"Mode : manuel",
                },
                3,
            );
            button(
                g,
                &c,
                1,
                1,
                if SHOW_LOG { b"Masquer" } else { b"Journal" },
                4,
            );
            if restart == b"reboot" {
                button(g, &c, 1, 2, b"Red\xE9marrer", 5);
            }
            text(g.hdc, g.f.small, p.text3, c.left, c.top + 208, b"Les mises \xE0 jour sont t\xE9l\xE9charg\xE9es depuis les releases publiques GitHub.");
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                c.left,
                c.top + 244,
                if SHOW_LOG {
                    b"Journal d'installation"
                } else {
                    b"Modifications apport\xE9es par la mise \xE0 jour"
                },
            );
            journal(
                g,
                &c,
                270,
                if SHOW_LOG {
                    LOG.get()
                } else if UPDATES.len == 0 {
                    b"Cliquez Rechercher pour afficher les modifications.\nMolette ou touches Haut/Bas pour lire la suite."
                } else {
                    UPDATES.get()
                },
            );
            text(g.hdc, g.f.small, p.text3, c.left, c.bottom - 30, b"Molette / Haut-Bas : d\xE9filer ; Page suivante/pr\xE9c\xE9dente : lecture rapide.");
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
                b"Gestionnaire de fichiers",
                b"Pr\xE9f\xE9rences de l'explorateur de fichiers Winlux.",
            );
            row(
                g,
                &c,
                104,
                b"Affichage par d\xE9faut",
                match unsafe { crate::explorer::current_view() } {
                    0 => b"Tr\xE8s grandes ic\xF4nes",
                    1 => b"Grandes ic\xF4nes",
                    2 => b"Ic\xF4nes moyennes",
                    3 => b"Petites ic\xF4nes",
                    4 => b"Liste",
                    5 => b"D\xE9tails",
                    _ => b"Mosa\xEFques",
                },
            );
            button(g, &c, 0, 0, b"Grandes ic\xF4nes", 0);
            button(g, &c, 0, 1, b"D\xE9tails", 1);
            button(g, &c, 0, 2, b"Mosa\xEFques", 2);
            row(
                g,
                &c,
                230,
                b"\xC9l\xE9ments masqu\xE9s",
                if unsafe { crate::explorer::show_hidden() } {
                    b"Visibles"
                } else {
                    b"Masqu\xE9s"
                },
            );
            button(g, &c, 2, 0, b"Afficher", 3);
            button(g, &c, 2, 1, b"Masquer", 4);
            row(
                g,
                &c,
                356,
                b"Extensions de noms de fichiers",
                if unsafe { crate::explorer::show_ext() } {
                    b"Affich\xE9es"
                } else {
                    b"Masqu\xE9es"
                },
            );
            button(g, &c, 4, 0, b"Afficher", 6);
            button(g, &c, 4, 1, b"Masquer", 7);
        }
        16 => {
            row(
                g,
                &c,
                44,
                b"P\xE9riph\xE9rique de sortie",
                value(b"audio_output"),
            );
            button(g, &c, 0, 0, b"Haut-parleurs", 0);
            button(g, &c, 0, 1, b"Casque / HDMI", 1);
            button(g, &c, 0, 2, b"Actualiser", 2);
            row(
                g,
                &c,
                170,
                b"P\xE9riph\xE9rique d'entr\xE9e",
                value(b"audio_input"),
            );
            button(g, &c, 1, 0, b"Micro int\xE9gr\xE9", 3);
            button(g, &c, 1, 1, b"Micro externe", 4);
            row(g, &c, 280, b"Volume principal", value(b"audio"));
            button(g, &c, 3, 0, b"Volume -", 6);
            button(g, &c, 3, 1, b"Muet / r\xE9tablir", 7);
            button(g, &c, 3, 2, b"Volume +", 8);
            row(
                g,
                &c,
                370,
                b"Volume par application",
                b"Chaque fen\xEAtre ouverte a son propre niveau de volume.",
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
        PAGE_SYSUPDATE => draw_sysupdate(g, &c),
        PAGE_PRIVACY => draw_privacy(g, &c),
        PAGE_CPU => crate::settings_cpu::draw(g, &c),
        PAGE_NETDRIVES => crate::netdrives::draw(g, &c),
        BLUETOOTH_PAGE => draw_bluetooth(g, &c),
        PAGE_GRAPHICS => draw_graphics(g, &c),
        _ => {}
    }
    if PAGE == PAGE_NETDRIVES {
        return;
    }
    let notice = if PAGE == 10 {
        EXE_NOTICE
    } else if PAGE == PAGE_GRAPHICS {
        gfx_notice()
    } else if !available() {
        b"Service Winlux indisponible : les actions n\xE9cessitent Winlux.".as_slice()
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
            0 => self.push(b"Jamais"),
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
        b"nothing" => b"Ne rien faire",
        b"sleep" => b"Veille",
        b"hibernate" => b"Veille prolong\xE9e",
        b"shutdown" => b"Arr\xEAter",
        b"display-off" => b"\xC9teindre l'\xE9cran",
        _ => b"?",
    }
}

/// Page « Alimentation et batterie » : état, modes, délais, capot/bouton,
/// luminosité. Les valeurs viennent de `/run/winlux/power.txt`.
unsafe fn draw_power(g: &mut Gfx, c: &Rect) {
    let mut battery = Line::new();
    if pvalue(b"power_battery").is_empty() {
        battery.push(b"Service d'alimentation Winlux non d\xE9marr\xE9");
    } else if pvalue(b"power_battery") == b"0" {
        battery.push(b"Alimentation secteur - aucune batterie d\xE9tect\xE9e");
    } else {
        battery.num(number(pvalue(b"power_percent"))).push(b" % - ");
        let left = pvalue(b"power_seconds_left");
        let full = pvalue(b"power_seconds_to_full");
        match pvalue(b"power_state") {
            b"charging" => {
                battery.push(b"En charge");
                if !full.is_empty() {
                    battery.push(b", pleine dans ").duration(number(full));
                }
            }
            b"discharging" => {
                battery.push(b"Sur batterie");
                if !left.is_empty() {
                    battery
                        .push(b", ")
                        .duration(number(left))
                        .push(b" restantes");
                }
            }
            b"full" => {
                battery.push(b"Charg\xE9e");
            }
            b"not-charging" => {
                battery.push(b"Branch\xE9e, pas en charge");
            }
            _ => {
                battery.push(b"\xC9tat inconnu");
            }
        }
        if !pvalue(b"power_health").is_empty() {
            battery
                .push(b" - sant\xE9 ")
                .num(number(pvalue(b"power_health")))
                .push(b" %");
        }
    }
    row(g, c, 44, b"Batterie et mode d'alimentation", battery.get());
    button(g, c, 0, 0, b"Performances maximales", 0);
    button(g, c, 0, 1, b"\xC9quilibr\xE9", 1);
    button(g, c, 0, 2, b"\xC9conomie d'\xE9nergie", 2);

    let mut now = Line::new();
    now.push(b"Appliqu\xE9 maintenant : \xE9cran ")
        .push(pvalue(b"power_screen_now"))
        .push(b", veille ")
        .push(pvalue(b"power_sleep_now"));
    row(
        g,
        c,
        176,
        b"\xC9cran et mise en veille (cliquer pour changer)",
        now.get(),
    );
    let mut l = Line::new();
    l.push(b"\xC9cran secteur : ")
        .delay(pvalue(b"power_screen_ac"));
    button(g, c, 3, 0, l.get(), 9);
    let mut l = Line::new();
    l.push(b"\xC9cran batterie : ")
        .delay(pvalue(b"power_screen_dc"));
    button(g, c, 3, 1, l.get(), 10);
    if pvalue(b"power_can_sleep") == b"1" {
        button(g, c, 3, 2, b"Mettre en veille", 11);
    }
    let mut l = Line::new();
    l.push(b"Veille secteur : ")
        .delay(pvalue(b"power_sleep_ac"));
    button(g, c, 4, 0, l.get(), 12);
    let mut l = Line::new();
    l.push(b"Veille batterie : ")
        .delay(pvalue(b"power_sleep_dc"));
    button(g, c, 4, 1, l.get(), 13);

    let mut l = Line::new();
    l.push(b"Luminosit\xE9 ");
    if pvalue(b"power_brightness").is_empty() {
        l.push(b"non r\xE9glable ici (VM ou \xE9cran externe)");
    } else {
        l.num(number(pvalue(b"power_brightness"))).push(b" %");
        if pvalue(b"power_profile") == b"saver" {
            l.push(b" (mode \xE9conomie)");
        }
    }
    l.push(if pvalue(b"power_can_hibernate") == b"1" {
        b" - veille prolong\xE9e pr\xEAte" as &[u8]
    } else {
        b" - veille prolong\xE9e \xE0 pr\xE9parer"
    });
    row(
        g,
        c,
        352,
        b"Capot, bouton d'alimentation et luminosit\xE9",
        l.get(),
    );
    let mut l = Line::new();
    l.push(b"Capot : ").push(action_label(pvalue(b"power_lid")));
    button(g, c, 7, 0, l.get(), 21);
    let mut l = Line::new();
    l.push(b"Bouton : ")
        .push(action_label(pvalue(b"power_button")));
    button(g, c, 7, 1, l.get(), 22);
    if pvalue(b"power_can_hibernate") == b"1" {
        button(g, c, 7, 2, b"Veille prolong\xE9e", 23);
    }
    button(g, c, 8, 0, b"Luminosit\xE9 -", 24);
    button(g, c, 8, 1, b"Luminosit\xE9 +", 25);
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
    // Neuf rangées de trois boutons (la page Alimentation va jusqu'à 8),
    // douze pour la page Graphismes.
    let count = if unsafe { PAGE == PAGE_GRAPHICS } {
        36
    } else {
        27
    };
    for i in 0..count {
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
                GFX_LOADED = false;
            }
            PAGE = i;
            SCROLL = 0;
            HOVER = -1;
            LAST_POLL = 0;
            system_tick();
            return true;
        }
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
    if PAGE == PAGE_GRAPHICS {
        let id = hit(f, x, y);
        return click_graphics(id);
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
        (4, 0) => Some(c"keyboard-fr"),
        (4, 1) => Some(c"keyboard-us"),
        (4, 3) => Some(c"tap-on"),
        (4, 4) => Some(c"tap-off"),
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
        (8, 0) => {
            SELECTED_MODE = SELECTED_MODE.saturating_sub(1);
            return true;
        }
        (8, 1) => {
            let n = value(b"display_modes").split(|&v| v == b',').count();
            SELECTED_MODE = (SELECTED_MODE + 1).min(n.saturating_sub(1));
            return true;
        }
        (8, 2) => {
            let chosen = value(b"display_modes")
                .split(|&v| v == b',')
                .nth(SELECTED_MODE)
                .unwrap_or(b"");
            if chosen.is_empty() || chosen.len() > 24 {
                return true;
            }
            let mut command = [0u8; 64];
            let prefix = b"display-mode-";
            command[..prefix.len()].copy_from_slice(prefix);
            command[prefix.len()..prefix.len() + chosen.len()].copy_from_slice(chosen);
            if let Ok(cstr) = core::ffi::CStr::from_bytes_until_nul(&command) {
                control(cstr);
            }
            return true;
        }
        (8, 3) => Some(c"display-fixed"),
        (8, 4) => Some(c"display-follow"),
        (8, 5) => Some(c"status"),
        (8, 15) => Some(c"display-scale-down"),
        (8, 16) => Some(c"display-scale-auto"),
        (8, 17) => Some(c"display-scale-up"),
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
        b"Mode de mise \xE0 jour",
        match mode {
            b"auto" => b"Automatique : recherche et installation chaque jour, sans intervention.",
            b"scheduled" => b"Planifi\xE9 : installation au cr\xE9neau choisi ci-dessous.",
            _ => b"Manuel : recherche chaque jour et notification ; vous installez quand vous voulez.",
        },
    );
    button(g, c, 0, 0, b"Automatique", 0);
    button(g, c, 0, 1, b"Manuel", 1);
    button(g, c, 0, 2, b"Planifi\xE9", 2);
    let mut slot = [0u8; 64];
    let mut n = push(
        &mut slot,
        0,
        match value(b"sysupdate_day") {
            b"mon" => b"Chaque lundi",
            b"tue" => b"Chaque mardi",
            b"wed" => b"Chaque mercredi",
            b"thu" => b"Chaque jeudi",
            b"fri" => b"Chaque vendredi",
            b"sat" => b"Chaque samedi",
            b"sun" => b"Chaque dimanche",
            _ => b"Tous les jours",
        },
    );
    n = push(&mut slot, n, b" \xE0 ");
    let hour = number(value(b"sysupdate_hour")).min(23);
    if hour < 10 {
        n = push(&mut slot, n, b"0");
    }
    n = push_number(&mut slot, n, hour);
    n = push(&mut slot, n, b":00");
    if mode != b"scheduled" {
        n = push(&mut slot, n, b" (utilis\xE9 en mode Planifi\xE9)");
    }
    row(g, c, 166, b"Cr\xE9neau planifi\xE9", &slot[..n]);
    button(g, c, 3, 0, b"Heure -", 9);
    button(g, c, 3, 1, b"Heure +", 10);
    button(g, c, 3, 2, b"Changer le jour", 11);
    let count = number(value(b"sysupdate_count"));
    let mut summary = [0u8; 128];
    let mut n = 0;
    if value(b"sysupdate_apt") != b"1" {
        n = push(&mut summary, n, b"APT absent de cette session Live : seul Winlux est mis \xE0 jour (page Mises \xE0 jour).");
    } else if sysupdate_busy() {
        n = push(
            &mut summary,
            n,
            b"Installation des paquets en cours : n'\xE9teignez pas la machine.",
        );
    } else {
        if count == 0 {
            n = push(&mut summary, n, b"Syst\xE8me \xE0 jour");
        } else {
            n = push_number(&mut summary, n, count);
            n = push(
                &mut summary,
                n,
                if count == 1 {
                    b" paquet \xE0 mettre \xE0 jour"
                } else {
                    b" paquets \xE0 mettre \xE0 jour"
                },
            );
        }
        n = push(&mut summary, n, b" - derni\xE8re recherche : ");
        n = push(&mut summary, n, value(b"sysupdate_last"));
    }
    row(
        g,
        c,
        300,
        b"Paquets du syst\xE8me Linux (APT)",
        &summary[..n],
    );
    button(g, c, 6, 0, b"Rechercher", 18);
    button(g, c, 6, 1, b"Tout installer", 19);
    button(
        g,
        c,
        6,
        2,
        if SHOW_LOG {
            b"Liste des paquets"
        } else {
            b"Journal"
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
            b"Journal APT"
        } else {
            b"Paquets concern\xE9s"
        },
    );
    journal(
        g,
        c,
        444,
        if SHOW_LOG {
            LOG.get()
        } else if SYSUPDATES.len == 0 {
            b"Aucun paquet en attente. Cliquez Rechercher pour interroger les d\xE9p\xF4ts APT."
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
        b"T\xE9l\xE9m\xE9trie et donn\xE9es de diagnostic",
        if value(b"privacy_telemetry") == b"on" {
            b"Autoris\xE9e : aucun composant actuel de Winlux n'envoie pourtant de donn\xE9es."
        } else {
            b"D\xE9sactiv\xE9e : Winlux n'envoie aucune donn\xE9e d'utilisation."
        },
    );
    button(g, c, 0, 0, b"D\xE9sactiv\xE9e", 0);
    button(g, c, 0, 1, b"Autoris\xE9e", 1);
    row(
        g,
        c,
        166,
        b"Autorisations des applications Windows (.exe)",
        b"Bouton en surbrillance : acc\xE8s autoris\xE9. Cliquez pour bloquer ou autoriser.",
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
            b"Microphone : autoris\xE9"
        } else {
            b"Microphone : bloqu\xE9"
        },
        9,
    );
    button(
        g,
        c,
        3,
        1,
        if state(b"privacy_camera", true) {
            b"Cam\xE9ra : autoris\xE9e"
        } else {
            b"Cam\xE9ra : bloqu\xE9e"
        },
        10,
    );
    button(
        g,
        c,
        3,
        2,
        if state(b"privacy_location", false) {
            b"Localisation : autoris\xE9e"
        } else {
            b"Localisation : bloqu\xE9e"
        },
        11,
    );
    row(
        g,
        c,
        300,
        b"Historique de localisation",
        if value(b"privacy_location_history") == b"on" {
            b"Activ\xE9 : conserv\xE9 uniquement sur cet ordinateur."
        } else {
            b"D\xE9sactiv\xE9 : aucune position n'est enregistr\xE9e."
        },
    );
    button(g, c, 6, 0, b"D\xE9sactiv\xE9", 18);
    button(g, c, 6, 1, b"Activ\xE9", 19);
    row(
        g,
        c,
        430,
        b"Vos donn\xE9es restent chez vous",
        b"Un micro refus\xE9 est bloqu\xE9 par le runtime ; cam\xE9ra et localisation ne sont pas encore expos\xE9es aux .exe.",
    );
}
unsafe fn draw_installation(g: &mut Gfx, c: &Rect) {
    let p = pal();
    row(
        g,
        c,
        54,
        b"Installation en cours",
        b"N'\xE9teignez pas la machine. L'arr\xEAt est temporairement verrouill\xE9.",
    );
    let phase = field(PROGRESS.get(), b"phase");
    let title: &[u8] = match phase {
        b"downloading" => b"T\xE9l\xE9chargement du paquet",
        b"verifying" => b"V\xE9rification de l'int\xE9grit\xE9",
        b"backing-up" => b"Sauvegarde des fichiers pr\xE9c\xE9dents",
        b"installing" => b"Installation des composants",
        b"restoring" => b"Restauration de la sauvegarde",
        b"finalizing" => b"Finalisation et synchronisation",
        _ => b"Pr\xE9paration de la mise \xE0 jour",
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
        b"\xC9l\xE9ment trait\xE9 en temps r\xE9el",
        field(PROGRESS.get(), b"item"),
    );
    row(
        g,
        c,
        300,
        b"Fichiers termin\xE9s dans cette \xE9tape",
        field(PROGRESS.get(), b"done"),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        c.right - 140,
        c.top + 300,
        b"Nombre total",
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
    let changed = system_tick() | crate::settings_cpu::tick(PAGE == PAGE_CPU);
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
    (STATUS, PAGE) = saved;
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
    save_exe_settings(false);
    EXE_SCALE = 0;
    EXE_TRACE = false;
    open_exe_settings(path);
    check!(
        EXE_SCALE == 3 && EXE_TRACE,
        "Les reglages 150 % et diagnostic sont relus pour le bon EXE"
    );
    save_exe_settings(true);
    let mut sidecar = [0u16; 2060];
    exe_profile_path(&mut sidecar);
    check!(
        EXE_SCALE == 0 && !EXE_TRACE && GetFileAttributesW(sidecar.as_ptr()) == u32::MAX,
        "Restaurer les valeurs par defaut retire seulement le profil du programme"
    );
}
