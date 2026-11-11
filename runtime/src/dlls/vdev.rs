// Appareils virtuels (#1533, fille de #1531) : un logiciel constructeur
// ouvre son pilote par son nom (`CreateFileW("\\\\.\\NomDuPilote")`) puis lui
// parle avec `DeviceIoControl`. Winlux ne charge jamais de pilote `.sys` :
// le nom ouvre un appareil virtuel inscrit ici, et chaque code de contrôle
// est transmis à son module de traduction (table par marque, #1534), qui le
// rend en hidraw, hwmon ou PipeWire.
//
// Un code que le module ne connaît pas rend ERROR_INVALID_FUNCTION et une
// ligne de journal (WINLUX_TRACE) : nom, code décodé, tailles. Le contenu des
// tampons n'est jamais écrit par défaut ; WINLUX_VDEV_DUMP=1 l'ajoute en
// hexadécimal pour les passes d'observation (cadre d'interopérabilité de
// AGENTS.md : on journalise ce que le logiciel envoie à NOTRE appareil).
//
// L'appareil d'essai « WinluxEssai » n'existe que si WINLUX_VDEV_ESSAI est
// défini (scripts/test-vdev.sh).

use std::sync::{Arc, Mutex, OnceLock};

use crate::dlls::devio::Reply;

const ERROR_INVALID_FUNCTION: u32 = 1;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_MORE_DATA: u32 = 234;

/// Module de traduction d'un appareil virtuel.
pub trait VirtualDevice: Send + Sync {
    /// Réponse à un code de contrôle ; `None` pour un code inconnu.
    fn control(&self, code: u32, input: &[u8], out_len: usize) -> Option<Reply>;
    /// Réponse à un `WriteFile` ; `None` : écriture refusée
    /// (ERROR_INVALID_FUNCTION).
    fn write(&self, _data: &[u8]) -> Option<Reply> {
        None
    }
}

struct Entry {
    name: String,
    device: Arc<dyn VirtualDevice>,
}

fn devices() -> &'static Mutex<Vec<Entry>> {
    static DEVICES: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    DEVICES.get_or_init(|| {
        let mut v = Vec::new();
        if std::env::var_os("WINLUX_VDEV_ESSAI").is_some() {
            v.push(Entry {
                name: "WinluxEssai".into(),
                device: Arc::new(Essai::default()),
            });
        }
        Mutex::new(v)
    })
}

/// Inscrit (ou remplace) l'appareil virtuel `name`. Appelé par
/// l'installation virtualisée (#1532) et les tables par marque (#1534).
#[cfg_attr(not(test), allow(dead_code))]
pub fn register(name: &str, device: Arc<dyn VirtualDevice>) {
    let mut v = crate::dlls::lock(devices());
    v.retain(|e| !e.name.eq_ignore_ascii_case(name));
    v.push(Entry {
        name: name.to_string(),
        device,
    });
}

/// Retire l'appareil virtuel `name` ; les handles déjà ouverts restent valides.
#[cfg_attr(not(test), allow(dead_code))]
pub fn unregister(name: &str) -> bool {
    let mut v = crate::dlls::lock(devices());
    let before = v.len();
    v.retain(|e| !e.name.eq_ignore_ascii_case(name));
    v.len() != before
}

/// Nom d'appareil désigné par un chemin `\\.\Nom`, `\\.\Global\Nom`,
/// `\\?\Nom` ou `\\?\GLOBALROOT\Device\Nom`.
pub fn device_name(path: &str) -> Option<&str> {
    fn strip<'a>(s: &'a str, p: &str) -> Option<&'a str> {
        s.get(..p.len())
            .filter(|h| h.eq_ignore_ascii_case(p))
            .map(|_| &s[p.len()..])
    }
    let rest = strip(path, "\\\\.\\").or_else(|| strip(path, "\\\\?\\"))?;
    let rest = strip(rest, "GLOBALROOT\\Device\\")
        .or_else(|| strip(rest, "Global\\"))
        .unwrap_or(rest);
    (!rest.is_empty() && !rest.contains(['\\', '/', ':'])).then_some(rest)
}

