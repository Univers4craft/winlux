#![allow(non_snake_case)]
// Pilotes Windows virtualisés (#1532, fille de #1531). Un installeur
// constructeur pose son pilote (.inf + .sys) avec SetupCopyOEMInf,
// DiInstallDriver, UpdateDriverForPlugAndPlayDevices (newdev.dll) ou
// CreateService(SERVICE_KERNEL_DRIVER) puis StartService. Winlux :
//   - copie le .inf dans son magasin de pilotes (C:\Windows\INF\oemN.inf) ;
//   - inscrit les services AddService du .inf, comme Windows ;
//   - ne charge ni n'exécute JAMAIS le .sys : quand une traduction existe
//     pour ce pilote (table par marque, #1534), « démarrer » le service rend
//     visible l'appareil virtuel de vdev.rs ; sinon le service reste arrêté
//     avec ERROR_DRIVER_BLOCKED, et la personne est prévenue dans sa langue.
//
// Le .inf est lu comme une donnée (format public documenté sur Microsoft
// Learn : sections, [Strings], AddService, identifiants de dossier) ; aucun
// fichier d'un éditeur n'est gardé dans le dépôt.

use std::collections::HashMap;
use std::sync::Arc;

use crate::dlls::vdev::VirtualDevice;

pub const ERROR_DRIVER_BLOCKED: u32 = 1275;

// ---- Lecture d'un .inf ----------------------------------------------------

/// Fichier .inf : sections dans l'ordre, lignes sans commentaires.
pub struct Inf {
    sections: Vec<(String, Vec<String>)>,
    strings: HashMap<String, String>,
}

/// Service décrit par une directive `AddService` et sa section.
#[derive(Debug, Clone, PartialEq)]
pub struct DriverService {
    pub name: String,
    pub display: String,
    pub binary: String,
    pub service_type: u32,
    pub start_type: u32,
}

/// Texte d'un .inf : UTF-16LE (avec BOM) ou 8 bits.
pub fn decode(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let w: Vec<u16> = (2..bytes.len().saturating_sub(1))
            .step_by(2)
            .map(|i| u16::from_le_bytes([bytes[i], bytes[i + 1]]))
            .collect();
        return String::from_utf16_lossy(&w);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

/// Retire le commentaire (`;` hors guillemets) d'une ligne.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Découpe `a, "b, c", d` en champs, guillemets retirés (`""` → `"`).
fn fields(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

impl Inf {
    pub fn parse(text: &str) -> Inf {
        let mut sections: Vec<(String, Vec<String>)> = Vec::new();
        let mut pending = String::new();
        for raw in text.lines() {
            let line = strip_comment(raw).trim_end();
            // Ligne continuée par une barre oblique inverse finale.
            if let Some(head) = line.strip_suffix('\\') {
                pending.push_str(head);
                continue;
            }
            pending.push_str(line);
            let line = std::mem::take(&mut pending);
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.split(']').next()) {
                sections.push((name.trim().to_string(), Vec::new()));
            } else if let Some((_, lines)) = sections.last_mut() {
                lines.push(line.to_string());
            }
        }
        let mut strings = HashMap::new();
        for (name, lines) in &sections {
            if !name.eq_ignore_ascii_case("Strings") {
                continue;
            }
            for l in lines {
                if let Some((k, v)) = l.split_once('=') {
                    let v = fields(v).into_iter().next().unwrap_or_default();
                    strings.insert(k.trim().to_ascii_lowercase(), v);
                }
            }
        }
        Inf { sections, strings }
    }

    fn section(&self, name: &str) -> Option<&[String]> {
        self.sections
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, l)| l.as_slice())
    }

    /// Remplace `%clé%` par [Strings] ou par un identifiant de dossier.
    pub fn expand(&self, s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find('%') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let Some(j) = after.find('%') else {
                out.push_str(&rest[i..]);
                return out;
            };
            let key = &after[..j];
            if key.is_empty() {
                out.push('%');
            } else if let Some(v) = self.strings.get(&key.to_ascii_lowercase()) {
                out.push_str(v);
            } else if let Some(dir) = dirid(key) {
                out.push_str(dir);
            } else {
                out.push('%');
                out.push_str(key);
                out.push('%');
            }
            rest = &after[j + 1..];
        }
        out.push_str(rest);
        out
    }

    /// `clé = valeurs` d'une section, valeurs développées.
    fn entries(&self, section: &str) -> Vec<(String, Vec<String>)> {
        self.section(section)
            .unwrap_or(&[])
            .iter()
            .filter_map(|l| {
                let (k, v) = l.split_once('=')?;
                Some((
                    k.trim().to_string(),
                    fields(v).iter().map(|f| self.expand(f)).collect(),
                ))
            })
            .collect()
    }

    /// Services déclarés par `AddService` (toutes sections confondues).
    pub fn services(&self) -> Vec<DriverService> {
        let mut out: Vec<DriverService> = Vec::new();
        for (section, _) in &self.sections {
            for (key, v) in self.entries(section) {
                if !key.eq_ignore_ascii_case("AddService") || v.len() < 3 || v[0].is_empty() {
                    continue;
                }
                let install = self.entries(&v[2]);
                let get = |k: &str| {
                    install
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(k))
                        .and_then(|(_, v)| v.first().cloned())
                };
                let num = |k: &str| get(k).and_then(|s| parse_number(&s));
                let Some(binary) = get("ServiceBinary") else {
                    continue;
                };
                if out.iter().any(|s| s.name.eq_ignore_ascii_case(&v[0])) {
                    continue;
                }
                out.push(DriverService {
                    name: v[0].clone(),
                    display: get("DisplayName").unwrap_or_else(|| v[0].clone()),
                    binary,
                    service_type: num("ServiceType").unwrap_or(1),
                    start_type: num("StartType").unwrap_or(3),
                });
            }
        }
        out
    }
}