// ---- Observation (#1655) ---------------------------------------------------

/// Interfaces à présenter pour une passe d'observation :
/// `WINLUX_VDEV_OBSERVE={guid}[,{guid}…]`. Un logiciel qui cherche l'une
/// d'elles (SetupDiGetClassDevs) la trouve sur un appareil d'observation
/// (devnodes.rs) ; ses DeviceIoControl et WriteFile y sont acceptés sans
/// effet et journalisés. Sert à voir, en boîte noire, ce qu'un logiciel
/// envoie à un pilote que Winlux ne charge pas, avant d'écrire sa table.
pub fn observed_interfaces() -> Vec<[u8; 16]> {
    let Some(list) = std::env::var_os("WINLUX_VDEV_OBSERVE") else {
        return Vec::new();
    };
    let mut out: Vec<[u8; 16]> = Vec::new();
    for g in list.to_string_lossy().split([',', ';', ' ']) {
        if let Some(g) = crate::dlls::devnodes::parse_guid(g.trim()) {
            if !out.contains(&g) {
                out.push(g);
            }
        }
    }
    out
}

/// Nom d'appareil (chemin d'interface sans `\\?\`) d'un appareil
/// d'observation.
fn observation_device(name: &str) -> Option<(String, Arc<dyn VirtualDevice>)> {
    let lower = name.to_ascii_lowercase();
    if !lower.starts_with("root#winlux_observation#") {
        return None;
    }
    let known = crate::dlls::devnodes::observation_nodes()
        .iter()
        .filter_map(|n| n.interface_path())
        .any(|p| p.get(4..).is_some_and(|t| t.eq_ignore_ascii_case(name)));
    known.then(|| {
        (
            name.to_string(),
            Arc::new(Observation) as Arc<dyn VirtualDevice>,
        )
    })
}

/// Appareil d'observation : tout est accepté, rien n'est exécuté.
struct Observation;

impl VirtualDevice for Observation {
    fn control(&self, _code: u32, _input: &[u8], _out_len: usize) -> Option<Reply> {
        Some(Reply::Data(Vec::new()))
    }
    fn write(&self, _data: &[u8]) -> Option<Reply> {
        Some(Reply::Data(Vec::new()))
    }
}

fn lookup(name: &str) -> Option<(String, Arc<dyn VirtualDevice>)> {
    if let Some(found) = observation_device(name) {
        return Some(found);
    }
    let found = crate::dlls::lock(devices())
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(name))
        .map(|e| (e.name.clone(), e.device.clone()));
    found.or_else(|| driver_device(name))
}

/// Appareil d'un pilote constructeur remplacé (#1532) : présent tant que son
/// service est inscrit et démarré, vérifié à chaque ouverture ; un seul
/// module par nom dans le processus, pour que son état soit partagé.
fn driver_device(name: &str) -> Option<(String, Arc<dyn VirtualDevice>)> {
    static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
    let (device, make) = crate::dlls::drivers::running_device(name)?;
    let mut cache = crate::dlls::lock(&CACHE);
    if let Some(e) = cache.iter().find(|e| e.name.eq_ignore_ascii_case(device)) {
        return Some((e.name.clone(), e.device.clone()));
    }
    let entry = Entry {
        name: device.to_string(),
        device: make(),
    };
    let out = (entry.name.clone(), entry.device.clone());
    cache.push(entry);
    Some(out)
}

/// Vrai si `path` désigne un appareil virtuel inscrit.
pub fn is_virtual(path: &str) -> bool {
    device_name(path).and_then(lookup).is_some()
}

// ---- Handles -------------------------------------------------------------

struct Opened {
    inode: u64,
    name: String,
    device: Arc<dyn VirtualDevice>,
}
static OPENED: Mutex<Vec<Opened>> = Mutex::new(Vec::new());

fn inode_of(fd: i32) -> Option<u64> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    (unsafe { libc::fstat(fd, &mut st) } == 0).then_some(st.st_ino as u64)
}