fn parse_number(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Identifiants de dossier documentés des .inf (DIRID).
fn dirid(key: &str) -> Option<&'static str> {
    Some(match key.parse::<u32>().ok()? {
        10 => "C:\\Windows",
        11 => "C:\\Windows\\System32",
        12 => "C:\\Windows\\System32\\drivers",
        17 => "C:\\Windows\\INF",
        _ => return None,
    })
}

// ---- Traductions ----------------------------------------------------------

/// Pilote constructeur remplacé par un appareil virtuel de Winlux.
pub struct Translation {
    /// Nom du fichier .sys, en minuscules.
    pub binary: &'static str,
    /// Nom ouvert par le logiciel (`\\.\device`).
    pub device: &'static str,
    /// Produit nommé dans la notification.
    pub product: &'static str,
    pub make: fn() -> Arc<dyn VirtualDevice>,
}

/// Table des pilotes remplacés. Les marques arrivent avec #1534 (une issue
/// par marque, #1537 à #1540) ; l'essai n'existe qu'avec WINLUX_VDEV_ESSAI.
fn table() -> Vec<Translation> {
    let mut t = Vec::new();
    if std::env::var_os("WINLUX_VDEV_ESSAI").is_some() {
        t.push(Translation {
            binary: "winluxessai.sys",
            device: "WinluxEssaiPilote",
            product: "Winlux Essais",
            make: crate::dlls::vdev::essai,
        });
    }
    t
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Traduction du pilote dont le binaire est `image_path`.
pub fn translation(image_path: &str) -> Option<Translation> {
    let file = file_name(image_path.trim().trim_matches('"'));
    table()
        .into_iter()
        .find(|t| t.binary.eq_ignore_ascii_case(file))
}

/// Nom et module de l'appareil virtuel `name` si son pilote est inscrit et
/// démarré.
pub fn running_device(name: &str) -> Option<(&'static str, fn() -> Arc<dyn VirtualDevice>)> {
    table()
        .into_iter()
        .find(|t| t.device.eq_ignore_ascii_case(name))
        .filter(|t| crate::dlls::advapi32_services::driver_running(t.binary))
        .map(|t| (t.device, t.make))
}

// ---- Notification ---------------------------------------------------------

/// Texte de la notification, dans la langue de l'interface.
pub fn notice(lang: &str, replaced: Option<&str>, file: &str) -> String {
    let lang = lang.get(..2).unwrap_or("fr").to_ascii_lowercase();
    match (replaced, lang.as_str()) {
        (Some(p), "en") => format!("The Windows driver for {p} has been replaced by Winlux."),
        (Some(p), "de") => format!("Der Windows-Treiber für {p} wurde durch Winlux ersetzt."),
        (Some(p), "es") => format!("El controlador de Windows de {p} ha sido sustituido por Winlux."),
        (Some(p), _) => format!("Le pilote Windows de {p} a été remplacé par Winlux."),
        (None, "en") => format!(
            "The Windows driver {file} cannot be replaced yet: Winlux never loads Windows drivers. The program may not be able to reach its device."
        ),
        (None, "de") => format!(
            "Der Windows-Treiber {file} kann noch nicht ersetzt werden: Winlux lädt nie Windows-Treiber. Das Programm erreicht sein Gerät möglicherweise nicht."
        ),
        (None, "es") => format!(
            "El controlador de Windows {file} aún no puede sustituirse: Winlux nunca carga controladores de Windows. Es posible que el programa no llegue a su dispositivo."
        ),
        (None, _) => format!(
            "Le pilote Windows {file} ne peut pas encore être remplacé : Winlux ne charge jamais de pilote Windows. Le programme risque de ne pas joindre son appareil."
        ),
    }
}

/// Prévient la personne (une fois par pilote et par exécution).
pub fn tell(image_path: &str, replaced: Option<&str>) {
    static TOLD: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let file = file_name(image_path).to_ascii_lowercase();
    {
        let mut told = crate::dlls::lock(&TOLD);
        if told.contains(&file) {
            return;
        }
        told.push(file.clone());
    }
    let text = notice(&crate::permissions::interface_lang(), replaced, &file);
    crate::log_warn!("pilotes", "{text}");
    if std::env::var_os("DISPLAY").is_none() {
        return;
    }
    if let Ok(mut child) = std::process::Command::new("notify-send")
        .args(["-i", "drive-harddisk", "Winlux", &text])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

// ---- Magasin de pilotes ---------------------------------------------------

fn store_dir() -> std::path::PathBuf {
    crate::paths::to_linux("C:\\Windows\\INF")
}

/// Copie `src` dans le magasin : `oemN.inf` (le même contenu garde son nom).
/// Rend le nom choisi, ou une erreur Win32.
pub fn copy_to_store(src: &str, no_overwrite: bool) -> Result<String, u32> {
    let bytes = std::fs::read(crate::paths::to_linux(src)).map_err(|_| 2u32)?;
    let dir = store_dir();
    std::fs::create_dir_all(&dir).map_err(|_| 5u32)?;
    let mut used = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|_| 5u32)?.flatten() {
        let file = e.file_name().to_string_lossy().to_ascii_lowercase();
        let Some(n) = file
            .strip_prefix("oem")
            .and_then(|f| f.strip_suffix(".inf"))
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        used.push(n);
        if std::fs::read(e.path()).is_ok_and(|old| old == bytes) {
            if no_overwrite {
                return Err(80); // ERROR_FILE_EXISTS
            }
            return Ok(format!("oem{n}.inf"));
        }
    }
    let n = (0..).find(|n| !used.contains(n)).unwrap_or(0);
    let name = format!("oem{n}.inf");
    std::fs::write(dir.join(&name), &bytes).map_err(|_| 5u32)?;
    Ok(name)
}

/// Inscrit les services du .inf copié dans le magasin.
pub fn install_services(store_name: &str) -> Result<Vec<DriverService>, u32> {
    let bytes = std::fs::read(store_dir().join(store_name)).map_err(|_| 2u32)?;
    let inf = Inf::parse(&decode(&bytes));
    let services = inf.services();
    for s in &services {
        crate::dlls::advapi32_services::install_driver_service(s)?;
    }
    Ok(services)
}

/// Vrai si un nœud présent porte l'identifiant matériel `hwid`.
pub fn device_present(hwid: &str) -> bool {
    use crate::dlls::devnodes;
    devnodes::nodes().iter().any(|n| {
        [1u32, 2] // SPDRP_HARDWAREID, SPDRP_COMPATIBLEIDS
            .iter()
            .filter_map(|p| n.property(*p))
            .any(|(_, w)| {
                String::from_utf16_lossy(&w)
                    .split('\0')
                    .any(|id| !id.is_empty() && id.eq_ignore_ascii_case(hwid))
            })
            || n.id.eq_ignore_ascii_case(hwid)
    })
}

// ---- setupapi.dll et newdev.dll -----------------------------------------

const SP_COPY_NOOVERWRITE: u32 = 0x8;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_SUCH_DEVINST: u32 = 0xE000_020B;

unsafe fn opt_w(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| crate::dlls::wstr(p))
}
unsafe fn opt_a(p: *const u8) -> Option<String> {
    (!p.is_null()).then(|| crate::dlls::cstr(p))
}
fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}
fn store_path(name: &str) -> String {
    format!("C:\\Windows\\INF\\{name}")
}