/// Ouvre l'appareil virtuel désigné par `path` : un memfd vide, propre au
/// handle, qui ne sert qu'à retrouver l'appareil dans DeviceIoControl.
pub fn open(path: &str) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::FromRawFd;
    let (name, device) = device_name(path)
        .and_then(lookup)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    let fd = unsafe { libc::memfd_create(c"winlux-vdev".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let inode = inode_of(fd).unwrap_or(0);
    let mut opened = crate::dlls::lock(&OPENED);
    // Un inode libéré peut être réutilisé : l'ancienne entrée est morte.
    opened.retain(|o| o.inode != inode);
    opened.push(Opened {
        inode,
        name,
        device,
    });
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

fn device_of_handle(h: usize) -> Option<(String, Arc<dyn VirtualDevice>)> {
    // Aucun appareil virtuel ouvert : rien à chercher (WriteFile passe ici).
    if crate::dlls::lock(&OPENED).is_empty() {
        return None;
    }
    let fd = crate::dlls::kernel32::fd_of_handle(h)?;
    let mut name = [0u8; 32];
    let link = format!("/proc/self/fd/{fd}\0");
    let n = unsafe {
        libc::readlink(
            link.as_ptr() as *const libc::c_char,
            name.as_mut_ptr() as *mut libc::c_char,
            name.len(),
        )
    };
    // Seuls nos memfd « winlux-vdev » comptent, pas un fichier de même inode.
    if n <= 0 || !name[..n as usize].starts_with(b"/memfd:winlux-vdev") {
        return None;
    }
    let inode = inode_of(fd)?;
    crate::dlls::lock(&OPENED)
        .iter()
        .find(|o| o.inode == inode)
        .map(|o| (o.name.clone(), o.device.clone()))
}

// ---- DeviceIoControl -----------------------------------------------------

/// Code de contrôle décodé (CTL_CODE) pour le journal.
pub fn describe(code: u32) -> String {
    let method = ["BUFFERED", "IN_DIRECT", "OUT_DIRECT", "NEITHER"][(code & 3) as usize];
    let access = ["ANY", "READ", "WRITE", "READ|WRITE"][((code >> 14) & 3) as usize];
    format!(
        "0x{code:08X} (type 0x{:04X}, fonction 0x{:03X}, METHOD_{method}, FILE_{access}_ACCESS)",
        code >> 16,
        (code >> 2) & 0xFFF
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn journal(name: &str, code: u32, input: &[u8], out_len: usize, outcome: &str) {
    if !crate::log::enabled(crate::log::Level::Trace) {
        return;
    }
    let mut line = format!(
        "{name} : DeviceIoControl {} entrée {} o, sortie {out_len} o : {outcome}",
        describe(code),
        input.len()
    );
    if std::env::var_os("WINLUX_VDEV_DUMP").is_some_and(|v| v == "1") {
        line.push_str(&format!(" ; entrée = {}", hex(input)));
    }
    crate::log::emit(crate::log::Level::Trace, "vdev", &format_args!("{line}"));
}

/// `DeviceIoControl` sur un handle d'appareil virtuel ; `None` si `h` n'en
/// est pas un. Réponse synchrone : avec OVERLAPPED, l'opération est déjà
/// finie au retour (TRUE), l'événement est signalé et le port de complétion
/// reçoit son paquet, comme un pilote qui complète sans attendre.
///
/// # Safety
/// Pointeurs fournis par le programme, comme pour DeviceIoControl.
#[allow(clippy::too_many_arguments)]
pub unsafe fn device_io_control(
    h: usize,
    code: u32,
    inb: *const u8,
    inl: u32,
    outb: *mut u8,
    outl: u32,
    returned: *mut u32,
    ov: *mut std::ffi::c_void,
) -> Option<i32> {
    let (name, device) = device_of_handle(h)?;
    let input = if inb.is_null() || inl == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(inb, inl as usize)
    };
    let out_len = if outb.is_null() { 0 } else { outl as usize };
    let fail = |e: u32| {
        crate::process::set_last_error(e);
        Some(0)
    };
    let (data, error) = match device.control(code, input, out_len) {
        None => {
            journal(&name, code, input, out_len, "code inconnu");
            return fail(ERROR_INVALID_FUNCTION);
        }
        Some(Reply::Error(e)) => {
            journal(&name, code, input, out_len, &format!("erreur {e}"));
            return fail(e);
        }
        Some(Reply::Data(d)) => (d, 0),
        Some(Reply::Header(d)) => (d, ERROR_MORE_DATA),
    };
    if data.len() > out_len {
        journal(&name, code, input, out_len, "tampon de sortie trop petit");
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    journal(
        &name,
        code,
        input,
        out_len,
        &format!("{} o rendus", data.len()),
    );
    if !data.is_empty() {
        std::ptr::copy_nonoverlapping(data.as_ptr(), outb, data.len());
    }
    let n = data.len() as u32;
    if !returned.is_null() {
        *returned = n;
    }
    if !ov.is_null() {
        crate::dlls::kernel32::complete_overlapped(h, ov as usize, n, error);
    }
    crate::process::set_last_error(error);
    Some((error == 0) as i32)
}

/// `WriteFile` sur un handle d'appareil virtuel ; `None` si `h` n'en est
/// pas un. Journalisé comme DeviceIoControl (contenu avec
/// WINLUX_VDEV_DUMP=1) ; réponse synchrone, OVERLAPPED compris.
///
/// # Safety
/// Pointeurs fournis par le programme, comme pour WriteFile.
pub unsafe fn write_file(
    h: usize,
    buf: *const u8,
    len: u32,
    written: *mut u32,
    ov: usize,
) -> Option<i32> {
    let (name, device) = device_of_handle(h)?;
    let data = if buf.is_null() || len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(buf, len as usize)
    };
    let error = match device.write(data) {
        Some(Reply::Error(e)) => e,
        Some(_) => 0,
        None => ERROR_INVALID_FUNCTION,
    };
    if crate::log::enabled(crate::log::Level::Trace) {
        let mut line = format!(
            "{name} : WriteFile {} o : {}",
            data.len(),
            if error == 0 {
                "accepté".to_string()
            } else {
                format!("erreur {error}")
            }
        );
        if std::env::var_os("WINLUX_VDEV_DUMP").is_some_and(|v| v == "1") {
            line.push_str(&format!(" ; données = {}", hex(data)));
        }
        crate::log::emit(crate::log::Level::Trace, "vdev", &format_args!("{line}"));
    }
    let n = if error == 0 { len } else { 0 };
    if !written.is_null() {
        *written = n;
    }
    if ov != 0 {
        crate::dlls::kernel32::complete_overlapped(h, ov, n, error);
    }
    crate::process::set_last_error(error);
    Some((error == 0) as i32)
}

// ---- Appareil d'essai ----------------------------------------------------

/// CTL_CODE(0x8000, fonction, METHOD_BUFFERED, FILE_ANY_ACCESS).
pub const fn essai_code(function: u32) -> u32 {
    (0x8000 << 16) | (function << 2)
}

/// Module de l'appareil d'essai (aussi servi par le pilote d'essai de
/// drivers.rs).
pub fn essai() -> Arc<dyn VirtualDevice> {
    Arc::new(Essai::default())
}

/// Appareil d'essai : version, écho inversé, compteur d'appels, réponse
/// partielle (ERROR_MORE_DATA) et code refusé.
#[derive(Default)]
struct Essai {
    calls: Mutex<u32>,
}

impl VirtualDevice for Essai {
    fn control(&self, code: u32, input: &[u8], out_len: usize) -> Option<Reply> {
        let calls = {
            let mut c = crate::dlls::lock(&self.calls);
            *c += 1;
            *c
        };
        Some(match code {
            c if c == essai_code(0x800) => {
                let mut v = 0x0001_0000u32.to_le_bytes().to_vec();
                v.extend_from_slice(&(input.len() as u32).to_le_bytes());
                Reply::Data(v)
            }
            c if c == essai_code(0x801) => Reply::Data(input.iter().rev().copied().collect()),
            c if c == essai_code(0x802) => Reply::Data(calls.to_le_bytes().to_vec()),
            c if c == essai_code(0x803) => {
                // Taille totale de 64 octets, seul l'en-tête tient.
                let v = 64u32.to_le_bytes().to_vec();
                if out_len >= 64 {
                    Reply::Data([v, vec![0; 60]].concat())
                } else {
                    Reply::Header(v)
                }
            }
            c if c == essai_code(0x804) => Reply::Error(5),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appareil_d_observation() {
        let _g = crate::mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("WINLUX_VDEV_OBSERVE");
        std::env::remove_var("WINLUX_VDEV_OBSERVE");
        assert!(observed_interfaces().is_empty());
        assert!(crate::dlls::devnodes::observation_nodes().is_empty());
        std::env::set_var(
            "WINLUX_VDEV_OBSERVE",
            "{2abad0de-4ddc-4965-8d7a-7faf3b7f1d59}, pas-un-guid",
        );
        assert_eq!(observed_interfaces().len(), 1);
        let nodes = crate::dlls::devnodes::observation_nodes();
        let path = nodes[0].interface_path().unwrap();
        assert_eq!(
            path,
            "\\\\?\\root#winlux_observation#0000#{2abad0de-4ddc-4965-8d7a-7faf3b7f1d59}"
        );
        assert!(is_virtual(&path));
        assert!(is_virtual(&path.to_ascii_uppercase()));
        assert!(!is_virtual(
            "\\\\?\\root#winlux_observation#0001#{2abad0de-4ddc-4965-8d7a-7faf3b7f1d59}"
        ));
        let (_, dev) = lookup(&path[4..]).unwrap();
        assert!(matches!(dev.write(b"abc"), Some(Reply::Data(_))));
        assert!(matches!(
            dev.control(0x0022_0000, b"", 0),
            Some(Reply::Data(_))
        ));
        assert!(open(&path).is_ok());
        match old {
            Some(v) => std::env::set_var("WINLUX_VDEV_OBSERVE", v),
            None => std::env::remove_var("WINLUX_VDEV_OBSERVE"),
        }
    }

    #[test]
    fn noms_d_appareil() {
        assert_eq!(device_name("\\\\.\\Pilote"), Some("Pilote"));
        assert_eq!(device_name("\\\\.\\Global\\Pilote"), Some("Pilote"));
        assert_eq!(
            device_name("\\\\?\\GLOBALROOT\\Device\\Pilote"),
            Some("Pilote")
        );
        assert_eq!(
            device_name("\\\\?\\globalroot\\device\\Pilote"),
            Some("Pilote")
        );
        assert_eq!(device_name("\\\\.\\C:"), None);
        assert_eq!(device_name("\\\\?\\C:\\dossier"), None);
        assert_eq!(device_name("C:\\Pilote"), None);
        assert_eq!(device_name("\\\\.\\"), None);
    }

    #[test]
    fn code_decode() {
        let d = describe(essai_code(0x801));
        assert!(
            d.starts_with("0x80002004 (type 0x8000, fonction 0x801"),
            "{d}"
        );
        assert!(
            d.contains("METHOD_BUFFERED") && d.contains("FILE_ANY_ACCESS"),
            "{d}"
        );
        assert!(describe(0x0022_E00F).contains("METHOD_NEITHER"));
        assert!(describe(0x0022_C000).contains("FILE_READ|WRITE_ACCESS"));
    }

    struct Fixe;
    impl VirtualDevice for Fixe {
        fn control(&self, code: u32, _: &[u8], _: usize) -> Option<Reply> {
            (code == 7).then(|| Reply::Data(vec![1, 2, 3]))
        }
    }

    #[test]
    fn inscription_et_ouverture() {
        register("WinluxTestUnitaire", Arc::new(Fixe));
        assert!(is_virtual("\\\\.\\winluxtestunitaire"));
        assert!(open("\\\\.\\Global\\WinluxTestUnitaire").is_ok());
        assert!(unregister("WINLUXTESTUNITAIRE"));
        assert!(!is_virtual("\\\\.\\WinluxTestUnitaire"));
        assert!(open("\\\\.\\WinluxTestUnitaire").is_err());
    }
}