/// Copie dans le magasin puis rend le chemin, en UTF-16 ou en 8 bits.
unsafe fn copy_oem_inf(
    src: Option<String>,
    style: u32,
    out: *mut u8,
    out_len: u32,
    required: *mut u32,
    component: *mut *mut u8,
    wide: bool,
) -> i32 {
    let Some(src) = src else {
        return fail(87);
    };
    let name = match copy_to_store(&src, style & SP_COPY_NOOVERWRITE != 0) {
        Ok(n) => n,
        Err(e) => return fail(e),
    };
    let path = store_path(&name);
    let units: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let len = if wide { units.len() } else { path.len() + 1 };
    if !required.is_null() {
        *required = len as u32;
    }
    if out.is_null() {
        crate::process::set_last_error(0);
        return 1;
    }
    if (out_len as usize) < len {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    let file_at = path.len() - name.len();
    if wide {
        std::ptr::copy_nonoverlapping(units.as_ptr(), out as *mut u16, units.len());
        if !component.is_null() {
            *component = out.add(file_at * 2);
        }
    } else {
        std::ptr::copy_nonoverlapping(path.as_ptr(), out, path.len());
        *out.add(path.len()) = 0;
        if !component.is_null() {
            *component = out.add(file_at);
        }
    }
    crate::process::set_last_error(0);
    1
}

/// `SetupCopyOEMInfW` : le .inf rejoint le magasin de pilotes de Winlux.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn SetupCopyOEMInfW(
    src: *const u16,
    _media: *const u16,
    _media_type: u32,
    style: u32,
    out: *mut u16,
    out_len: u32,
    required: *mut u32,
    component: *mut *mut u16,
) -> i32 {
    copy_oem_inf(
        opt_w(src),
        style,
        out as *mut u8,
        out_len,
        required,
        component as *mut *mut u8,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn SetupCopyOEMInfA(
    src: *const u8,
    _media: *const u8,
    _media_type: u32,
    style: u32,
    out: *mut u8,
    out_len: u32,
    required: *mut u32,
    component: *mut *mut u8,
) -> i32 {
    copy_oem_inf(opt_a(src), style, out, out_len, required, component, false)
}

/// `SetupUninstallOEMInfW` : retire `oemN.inf` du magasin.
pub unsafe extern "win64" fn SetupUninstallOEMInfW(
    name: *const u16,
    _flags: u32,
    _r: usize,
) -> i32 {
    let Some(name) = opt_w(name) else {
        return fail(87);
    };
    let file = file_name(&name);
    if !file.to_ascii_lowercase().starts_with("oem")
        || std::fs::remove_file(store_dir().join(file)).is_err()
    {
        return fail(2);
    }
    crate::process::set_last_error(0);
    1
}

/// Copie et inscription des services ; `start` démarre ceux « à la
/// demande », comme l'installation d'un appareil présent.
fn install(inf: Option<String>, start: bool) -> Result<(), u32> {
    let inf = inf.ok_or(87u32)?;
    let name = copy_to_store(&inf, false)?;
    for s in install_services(&name)? {
        if start && s.start_type == 3 {
            // Pilote sans traduction : refusé et signalé, l'installation
            // reste réussie comme sous Windows (l'appareil n'a pas démarré).
            let _ = crate::dlls::advapi32_services::start_driver_named(&s.name);
        }
    }
    Ok(())
}

unsafe fn no_reboot(p: *mut i32) {
    if !p.is_null() {
        *p = 0;
    }
}

/// `DiInstallDriverW` (newdev.dll) : le pilote est préinstallé.
pub unsafe extern "win64" fn DiInstallDriverW(
    _hwnd: usize,
    inf: *const u16,
    _flags: u32,
    reboot: *mut i32,
) -> i32 {
    no_reboot(reboot);
    match install(opt_w(inf), false) {
        Ok(()) => {
            crate::process::set_last_error(0);
            1
        }
        Err(e) => fail(e),
    }
}
pub unsafe extern "win64" fn DiInstallDriverA(
    _hwnd: usize,
    inf: *const u8,
    _flags: u32,
    reboot: *mut i32,
) -> i32 {
    no_reboot(reboot);
    match install(opt_a(inf), false) {
        Ok(()) => {
            crate::process::set_last_error(0);
            1
        }
        Err(e) => fail(e),
    }
}

/// `DiUninstallDriverW` : le .inf de même contenu quitte le magasin.
pub unsafe extern "win64" fn DiUninstallDriverW(
    _hwnd: usize,
    inf: *const u16,
    _flags: u32,
    reboot: *mut i32,
) -> i32 {
    no_reboot(reboot);
    let Some(inf) = opt_w(inf) else {
        return fail(87);
    };
    let Ok(bytes) = std::fs::read(crate::paths::to_linux(&inf)) else {
        return fail(2);
    };
    let found = std::fs::read_dir(store_dir())
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| std::fs::read(e.path()).is_ok_and(|b| b == bytes));
    match found {
        Some(e) if std::fs::remove_file(e.path()).is_ok() => {
            crate::process::set_last_error(0);
            1
        }
        _ => fail(2),
    }
}

fn update_driver(hwid: Option<String>, inf: Option<String>) -> i32 {
    let Some(hwid) = hwid.filter(|h| !h.is_empty()) else {
        return fail(87);
    };
    if !device_present(&hwid) {
        return fail(ERROR_NO_SUCH_DEVINST);
    }
    match install(inf, true) {
        Ok(()) => {
            crate::process::set_last_error(0);
            1
        }
        Err(e) => fail(e),
    }
}

/// `UpdateDriverForPlugAndPlayDevicesW` (newdev.dll) : appareil présent
/// requis (ERROR_NO_SUCH_DEVINST sinon), services inscrits et démarrés.
pub unsafe extern "win64" fn UpdateDriverForPlugAndPlayDevicesW(
    _hwnd: usize,
    hwid: *const u16,
    inf: *const u16,
    _flags: u32,
    reboot: *mut i32,
) -> i32 {
    no_reboot(reboot);
    update_driver(opt_w(hwid), opt_w(inf))
}
pub unsafe extern "win64" fn UpdateDriverForPlugAndPlayDevicesA(
    _hwnd: usize,
    hwid: *const u8,
    inf: *const u8,
    _flags: u32,
    reboot: *mut i32,
) -> i32 {
    no_reboot(reboot);
    update_driver(opt_a(hwid), opt_a(inf))
}

// ---- Appareils créés par un installeur (#1605) ---------------------------

const DICD_GENERATE_ID: u32 = 1;
const DIF_REMOVE: u32 = 5;
const DIF_REGISTERDEVICE: u32 = 0x19;
const ERROR_DEVINST_ALREADY_EXISTS: u32 = 0xE000_0207;
const ERROR_INVALID_HANDLE: u32 = 6;

unsafe fn guid_at(p: *const u8) -> [u8; 16] {
    let mut g = [0u8; 16];
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(p, g.as_mut_ptr(), 16);
    }
    g
}

/// `SetupDiCreateDeviceInfoList` : ensemble vide où créer un appareil.
pub unsafe extern "win64" fn SetupDiCreateDeviceInfoList(_guid: *const u8, _hwnd: usize) -> usize {
    crate::process::set_last_error(0);
    crate::dlls::devnodes::new_set()
}
pub unsafe extern "win64" fn SetupDiCreateDeviceInfoListExW(
    guid: *const u8,
    hwnd: usize,
    _machine: *const u16,
    _reserved: usize,
) -> usize {
    SetupDiCreateDeviceInfoList(guid, hwnd)
}

fn create_device_info(
    set: usize,
    name: Option<String>,
    guid: [u8; 16],
    desc: Option<String>,
    flags: u32,
    data: *mut u8,
) -> i32 {
    use crate::dlls::devnodes::{Kind, Node, Virtual};
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return fail(87);
    };
    let id = if flags & DICD_GENERATE_ID != 0 {
        if name.contains('\\') {
            return fail(87);
        }
        crate::dlls::devnodes::generate_root_id(&name)
    } else {
        if crate::dlls::devnodes::nodes()
            .iter()
            .any(|n| n.id.eq_ignore_ascii_case(&name))
        {
            return fail(ERROR_DEVINST_ALREADY_EXISTS);
        }
        name.to_ascii_uppercase()
    };
    let node = Node {
        id,
        parent: Some("HTREE\\ROOT\\0".into()),
        kind: Kind::Virtual(Virtual {
            class: guid,
            description: desc.unwrap_or_default(),
            ..Default::default()
        }),
    };
    if !unsafe { crate::dlls::devnodes::add_to_set(set, node, data) } {
        return fail(ERROR_INVALID_HANDLE);
    }
    crate::process::set_last_error(0);
    1
}

/// `SetupDiCreateDeviceInfoW` : appareil énuméré par ROOT, inscrit au
/// registre seulement par DIF_REGISTERDEVICE (comme Windows).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn SetupDiCreateDeviceInfoW(
    set: usize,
    name: *const u16,
    guid: *const u8,
    desc: *const u16,
    _hwnd: usize,
    flags: u32,
    data: *mut u8,
) -> i32 {
    create_device_info(set, opt_w(name), guid_at(guid), opt_w(desc), flags, data)
}
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn SetupDiCreateDeviceInfoA(
    set: usize,
    name: *const u8,
    guid: *const u8,
    desc: *const u8,
    _hwnd: usize,
    flags: u32,
    data: *mut u8,
) -> i32 {
    create_device_info(set, opt_a(name), guid_at(guid), opt_a(desc), flags, data)
}

/// Chaînes d'un tampon REG_MULTI_SZ ou REG_SZ (UTF-16 ou 8 bits).
unsafe fn strings_of(buf: *const u8, size: u32, wide: bool) -> Vec<String> {
    if buf.is_null() || size == 0 {
        return Vec::new();
    }
    let b = std::slice::from_raw_parts(buf, size as usize);
    let text = if wide {
        let w: Vec<u16> = (0..b.len() / 2)
            .map(|i| u16::from_le_bytes([b[2 * i], b[2 * i + 1]]))
            .collect();
        String::from_utf16_lossy(&w)
    } else {
        b.iter().map(|&c| c as char).collect()
    };
    text.split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

unsafe fn set_property(
    set: usize,
    data: *const u8,
    prop: u32,
    buf: *const u8,
    size: u32,
    wide: bool,
) -> i32 {
    use crate::dlls::devnodes::Kind;
    let values = strings_of(buf, size, wide);
    let done = crate::dlls::devnodes::with_set_node(set, data, |n| match &mut n.kind {
        Kind::Virtual(v) => {
            match prop {
                0 => v.description = values.first().cloned().unwrap_or_default(),
                1 => v.hardware_ids = values,
                2 => v.compatible_ids = values,
                // Les autres propriétés (nom convivial, filtres…) sont acceptées.
                _ => {}
            }
            true
        }
        // Les appareils réels gardent leurs propriétés lues dans sysfs.
        _ => true,
    });
    match done {
        Some(_) => {
            crate::process::set_last_error(0);
            1
        }
        None => fail(ERROR_INVALID_HANDLE),
    }
}

/// `SetupDiSetDeviceRegistryPropertyW` : identifiants matériels (SPDRP_HARDWAREID).
pub unsafe extern "win64" fn SetupDiSetDeviceRegistryPropertyW(
    set: usize,
    data: *const u8,
    prop: u32,
    buf: *const u8,
    size: u32,
) -> i32 {
    set_property(set, data, prop, buf, size, true)
}
pub unsafe extern "win64" fn SetupDiSetDeviceRegistryPropertyA(
    set: usize,
    data: *const u8,
    prop: u32,
    buf: *const u8,
    size: u32,
) -> i32 {
    set_property(set, data, prop, buf, size, false)
}

/// `SetupDiCallClassInstaller` : DIF_REGISTERDEVICE inscrit l'appareil
/// créé, DIF_REMOVE le retire ; les autres étapes réussissent sans rien
/// charger (aucun co-installeur ni pilote n'est exécuté).
pub unsafe extern "win64" fn SetupDiCallClassInstaller(
    dif: u32,
    set: usize,
    data: *const u8,
) -> i32 {
    use crate::dlls::devnodes;
    let node = devnodes::with_set_node(set, data, |n| n.clone());
    let result = match (dif, node) {
        (DIF_REGISTERDEVICE, Some(n)) => devnodes::register_virtual(&n),
        (DIF_REMOVE, Some(n)) => {
            devnodes::remove_virtual(&n.id);
            Ok(())
        }
        (DIF_REGISTERDEVICE | DIF_REMOVE, None) => Err(ERROR_INVALID_HANDLE),
        _ => Ok(()),
    };
    match result {
        Ok(()) => {
            crate::process::set_last_error(0);
            1
        }
        Err(e) => fail(e),
    }
}

/// `SetupDiRemoveDevice` : comme DIF_REMOVE.
pub unsafe extern "win64" fn SetupDiRemoveDevice(set: usize, data: *const u8) -> i32 {
    SetupDiCallClassInstaller(DIF_REMOVE, set, data)
}

/// `CMP_WaitNoPendingInstallEvents` : aucune installation n'est jamais en
/// attente sous Winlux (pas de pilote à charger), WAIT_OBJECT_0 tout de suite.
pub extern "win64" fn CMP_WaitNoPendingInstallEvents(_timeout: u32) -> u32 {
    0
}

/// `CM_Request_Device_EjectW` : un appareil créé par un installeur est
/// retiré ; un vrai appareil ne s'éjecte pas d'ici (CR_REMOVE_VETOED).
pub unsafe extern "win64" fn CM_Request_Device_EjectW(
    devinst: u32,
    veto: *mut u32,
    _name: *mut u16,
    _len: u32,
    _flags: u32,
) -> u32 {
    use crate::dlls::devnodes;
    let node = devnodes::nodes()
        .into_iter()
        .find(|n| devnodes::devinst(&n.id) == devinst);
    if !veto.is_null() {
        *veto = 0;
    }
    match node {
        Some(n) if matches!(n.kind, devnodes::Kind::Virtual(_)) => {
            devnodes::remove_virtual(&n.id);
            0
        }
        Some(_) => {
            if !veto.is_null() {
                *veto = 6; // PNP_VetoDevice
            }
            0x17 // CR_REMOVE_VETOED
        }
        None => 0x0D, // CR_NO_SUCH_DEVNODE
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetupDiCreateDeviceInfoList" => fn SetupDiCreateDeviceInfoList,
        "SetupDiCreateDeviceInfoListExW" => fn SetupDiCreateDeviceInfoListExW,
        "SetupDiCreateDeviceInfoW" => fn SetupDiCreateDeviceInfoW,
        "SetupDiCreateDeviceInfoA" => fn SetupDiCreateDeviceInfoA,
        "SetupDiSetDeviceRegistryPropertyW" => fn SetupDiSetDeviceRegistryPropertyW,
        "SetupDiSetDeviceRegistryPropertyA" => fn SetupDiSetDeviceRegistryPropertyA,
        "SetupDiCallClassInstaller" => fn SetupDiCallClassInstaller,
        "SetupDiRemoveDevice" => fn SetupDiRemoveDevice,
        "CMP_WaitNoPendingInstallEvents" => fn CMP_WaitNoPendingInstallEvents,
        "CM_Request_Device_EjectW" => fn CM_Request_Device_EjectW,
        "SetupCopyOEMInfW" => fn SetupCopyOEMInfW,
        "SetupCopyOEMInfA" => fn SetupCopyOEMInfA,
        "SetupUninstallOEMInfW" => fn SetupUninstallOEMInfW,
        "DiInstallDriverW" => fn DiInstallDriverW,
        "DiInstallDriverA" => fn DiInstallDriverA,
        "DiUninstallDriverW" => fn DiUninstallDriverW,
        "UpdateDriverForPlugAndPlayDevicesW" => fn UpdateDriverForPlugAndPlayDevicesW,
        "UpdateDriverForPlugAndPlayDevicesA" => fn UpdateDriverForPlugAndPlayDevicesA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INF: &str = "; Pilote d'essai\r\n\
[Version]\r\n\
Signature=\"$WINDOWS NT$\"\r\n\
Class=HIDClass\r\n\
\r\n\
[Essai.NTamd64.Services]\r\n\
AddService = WinluxEssai, 0x00000002, Essai_Service ; commentaire\r\n\
\r\n\
[Essai_Service]\r\n\
DisplayName    = %SvcDesc%\r\n\
ServiceType    = 1\r\n\
StartType      = 3\r\n\
ErrorControl   = 1\r\n\
ServiceBinary  = %12%\\winluxessai.sys\r\n\
\r\n\
[Strings]\r\n\
SvcDesc = \"Pilote d'essai ; Winlux\"\r\n";

    #[test]
    fn services_du_inf() {
        let inf = Inf::parse(INF);
        assert_eq!(
            inf.services(),
            vec![DriverService {
                name: "WinluxEssai".into(),
                display: "Pilote d'essai ; Winlux".into(),
                binary: "C:\\Windows\\System32\\drivers\\winluxessai.sys".into(),
                service_type: 1,
                start_type: 3,
            }]
        );
    }

    #[test]
    fn inf_en_utf16_et_continuation() {
        let text = "[S]\r\nAddService = A, 2, \\\r\n  Sec\r\n[Sec]\r\nServiceBinary=%10%\\a.sys\r\nServiceType=0x2\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let s = Inf::parse(&decode(&bytes)).services();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].binary, "C:\\Windows\\a.sys");
        assert_eq!(s[0].service_type, 2);
        assert_eq!(s[0].start_type, 3);
    }

    #[test]
    fn developpement_des_cles() {
        let inf = Inf::parse("[Strings]\nA=\"x\"\n");
        assert_eq!(inf.expand("%a%-%%-%99%-%inconnu%"), "x-%-%99%-%inconnu%");
    }

    #[test]
    fn notification_en_quatre_langues() {
        for l in ["fr_FR.UTF-8", "en", "de", "es"] {
            let a = notice(l, Some("Clavier"), "x.sys");
            let b = notice(l, None, "x.sys");
            assert!(a.contains("Clavier") && a.contains("Winlux"), "{a}");
            assert!(b.contains("x.sys") && b.contains("Winlux"), "{b}");
            assert_ne!(a, b);
        }
        assert!(notice("fr_FR", None, "x.sys").starts_with("Le pilote"));
    }

    #[test]
    fn nom_du_fichier_pilote() {
        assert_eq!(file_name("\\SystemRoot\\System32\\drivers\\A.SYS"), "A.SYS");
        assert_eq!(file_name("a.sys"), "a.sys");
    }
}
