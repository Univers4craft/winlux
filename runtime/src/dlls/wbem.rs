//! WMI en lecture : CLSID_WbemLocator (IWbemLocator, IWbemServices,
//! IEnumWbemClassObject, IWbemClassObject), #1198.
//!
//! Les installeurs, les outils de licence ou d'inventaire (Photoshop)
//! demandent à WMI la version du système, le processeur, la carte mère, les
//! disques ou la carte graphique. Winlux sert l'espace de noms ROOT\CIMV2 en
//! lecture seule, avec un WQL minimal (SELECT … FROM … WHERE, comparaisons,
//! LIKE, IS NULL, AND/OR) et les classes les plus demandées, alimentées par
//! /proc, /sys et les valeurs que kernel32, iphlpapi et DXGI servent déjà.
//!
//! Méthodes, ordre des tables, codes WBEM_* et types CIM : wbemcli.h de
//! MinGW-w64 et documentation Microsoft Learn des classes Win32_*.

use super::comobj::{guid, Guid};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const WBEM_S_NO_MORE_DATA: i32 = 0x40005;
const WBEM_E_NOT_FOUND: i32 = 0x8004_1002u32 as i32;
const WBEM_E_INVALID_PARAMETER: i32 = 0x8004_1008u32 as i32;
const WBEM_E_NOT_SUPPORTED: i32 = 0x8004_100Cu32 as i32;
const WBEM_E_INVALID_NAMESPACE: i32 = 0x8004_100Eu32 as i32;
const WBEM_E_INVALID_CLASS: i32 = 0x8004_1010u32 as i32;
const WBEM_E_INVALID_QUERY: i32 = 0x8004_1017u32 as i32;
const WBEM_E_INVALID_QUERY_TYPE: i32 = 0x8004_1018u32 as i32;
const WBEM_E_INVALID_OBJECT_PATH: i32 = 0x8004_103Au32 as i32;
const WBEM_E_ACCESS_DENIED: i32 = 0x8004_1003u32 as i32;

const WBEM_FLAG_SYSTEM_ONLY: i32 = 0x30;
const WBEM_FLAG_NONSYSTEM_ONLY: i32 = 0x40;
const WBEM_FLAG_KEYS_ONLY: i32 = 0x4;

pub const CIM_SINT32: i32 = 3;
pub const CIM_STRING: i32 = 8;
pub const CIM_BOOLEAN: i32 = 11;
pub const CIM_UINT16: i32 = 18;
pub const CIM_UINT32: i32 = 19;
pub const CIM_UINT64: i32 = 21;
pub const CIM_DATETIME: i32 = 101;
pub const CIM_FLAG_ARRAY: i32 = 0x2000;

const VT_NULL: u16 = 1;
const VT_I4: u16 = 3;
const VT_BSTR: u16 = 8;
const VT_BOOL: u16 = 11;
const VT_UI1: u16 = 17;
const VT_ARRAY: u16 = 0x2000;

pub const CLSID_WBEM_LOCATOR: Guid = guid(
    0x4590_F811,
    0x1D3A,
    0x11D0,
    [0x89, 0x1F, 0x00, 0xAA, 0x00, 0x4B, 0x2E, 0x24],
);
pub const IID_IWBEM_LOCATOR: Guid = guid(
    0xDC12_A687,
    0x737F,
    0x11CF,
    [0x88, 0x4D, 0x00, 0xAA, 0x00, 0x4B, 0x2E, 0x24],
);
pub const IID_IWBEM_CLASS_OBJECT: Guid = guid(
    0xDC12_A681,
    0x737F,
    0x11CF,
    [0x88, 0x4D, 0x00, 0xAA, 0x00, 0x4B, 0x2E, 0x24],
);
pub const IID_IWBEM_SERVICES: Guid = guid(
    0x9556_DC99,
    0x828C,
    0x11CF,
    [0xA3, 0x7E, 0x00, 0xAA, 0x00, 0x32, 0x40, 0xC7],
);
pub const IID_IENUM_WBEM_CLASS_OBJECT: Guid = guid(
    0x0279_47E1,
    0xD731,
    0x11CE,
    [0xA3, 0x57, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

// ── Valeurs et objets ──────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Null,
    Str(String),
    I32(i32),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Bool(bool),
    Date(String),
    Strs(Vec<String>),
}

impl Val {
    fn text(&self) -> Option<String> {
        Some(match self {
            Val::Null | Val::Strs(_) => return None,
            Val::Str(s) | Val::Date(s) => s.clone(),
            Val::I32(v) => v.to_string(),
            Val::U8(v) => v.to_string(),
            Val::U16(v) => v.to_string(),
            Val::U32(v) => v.to_string(),
            Val::U64(v) => v.to_string(),
            Val::Bool(v) => if *v { "TRUE" } else { "FALSE" }.into(),
        })
    }
    fn number(&self) -> Option<f64> {
        match self {
            Val::I32(v) => Some(*v as f64),
            Val::U8(v) => Some(*v as f64),
            Val::U16(v) => Some(*v as f64),
            Val::U32(v) => Some(*v as f64),
            Val::U64(v) => Some(*v as f64),
            Val::Bool(v) => Some(*v as u8 as f64),
            _ => None,
        }
    }
}

/// Une propriété : nom, type CIM, valeur.
pub type Prop = (String, i32, Val);

#[derive(Clone, Debug)]
pub struct Object {
    pub class: String,
    /// Faux pour une définition de classe (GetObject sur le seul nom).
    pub instance: bool,
    pub props: Vec<Prop>,
}

impl Object {
    fn get(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.0.eq_ignore_ascii_case(name))
    }

    fn key(&self) -> &'static str {
        class_key(&self.class)
    }

    fn relpath(&self) -> Option<String> {
        if !self.instance {
            return Some(self.class.clone());
        }
        let (_, _, v) = self.get(self.key())?;
        Some(match v {
            Val::Str(s) => format!(
                "{}.{}=\"{}\"",
                self.class,
                self.key(),
                s.replace('\\', "\\\\")
            ),
            other => format!("{}.{}={}", self.class, self.key(), other.text()?),
        })
    }

    /// Propriétés système (__CLASS, __PATH…).
    fn system(&self) -> Vec<Prop> {
        let server = super::kernel32::get_hostname_string().to_ascii_uppercase();
        let rel = self.relpath().map_or(Val::Null, Val::Str);
        let path = match &rel {
            Val::Str(r) => Val::Str(format!("\\\\{server}\\root\\cimv2:{r}")),
            _ => Val::Null,
        };
        vec![
            (
                "__GENUS".into(),
                CIM_SINT32,
                Val::I32(if self.instance { 2 } else { 1 }),
            ),
            ("__CLASS".into(), CIM_STRING, Val::Str(self.class.clone())),
            ("__SUPERCLASS".into(), CIM_STRING, Val::Null),
            ("__DYNASTY".into(), CIM_STRING, Val::Str(self.class.clone())),
            ("__RELPATH".into(), CIM_STRING, rel),
            (
                "__PROPERTY_COUNT".into(),
                CIM_SINT32,
                Val::I32(self.props.len() as i32),
            ),
            (
                "__DERIVATION".into(),
                CIM_STRING | CIM_FLAG_ARRAY,
                Val::Strs(Vec::new()),
            ),
            ("__SERVER".into(), CIM_STRING, Val::Str(server)),
            (
                "__NAMESPACE".into(),
                CIM_STRING,
                Val::Str("root\\cimv2".into()),
            ),
            ("__PATH".into(), CIM_STRING, path),
        ]
    }

    /// Noms rendus par GetNames / BeginEnumeration selon les drapeaux.
    fn listed(&self, flags: i32) -> Vec<Prop> {
        let mut out = Vec::new();
        let scope = flags & 0x70;
        if scope != WBEM_FLAG_NONSYSTEM_ONLY {
            out.extend(self.system());
        }
        if scope != WBEM_FLAG_SYSTEM_ONLY {
            let key = self.key();
            out.extend(
                self.props
                    .iter()
                    .filter(|p| flags & WBEM_FLAG_KEYS_ONLY == 0 || p.0.eq_ignore_ascii_case(key))
                    .cloned(),
            );
        }
        out
    }
}

// ── Classes ────────────────────────────────────────────────────────

pub const CLASSES: [&str; 10] = [
    "Win32_OperatingSystem",
    "Win32_ComputerSystem",
    "Win32_Processor",
    "Win32_BaseBoard",
    "Win32_BIOS",
    "Win32_VideoController",
    "Win32_LogicalDisk",
    "Win32_DiskDrive",
    "Win32_NetworkAdapterConfiguration",
    "Win32_Process",
];

fn class_key(class: &str) -> &'static str {
    match class.to_ascii_lowercase().as_str() {
        "win32_processor" | "win32_videocontroller" | "win32_logicaldisk" | "win32_diskdrive" => {
            "DeviceID"
        }
        "win32_baseboard" => "Tag",
        "win32_networkadapterconfiguration" => "Index",
        "win32_process" => "Handle",
        _ => "Name",
    }
}

fn canonical_class(name: &str) -> Option<&'static str> {
    CLASSES
        .iter()
        .copied()
        .find(|c| c.eq_ignore_ascii_case(name))
}

/// Instances d'une classe, lues au moment de la requête.
pub fn instances(class: &str) -> Option<Vec<Object>> {
    let class = canonical_class(class)?;
    let rows: Vec<Vec<Prop>> = match class {
        "Win32_OperatingSystem" => vec![data::operating_system()],
        "Win32_ComputerSystem" => vec![data::computer_system()],
        "Win32_Processor" => data::processors(),
        "Win32_BaseBoard" => vec![data::base_board()],
        "Win32_BIOS" => vec![data::bios()],
        "Win32_VideoController" => data::video_controllers(),
        "Win32_LogicalDisk" => data::logical_disks(),
        "Win32_DiskDrive" => data::disk_drives(),
        "Win32_NetworkAdapterConfiguration" => data::network_configs(),
        _ => data::processes(),
    };
    Some(
        rows.into_iter()
            .map(|props| Object {
                class: class.to_string(),
                instance: true,
                props,
            })
            .collect(),
    )
}

/// Définition de classe : mêmes propriétés, valeurs nulles.
fn class_object(class: &str) -> Option<Object> {
    let class = canonical_class(class)?;
    let sample = instances(class)?.into_iter().next();
    let props = sample
        .map(|o| o.props)
        .unwrap_or_default()
        .into_iter()
        .map(|(n, t, _)| (n, t, Val::Null))
        .collect();
    Some(Object {
        class: class.to_string(),
        instance: false,
        props,
    })
}

mod data {
    use super::*;

    fn s(name: &str, v: impl Into<String>) -> Prop {
        (name.into(), CIM_STRING, Val::Str(v.into()))
    }
    fn opt(name: &str, v: Option<String>) -> Prop {
        (name.into(), CIM_STRING, v.map_or(Val::Null, Val::Str))
    }
    fn u16p(name: &str, v: u16) -> Prop {
        (name.into(), CIM_UINT16, Val::U16(v))
    }
    fn u32p(name: &str, v: u32) -> Prop {
        (name.into(), CIM_UINT32, Val::U32(v))
    }
    fn u64p(name: &str, v: u64) -> Prop {
        (name.into(), CIM_UINT64, Val::U64(v))
    }
    fn b(name: &str, v: bool) -> Prop {
        (name.into(), CIM_BOOLEAN, Val::Bool(v))
    }
    fn date(name: &str, v: Option<String>) -> Prop {
        (name.into(), CIM_DATETIME, v.map_or(Val::Null, Val::Date))
    }
    fn strs(name: &str, v: Vec<String>) -> Prop {
        let val = if v.is_empty() {
            Val::Null
        } else {
            Val::Strs(v)
        };
        (name.into(), CIM_STRING | CIM_FLAG_ARRAY, val)
    }

    fn read(path: &str) -> Option<String> {
        let t = std::fs::read_to_string(path).ok()?;
        let t = t.trim().trim_end_matches('\0').trim().to_string();
        (!t.is_empty()).then_some(t)
    }

    fn dmi(field: &str) -> Option<String> {
        read(&format!("/sys/class/dmi/id/{field}"))
    }

    /// Date CIM : yyyymmddHHMMSS.mmmmmm+UUU (décalage en minutes).
    pub fn cim_date(secs: i64) -> String {
        // SAFETY : `tm` est rempli par localtime_r.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let t = secs as libc::time_t;
        unsafe { libc::localtime_r(&t, &mut tm) };
        let off = tm.tm_gmtoff / 60;
        format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}.000000{}{:03}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec,
            if off < 0 { '-' } else { '+' },
            off.abs()
        )
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    }

    fn boot_time() -> Option<i64> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        stat.lines()
            .find_map(|l| l.strip_prefix("btime "))
            .and_then(|v| v.trim().parse().ok())
    }

    fn meminfo() -> (u64, u64) {
        let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let field = |name: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(name))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
                .unwrap_or(0)
        };
        (field("MemTotal:"), field("MemAvailable:"))
    }

    fn computer_name() -> String {
        super::super::kernel32::get_hostname_string().to_ascii_uppercase()
    }

    fn product_name() -> String {
        crate::registry::get_string(
            "HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows NT\\CurrentVersion",
            "ProductName",
        )
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "Winlux".into())
    }

    fn build() -> u32 {
        (super::super::kernel32::get_version() >> 16) & 0xFFFF
    }

    pub fn operating_system() -> Vec<Prop> {
        let (total, avail) = meminfo();
        let lcid = crate::intl::api::get_user_default_lcid();
        let lang = crate::intl::locale::ui_language_tag().unwrap_or_else(|| "en-US".into());
        let caption = product_name();
        let processes = std::fs::read_dir("/proc")
            .map(|d| {
                d.flatten()
                    .filter(|e| e.file_name().to_string_lossy().parse::<u32>().is_ok())
                    .count()
            })
            .unwrap_or(0);
        let boot = boot_time();
        vec![
            s("BootDevice", "\\Device\\HarddiskVolume1"),
            s("BuildNumber", build().to_string()),
            s("BuildType", "Multiprocessor Free"),
            s("Caption", caption.clone()),
            s("CodeSet", crate::intl::api::get_acp().to_string()),
            s("CountryCode", "33"),
            s("CSName", computer_name()),
            (
                "CurrentTimeZone".into(),
                CIM_SINT32,
                Val::I32({
                    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
                    let t = now() as libc::time_t;
                    unsafe { libc::localtime_r(&t, &mut tm) };
                    (tm.tm_gmtoff / 60) as i32
                }),
            ),
            date("InstallDate", boot.map(cim_date)),
            date("LastBootUpTime", boot.map(cim_date)),
            date("LocalDateTime", Some(cim_date(now()))),
            s("Locale", format!("{:04x}", lcid & 0xFFFF)),
            s("Manufacturer", "Winlux"),
            strs("MUILanguages", vec![lang]),
            s(
                "Name",
                format!("{caption}|C:\\Windows|\\Device\\Harddisk0\\Partition1"),
            ),
            u32p("NumberOfProcesses", processes as u32),
            u32p("NumberOfUsers", 1),
            s("OSArchitecture", "64-bit"),
            u32p("OSLanguage", lcid & 0xFFFF),
            u16p("OSType", 18),
            b("Primary", true),
            u32p("ProductType", 1),
            s("RegisteredUser", crate::paths::user_name()),
            s("SerialNumber", "00000-00000-00000-AAAAA"),
            u16p("ServicePackMajorVersion", 0),
            u16p("ServicePackMinorVersion", 0),
            s("Status", "OK"),
            s("SystemDirectory", "C:\\Windows\\system32"),
            s("SystemDrive", "C:"),
            u64p("FreePhysicalMemory", avail),
            u64p("TotalVisibleMemorySize", total),
            u64p("FreeVirtualMemory", avail),
            u64p("TotalVirtualMemorySize", total),
            s("Version", format!("10.0.{}", build())),
            s("WindowsDirectory", "C:\\Windows"),
        ]
    }

    struct Cpu {
        name: String,
        vendor: String,
        family: u32,
        model: u32,
        stepping: u32,
        mhz: u32,
        logical: u32,
        cores: u32,
        sockets: u32,
        hypervisor: bool,
        virt: bool,
    }

    fn cpu() -> Cpu {
        let text = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let field = |name: &str| {
            text.lines()
                .find(|l| l.split(':').next().is_some_and(|k| k.trim() == name))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
        };
        let num = |name: &str| field(name).and_then(|v| v.parse::<f64>().ok());
        let logical = text
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count()
            .max(1) as u32;
        let mut sockets: Vec<String> = text
            .lines()
            .filter(|l| l.starts_with("physical id"))
            .filter_map(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
            .collect();
        sockets.sort();
        sockets.dedup();
        let sockets = (sockets.len() as u32).max(1);
        let flags = field("flags").unwrap_or_default();
        let has = |f: &str| flags.split_whitespace().any(|x| x == f);
        let max_khz = read("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
            .and_then(|v| v.parse::<u32>().ok());
        Cpu {
            name: field("model name").unwrap_or_else(|| "Processor".into()),
            vendor: field("vendor_id").unwrap_or_default(),
            family: num("cpu family").unwrap_or(0.0) as u32,
            model: num("model").unwrap_or(0.0) as u32,
            stepping: num("stepping").unwrap_or(0.0) as u32,
            mhz: max_khz
                .map(|k| k / 1000)
                .or_else(|| num("cpu MHz").map(|m| m as u32))
                .unwrap_or(0),
            logical,
            cores: num("cpu cores").map_or(logical, |c| c as u32 * sockets),
            sockets,
            hypervisor: has("hypervisor"),
            virt: has("vmx") || has("svm"),
        }
    }

    fn cache_kb(level: &str) -> Option<u32> {
        for i in 0..8 {
            let dir = format!("/sys/devices/system/cpu/cpu0/cache/index{i}");
            if read(&format!("{dir}/level")).as_deref() != Some(level) {
                continue;
            }
            if read(&format!("{dir}/type")).as_deref() == Some("Instruction") {
                continue;
            }
            let size = read(&format!("{dir}/size"))?;
            return size.trim_end_matches('K').parse().ok();
        }
        None
    }

    fn processor_id() -> String {
        #[cfg(target_arch = "x86_64")]
        {
            // SAFETY : CPUID existe sur tout processeur x86_64.
            #[allow(unused_unsafe)]
            let r = unsafe { std::arch::x86_64::__cpuid(1) };
            format!("{:08X}{:08X}", r.edx, r.eax)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            String::new()
        }
    }

    pub fn processors() -> Vec<Vec<Prop>> {
        let c = cpu();
        let arch = if c.vendor == "AuthenticAMD" {
            "AMD64"
        } else {
            "Intel64"
        };
        let caption = format!(
            "{arch} Family {} Model {} Stepping {}",
            c.family, c.model, c.stepping
        );
        let per_socket_cores = (c.cores / c.sockets).max(1);
        let per_socket_logical = (c.logical / c.sockets).max(1);
        (0..c.sockets)
            .map(|i| {
                vec![
                    u16p("AddressWidth", 64),
                    u16p("Architecture", 9),
                    u16p("Availability", 3),
                    s("Caption", caption.clone()),
                    u32p("CurrentClockSpeed", c.mhz),
                    u16p("DataWidth", 64),
                    s("Description", caption.clone()),
                    s("DeviceID", format!("CPU{i}")),
                    u16p("Family", 2),
                    (
                        "L2CacheSize".into(),
                        CIM_UINT32,
                        cache_kb("2").map_or(Val::Null, Val::U32),
                    ),
                    (
                        "L3CacheSize".into(),
                        CIM_UINT32,
                        cache_kb("3").map_or(Val::Null, Val::U32),
                    ),
                    u16p("Level", c.family as u16),
                    s("Manufacturer", c.vendor.clone()),
                    u32p("MaxClockSpeed", c.mhz),
                    s("Name", c.name.clone()),
                    u32p("NumberOfCores", per_socket_cores),
                    u32p("NumberOfEnabledCore", per_socket_cores),
                    u32p("NumberOfLogicalProcessors", per_socket_logical),
                    s("ProcessorId", processor_id()),
                    u16p("ProcessorType", 3),
                    u16p("Revision", ((c.model << 8) | c.stepping) as u16),
                    s("Role", "CPU"),
                    s("SocketDesignation", format!("CPU{i}")),
                    s("Status", "OK"),
                    s("Stepping", c.stepping.to_string()),
                    s("SystemName", computer_name()),
                    u32p("ThreadCount", per_socket_logical),
                    b("VirtualizationFirmwareEnabled", c.virt),
                    b("VMMonitorModeExtensions", c.virt),
                ]
            })
            .collect()
    }

    fn laptop() -> bool {
        std::fs::read_dir("/sys/class/power_supply").is_ok_and(|d| {
            d.flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("BAT"))
        })
    }

    pub fn computer_system() -> Vec<Prop> {
        let c = cpu();
        let (total, _) = meminfo();
        let name = computer_name();
        let host = super::super::kernel32::get_hostname_string();
        vec![
            b("AutomaticResetBootOption", true),
            s("BootupState", "Normal boot"),
            s("Caption", name.clone()),
            s("DNSHostName", host),
            s("Domain", "WORKGROUP"),
            u16p("DomainRole", 0),
            b("HypervisorPresent", c.hypervisor),
            s(
                "Manufacturer",
                dmi("sys_vendor").unwrap_or_else(|| "Winlux".into()),
            ),
            s(
                "Model",
                dmi("product_name").unwrap_or_else(|| "Winlux".into()),
            ),
            s("Name", name),
            u32p("NumberOfLogicalProcessors", c.logical),
            u32p("NumberOfProcessors", c.sockets),
            b("PartOfDomain", false),
            u16p("PCSystemType", if laptop() { 2 } else { 1 }),
            s("PrimaryOwnerName", crate::paths::user_name()),
            s("Status", "OK"),
            opt("SystemFamily", dmi("product_family")),
            opt("SystemSKUNumber", dmi("product_sku")),
            s("SystemType", "x64-based PC"),
            u64p("TotalPhysicalMemory", total * 1024),
            s(
                "UserName",
                format!("{}\\{}", computer_name(), crate::paths::user_name()),
            ),
            s("Workgroup", "WORKGROUP"),
        ]
    }

    pub fn base_board() -> Vec<Prop> {
        vec![
            s("Caption", "Base Board"),
            s("Description", "Base Board"),
            b("HostingBoard", true),
            opt("Manufacturer", dmi("board_vendor")),
            s("Name", "Base Board"),
            opt("Product", dmi("board_name")),
            s("SerialNumber", dmi("board_serial").unwrap_or_default()),
            s("Status", "OK"),
            s("Tag", "Base Board"),
            opt("Version", dmi("board_version")),
        ]
    }

    /// « mm/dd/yyyy » de la DMI → date CIM à minuit UTC.
    fn bios_date() -> Option<String> {
        let d = dmi("bios_date")?;
        let mut it = d.split('/');
        let (m, dd, y) = (it.next()?, it.next()?, it.next()?);
        Some(format!("{y:0>4}{m:0>2}{dd:0>2}000000.000000+000"))
    }

    pub fn bios() -> Vec<Prop> {
        let version = dmi("bios_version").unwrap_or_default();
        let vendor = dmi("bios_vendor").unwrap_or_default();
        vec![
            s("Caption", version.clone()),
            s("Description", version.clone()),
            opt(
                "Manufacturer",
                Some(vendor.clone()).filter(|v| !v.is_empty()),
            ),
            s("Name", version.clone()),
            b("PrimaryBIOS", true),
            date("ReleaseDate", bios_date()),
            s("SerialNumber", dmi("product_serial").unwrap_or_default()),
            s("SMBIOSBIOSVersion", version.clone()),
            b("SMBIOSPresent", !vendor.is_empty()),
            s("SoftwareElementID", version.clone()),
            u16p("SoftwareElementState", 3),
            s("Status", "OK"),
            u16p("TargetOperatingSystem", 0),
            s(
                "Version",
                format!("{} - 1", vendor.chars().take(8).collect::<String>()),
            ),
        ]
    }

    fn vendor_name(id: u32) -> &'static str {
        match id {
            0x10DE => "NVIDIA",
            0x1002 | 0x1022 => "Advanced Micro Devices, Inc.",
            0x8086 => "Intel Corporation",
            0x1AF4 => "Red Hat, Inc.",
            0x15AD => "VMware, Inc.",
            _ => "Winlux",
        }
    }

    pub fn video_controllers() -> Vec<Vec<Prop>> {
        let desc = super::super::dxgi::adapter_desc();
        let len = desc.description.iter().position(|&c| c == 0).unwrap_or(128);
        let name = String::from_utf16_lossy(&desc.description[..len]);
        let (w, h) = super::super::gui::screen_size();
        let vram = desc.dedicated_video_memory as u64;
        vec![vec![
            s("AdapterCompatibility", vendor_name(desc.vendor_id)),
            s("AdapterDACType", "Internal"),
            u32p("AdapterRAM", vram.min(u32::MAX as u64) as u32),
            u16p("Availability", 3),
            s("Caption", name.clone()),
            u32p("CurrentBitsPerPixel", 32),
            u32p("CurrentHorizontalResolution", w.max(0) as u32),
            u64p("CurrentNumberOfColors", 4_294_967_296),
            u32p("CurrentRefreshRate", 60),
            u32p("CurrentVerticalResolution", h.max(0) as u32),
            s("Description", name.clone()),
            s("DeviceID", "VideoController1"),
            s("DriverVersion", "1.0.0.0"),
            s("Name", name.clone()),
            s(
                "PNPDeviceID",
                format!("PCI\\VEN_{:04X}&DEV_{:04X}", desc.vendor_id, desc.device_id),
            ),
            s("Status", "OK"),
            u16p("VideoArchitecture", 5),
            u16p("VideoMemoryType", 2),
            s(
                "VideoModeDescription",
                format!("{w} x {h} x 4294967296 colors"),
            ),
            s("VideoProcessor", name),
        ]]
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn logical_disks() -> Vec<Vec<Prop>> {
        use super::super::kernel32 as k;
        let mask = k::get_logical_drives();
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = wide(&format!("{letter}:\\"));
            // SAFETY : tampons locaux de taille annoncée.
            let kind = unsafe { k::get_drive_type_w(root.as_ptr()) };
            let mut label = [0u16; 261];
            let mut fs = [0u16; 261];
            let mut serial = 0u32;
            let (mut maxc, mut flags) = (0u32, 0u32);
            let has_volume = unsafe {
                k::get_volume_information_w(
                    root.as_ptr(),
                    label.as_mut_ptr(),
                    261,
                    &mut serial,
                    &mut maxc,
                    &mut flags,
                    fs.as_mut_ptr(),
                    261,
                )
            } != 0;
            let (mut free, mut total, mut total_free) = (0i64, 0i64, 0i64);
            let has_space = unsafe {
                k::get_disk_free_space_ex_w(root.as_ptr(), &mut free, &mut total, &mut total_free)
            } != 0;
            let text = |b: &[u16]| {
                let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                String::from_utf16_lossy(&b[..n])
            };
            let id = format!("{letter}:");
            let description = match kind {
                2 => "Removable Disk",
                4 => "Network Connection",
                5 => "CD-ROM Disc",
                _ => "Local Fixed Disk",
            };
            out.push(vec![
                s("Caption", id.clone()),
                b("Compressed", false),
                s("Description", description),
                s("DeviceID", id.clone()),
                u32p("DriveType", kind),
                opt("FileSystem", has_volume.then(|| text(&fs))),
                (
                    "FreeSpace".into(),
                    CIM_UINT64,
                    if has_space {
                        Val::U64(total_free as u64)
                    } else {
                        Val::Null
                    },
                ),
                (
                    "MaximumComponentLength".into(),
                    CIM_UINT32,
                    if has_volume {
                        Val::U32(maxc)
                    } else {
                        Val::Null
                    },
                ),
                (
                    "MediaType".into(),
                    CIM_UINT32,
                    if kind == 3 { Val::U32(12) } else { Val::Null },
                ),
                s("Name", id),
                (
                    "Size".into(),
                    CIM_UINT64,
                    if has_space {
                        Val::U64(total as u64)
                    } else {
                        Val::Null
                    },
                ),
                s("SystemName", computer_name()),
                opt("VolumeName", has_volume.then(|| text(&label))),
                opt(
                    "VolumeSerialNumber",
                    has_volume.then(|| format!("{serial:08X}")),
                ),
            ]);
        }
        out
    }

    pub fn disk_drives() -> Vec<Vec<Prop>> {
        let mut names: Vec<String> = std::fs::read_dir("/sys/block")
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| {
                        !["loop", "ram", "zram", "dm-", "sr", "fd", "md"]
                            .iter()
                            .any(|p| n.starts_with(p))
                    })
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let dir = format!("/sys/block/{n}");
                let sectors: u64 = read(&format!("{dir}/size"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let sector: u32 = read(&format!("{dir}/queue/logical_block_size"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(512);
                let removable = read(&format!("{dir}/removable")).as_deref() == Some("1");
                let usb =
                    std::fs::canonicalize(&dir).is_ok_and(|p| p.to_string_lossy().contains("/usb"));
                let model = read(&format!("{dir}/device/model"))
                    .or_else(|| read(&format!("{dir}/device/vendor")).map(|v| format!("{v} Disk")))
                    .unwrap_or_else(|| format!("{} Disk Device", n.to_ascii_uppercase()));
                let partitions = std::fs::read_dir(&dir)
                    .map(|d| {
                        d.flatten()
                            .filter(|e| e.file_name().to_string_lossy().starts_with(n.as_str()))
                            .count()
                    })
                    .unwrap_or(0);
                vec![
                    u16p("BytesPerSector", sector as u16 as u32 as u16),
                    s("Caption", model.clone()),
                    s("Description", "Disk drive"),
                    s("DeviceID", format!("\\\\.\\PHYSICALDRIVE{i}")),
                    opt(
                        "FirmwareRevision",
                        read(&format!("{dir}/device/firmware_rev"))
                            .or_else(|| read(&format!("{dir}/device/rev"))),
                    ),
                    u32p("Index", i as u32),
                    s("InterfaceType", if usb { "USB" } else { "SCSI" }),
                    s(
                        "MediaType",
                        if removable || usb {
                            "Removable Media"
                        } else {
                            "Fixed hard disk media"
                        },
                    ),
                    s("Model", model.clone()),
                    s("Name", format!("\\\\.\\PHYSICALDRIVE{i}")),
                    u32p("Partitions", partitions as u32),
                    s(
                        "SerialNumber",
                        read(&format!("{dir}/device/serial")).unwrap_or_default(),
                    ),
                    u64p("Size", sectors * 512),
                    s("Status", "OK"),
                    s("SystemName", computer_name()),
                ]
            })
            .collect()
    }

    fn mask_v4(prefix: u8) -> String {
        let m: u32 = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix.min(32) as u32)
        };
        std::net::Ipv4Addr::from(m).to_string()
    }

    pub fn network_configs() -> Vec<Vec<Prop>> {
        let dns: Vec<String> = std::fs::read_to_string("/etc/resolv.conf")
            .map(|t| super::super::iphlpapi::dns_servers(&t))
            .unwrap_or_default()
            .iter()
            .map(|a| a.to_string())
            .collect();
        let host = super::super::kernel32::get_hostname_string();
        super::super::iphlpapi::adapters()
            .into_iter()
            .filter(|a| !a.loopback)
            .map(|a| {
                let mut addrs = a.addrs.clone();
                addrs.sort_by_key(|(ip, _)| ip.is_ipv6());
                let ips: Vec<String> = addrs.iter().map(|(ip, _)| ip.to_string()).collect();
                let masks: Vec<String> = addrs
                    .iter()
                    .map(|(ip, p)| {
                        if ip.is_ipv4() {
                            mask_v4(*p)
                        } else {
                            p.to_string()
                        }
                    })
                    .collect();
                let enabled = a.up && !ips.is_empty();
                let mac = (a.mac.len() == 6).then(|| {
                    a.mac
                        .iter()
                        .map(|b| format!("{b:02X}"))
                        .collect::<Vec<_>>()
                        .join(":")
                });
                vec![
                    s("Caption", format!("[{:08}] {}", a.index, a.name)),
                    b("DHCPEnabled", true),
                    strs(
                        "DefaultIPGateway",
                        a.gateways.iter().map(|g| g.to_string()).collect(),
                    ),
                    s("Description", a.name.clone()),
                    opt("DNSHostName", enabled.then(|| host.clone())),
                    strs(
                        "DNSServerSearchOrder",
                        if enabled { dns.clone() } else { Vec::new() },
                    ),
                    u32p("Index", a.index),
                    u32p("InterfaceIndex", a.index),
                    strs("IPAddress", ips),
                    b("IPEnabled", enabled),
                    strs("IPSubnet", masks),
                    opt("MACAddress", mac),
                    u32p("MTU", a.mtu),
                    s("ServiceName", a.name.clone()),
                    s("SettingID", super::super::iphlpapi::adapter_guid(&a)),
                ]
            })
            .collect()
    }

    pub fn processes() -> Vec<Vec<Prop>> {
        let me = std::process::id();
        let boot = boot_time().unwrap_or(0);
        // SAFETY : sysconf sans effet de bord.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as i64;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
        let mut out = Vec::new();
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return out;
        };
        let mut pids: Vec<u32> = dir
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse().ok())
            .collect();
        pids.sort_unstable();
        for pid in pids {
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
                continue;
            };
            let raw = &stat[open + 1..close];
            let rest: Vec<&str> = stat[close + 1..].split_whitespace().collect();
            let ppid: u32 = rest.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            let threads: u32 = rest.get(17).and_then(|v| v.parse().ok()).unwrap_or(1);
            let start: i64 = rest.get(19).and_then(|v| v.parse().ok()).unwrap_or(0);
            let rss: u64 = rest.get(21).and_then(|v| v.parse().ok()).unwrap_or(0);
            let name = if raw.to_ascii_lowercase().ends_with(".exe") {
                raw.to_string()
            } else {
                format!("{raw}.exe")
            };
            let (path, cmd) = if pid == me {
                let mut buf = [0u16; 1024];
                // SAFETY : tampon local de 1024 caractères.
                let n = unsafe {
                    super::super::kernel32::get_module_file_name_w(0, buf.as_mut_ptr(), 1024)
                } as usize;
                let cl = super::super::kernel32::get_command_line_w() as *const u16;
                let cmd = (!cl.is_null()).then(|| {
                    let mut len = 0;
                    unsafe {
                        while *cl.add(len) != 0 {
                            len += 1;
                        }
                        String::from_utf16_lossy(std::slice::from_raw_parts(cl, len))
                    }
                });
                (
                    (n > 0).then(|| String::from_utf16_lossy(&buf[..n.min(1024)])),
                    cmd,
                )
            } else {
                (None, None)
            };
            out.push(vec![
                s("Caption", name.clone()),
                opt("CommandLine", cmd),
                date("CreationDate", Some(cim_date(boot + start / hz))),
                s("Description", name.clone()),
                opt("ExecutablePath", path),
                s("Handle", pid.to_string()),
                s("Name", name),
                u32p("ParentProcessId", ppid),
                u32p("Priority", 8),
                u32p("ProcessId", pid),
                u32p("SessionId", 1),
                u32p("ThreadCount", threads),
                u64p("WorkingSetSize", rss * page),
            ]);
        }
        out
    }
}

// ── WQL ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Num(String),
    Op(String),
    Comma,
    Star,
    Open,
    Close,
}

fn lex(q: &str) -> Option<Vec<Tok>> {
    let c: Vec<char> = q.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '\'' || ch == '"' {
            let mut s = String::new();
            i += 1;
            loop {
                let x = *c.get(i)?;
                i += 1;
                if x == ch {
                    break;
                }
                if x == '\\' && i < c.len() && (c[i] == ch || c[i] == '\\') {
                    s.push(c[i]);
                    i += 1;
                } else {
                    s.push(x);
                }
            }
            out.push(Tok::Str(s));
        } else if ch.is_ascii_digit()
            || (ch == '-' && c.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            i += 1;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(c[start..i].iter().collect()));
        } else if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Word(c[start..i].iter().collect()));
        } else {
            i += 1;
            out.push(match ch {
                ',' => Tok::Comma,
                '*' => Tok::Star,
                '(' => Tok::Open,
                ')' => Tok::Close,
                '=' => Tok::Op("=".into()),
                '<' | '>' | '!' => {
                    let mut op = ch.to_string();
                    if let Some(&n) = c.get(i) {
                        if n == '=' || (ch == '<' && n == '>') {
                            op.push(n);
                            i += 1;
                        }
                    }
                    if op == "!" {
                        return None;
                    }
                    Tok::Op(op)
                }
                _ => return None,
            });
        }
    }
    Some(out)
}

#[derive(Debug, Clone)]
enum Cond {
    Cmp(String, String, Val),
    Like(String, String, bool),
    IsNull(String, bool),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
}

#[derive(Debug)]
pub struct Query {
    pub class: &'static str,
    /// None : toutes les propriétés.
    pub props: Option<Vec<String>>,
    cond: Option<Cond>,
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn keyword(&mut self, k: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Word(w)) if w.eq_ignore_ascii_case(k)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn or(&mut self) -> Option<Cond> {
        let mut left = self.and()?;
        while self.keyword("OR") {
            left = Cond::Or(Box::new(left), Box::new(self.and()?));
        }
        Some(left)
    }
    fn and(&mut self) -> Option<Cond> {
        let mut left = self.unary()?;
        while self.keyword("AND") {
            left = Cond::And(Box::new(left), Box::new(self.unary()?));
        }
        Some(left)
    }
    fn unary(&mut self) -> Option<Cond> {
        if self.keyword("NOT") {
            return Some(Cond::Not(Box::new(self.unary()?)));
        }
        if self.peek() == Some(&Tok::Open) {
            self.i += 1;
            let c = self.or()?;
            (self.next()? == Tok::Close).then_some(c)
        } else {
            self.cmp()
        }
    }
    fn literal(&mut self) -> Option<Val> {
        Some(match self.next()? {
            Tok::Str(s) => Val::Str(s),
            Tok::Num(n) => {
                if let Ok(v) = n.parse::<i64>() {
                    if v < 0 {
                        Val::I32(v as i32)
                    } else {
                        Val::U64(v as u64)
                    }
                } else {
                    Val::Str(n)
                }
            }
            Tok::Word(w) if w.eq_ignore_ascii_case("TRUE") => Val::Bool(true),
            Tok::Word(w) if w.eq_ignore_ascii_case("FALSE") => Val::Bool(false),
            Tok::Word(w) if w.eq_ignore_ascii_case("NULL") => Val::Null,
            _ => return None,
        })
    }
    fn cmp(&mut self) -> Option<Cond> {
        let Tok::Word(prop) = self.next()? else {
            return None;
        };
        if self.keyword("IS") {
            let not = self.keyword("NOT");
            return self.keyword("NULL").then_some(Cond::IsNull(prop, !not));
        }
        let not = self.keyword("NOT");
        if self.keyword("LIKE") {
            let Some(Tok::Str(p)) = self.next() else {
                return None;
            };
            return Some(Cond::Like(prop, p, !not));
        }
        if not {
            return None;
        }
        let Tok::Op(op) = self.next()? else {
            return None;
        };
        let v = self.literal()?;
        Some(Cond::Cmp(prop, op, v))
    }
}

pub fn parse_query(q: &str) -> Result<Query, i32> {
    let t = lex(q).ok_or(WBEM_E_INVALID_QUERY)?;
    let mut p = Parser { t, i: 0 };
    if !p.keyword("SELECT") {
        return Err(WBEM_E_INVALID_QUERY);
    }
    let props = if p.peek() == Some(&Tok::Star) {
        p.i += 1;
        None
    } else {
        let mut list = Vec::new();
        loop {
            match p.next() {
                Some(Tok::Word(w)) => list.push(w),
                _ => return Err(WBEM_E_INVALID_QUERY),
            }
            if p.peek() == Some(&Tok::Comma) {
                p.i += 1;
            } else {
                break;
            }
        }
        Some(list)
    };
    if !p.keyword("FROM") {
        return Err(WBEM_E_INVALID_QUERY);
    }
    let Some(Tok::Word(class)) = p.next() else {
        return Err(WBEM_E_INVALID_QUERY);
    };
    let class = canonical_class(&class).ok_or(WBEM_E_INVALID_CLASS)?;
    let cond = if p.keyword("WHERE") {
        Some(p.or().ok_or(WBEM_E_INVALID_QUERY)?)
    } else {
        None
    };
    if p.i != p.t.len() {
        return Err(WBEM_E_INVALID_QUERY);
    }
    Ok(Query { class, props, cond })
}

fn like(text: &str, pattern: &str) -> bool {
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    fn go(t: &[char], p: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('%') => (0..=t.len()).any(|i| go(&t[i..], &p[1..])),
            Some('_') => !t.is_empty() && go(&t[1..], &p[1..]),
            Some(c) => t.first() == Some(c) && go(&t[1..], &p[1..]),
        }
    }
    go(&t, &p)
}

fn matches(o: &Object, c: &Cond) -> bool {
    let val = |name: &str| o.get(name).map(|p| p.2.clone());
    match c {
        Cond::And(a, b) => matches(o, a) && matches(o, b),
        Cond::Or(a, b) => matches(o, a) || matches(o, b),
        Cond::Not(a) => !matches(o, a),
        Cond::IsNull(p, want) => val(p).is_none_or(|v| v == Val::Null) == *want,
        Cond::Like(p, pat, want) => {
            val(p).and_then(|v| v.text()).is_some_and(|t| like(&t, pat)) == *want
        }
        Cond::Cmp(p, op, lit) => {
            let Some(v) = val(p) else {
                return false;
            };
            if v == Val::Null || *lit == Val::Null {
                return false;
            }
            let ord = match (
                v.number(),
                lit.number().or_else(|| lit.text()?.parse().ok()),
            ) {
                (Some(a), Some(b)) => a.partial_cmp(&b),
                _ => {
                    let (a, b) = (v.text().unwrap_or_default(), lit.text().unwrap_or_default());
                    Some(a.to_lowercase().cmp(&b.to_lowercase()))
                }
            };
            let Some(ord) = ord else {
                return false;
            };
            use std::cmp::Ordering::*;
            match op.as_str() {
                "=" => ord == Equal,
                "<>" | "!=" => ord != Equal,
                "<" => ord == Less,
                ">" => ord == Greater,
                "<=" => ord != Greater,
                ">=" => ord != Less,
                _ => false,
            }
        }
    }
}

/// Exécute une requête WQL : objets résultats, ou code WBEM_E_*.
pub fn run_query(q: &str) -> Result<Vec<Object>, i32> {
    let query = parse_query(q)?;
    let all = instances(query.class).ok_or(WBEM_E_INVALID_CLASS)?;
    if let Some(list) = &query.props {
        let sample = class_object(query.class).ok_or(WBEM_E_INVALID_CLASS)?;
        if list.iter().any(|n| sample.get(n).is_none()) {
            return Err(WBEM_E_INVALID_QUERY);
        }
    }
    Ok(all
        .into_iter()
        .filter(|o| query.cond.as_ref().is_none_or(|c| matches(o, c)))
        .map(|mut o| {
            if let Some(list) = &query.props {
                let key = o.key();
                o.props.retain(|p| {
                    p.0.eq_ignore_ascii_case(key)
                        || list.iter().any(|n| n.eq_ignore_ascii_case(&p.0))
                });
            }
            o
        })
        .collect())
}

/// GetObject : « Classe », « Classe=@ », « Classe.Clé="valeur" », avec ou
/// sans préfixe d'espace de noms (\\.\root\cimv2:).
pub fn object_at(path: &str) -> Result<Object, i32> {
    let path = path.rsplit_once(':').map_or(path, |(_, p)| p).trim();
    let (class, rest) = match path.find(['.', '=']) {
        Some(i) => (&path[..i], Some(&path[i..])),
        None => (path, None),
    };
    let Some(rest) = rest else {
        return class_object(class).ok_or(WBEM_E_INVALID_CLASS);
    };
    let all = instances(class).ok_or(WBEM_E_INVALID_CLASS)?;
    if rest == "=@" {
        return all.into_iter().next().ok_or(WBEM_E_NOT_FOUND);
    }
    let rest = rest.strip_prefix('.').ok_or(WBEM_E_INVALID_OBJECT_PATH)?;
    let mut keys = Vec::new();
    for part in rest.split(',') {
        let (k, v) = part.split_once('=').ok_or(WBEM_E_INVALID_OBJECT_PATH)?;
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .map(|v| v.replace("\\\\", "\\").replace("\\\"", "\""))
            .unwrap_or_else(|| v.to_string());
        keys.push((k.trim().to_string(), v));
    }
    all.into_iter()
        .find(|o| {
            keys.iter().all(|(k, v)| {
                o.get(k)
                    .and_then(|p| p.2.text())
                    .is_some_and(|t| t.eq_ignore_ascii_case(v))
            })
        })
        .ok_or(WBEM_E_NOT_FOUND)
}

// ── Objets COM ─────────────────────────────────────────────────────

enum What {
    Locator,
    Services,
    Enum(Mutex<(Vec<Object>, usize)>),
    Object(Mutex<(Object, usize, i32)>),
}

#[repr(C)]
struct Com {
    vtbl: *const usize,
    refs: AtomicU32,
    what: What,
}

struct Tables {
    locator: Vec<usize>,
    services: Vec<usize>,
    enumerator: Vec<usize>,
    object: Vec<usize>,
}
static TABLES: OnceLock<Tables> = OnceLock::new();

macro_rules! table {
    ($($f:expr),* $(,)?) => { vec![$($f as *const () as usize),*] };
}

fn tables() -> &'static Tables {
    TABLES.get_or_init(|| {
        let unknown = table![query_interface, add_ref, release];
        let mut locator = unknown.clone();
        locator.extend(table![connect_server]);
        let mut services = unknown.clone();
        services.extend(table![
            open_namespace,       // OpenNamespace
            not_supported,        // CancelAsyncCall
            not_supported,        // QueryObjectSink
            get_object,           // GetObject
            not_supported,        // GetObjectAsync
            access_denied,        // PutClass
            not_supported,        // PutClassAsync
            access_denied,        // DeleteClass
            not_supported,        // DeleteClassAsync
            create_class_enum,    // CreateClassEnum
            not_supported,        // CreateClassEnumAsync
            access_denied,        // PutInstance
            not_supported,        // PutInstanceAsync
            access_denied,        // DeleteInstance
            not_supported,        // DeleteInstanceAsync
            create_instance_enum, // CreateInstanceEnum
            not_supported,        // CreateInstanceEnumAsync
            exec_query,           // ExecQuery
            not_supported,        // ExecQueryAsync
            not_supported,        // ExecNotificationQuery
            not_supported,        // ExecNotificationQueryAsync
            not_supported,        // ExecMethod
            not_supported,        // ExecMethodAsync
        ]);
        let mut enumerator = unknown.clone();
        enumerator.extend(table![e_reset, e_next, not_supported, e_clone, e_skip]);
        let mut object = unknown;
        object.extend(table![
            not_supported, // GetQualifierSet
            o_get,
            o_put,
            o_delete,
            o_get_names,
            o_begin_enumeration,
            o_next,
            o_end_enumeration,
            not_supported, // GetPropertyQualifierSet
            o_clone,
            o_get_object_text,
            not_supported, // SpawnDerivedClass
            o_spawn_instance,
            o_compare_to,
            o_get_property_origin,
            o_inherits_from,
            not_supported, // GetMethod
            not_supported, // PutMethod
            not_supported, // DeleteMethod
            o_begin_method_enumeration,
            o_next_method,
            s_ok,          // EndMethodEnumeration
            not_supported, // GetMethodQualifierSet
            not_supported, // GetMethodOrigin
        ]);
        Tables {
            locator,
            services,
            enumerator,
            object,
        }
    })
}

fn new_com(what: What) -> *mut Com {
    let t = tables();
    let vt = match &what {
        What::Locator => &t.locator,
        What::Services => &t.services,
        What::Enum(_) => &t.enumerator,
        What::Object(_) => &t.object,
    };
    Box::into_raw(Box::new(Com {
        vtbl: vt.as_ptr(),
        refs: AtomicU32::new(1),
        what,
    }))
}

fn new_object(o: Object) -> *mut Com {
    new_com(What::Object(Mutex::new((o, 0, 0))))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn com<'a>(this: *mut Com) -> &'a Com {
    &*this
}

unsafe extern "win64" fn query_interface(
    this: *mut Com,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_POINTER;
    }
    let own = match com(this).what {
        What::Locator => IID_IWBEM_LOCATOR,
        What::Services => IID_IWBEM_SERVICES,
        What::Enum(_) => IID_IENUM_WBEM_CLASS_OBJECT,
        What::Object(_) => IID_IWBEM_CLASS_OBJECT,
    };
    if *iid == IID_IUNKNOWN || *iid == own {
        add_ref(this);
        *out = this.cast();
        return S_OK;
    }
    super::comobj::log_no_interface("wbem", iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn add_ref(this: *mut Com) -> u32 {
    com(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn release(this: *mut Com) -> u32 {
    let n = com(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}

extern "win64" fn not_supported() -> i32 {
    WBEM_E_NOT_SUPPORTED
}

extern "win64" fn access_denied() -> i32 {
    WBEM_E_ACCESS_DENIED
}

extern "win64" fn s_ok() -> i32 {
    S_OK
}

unsafe fn read_wide(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)))
}

/// Espaces de noms servis : ROOT\CIMV2 (et ROOT\DEFAULT, vide).
fn namespace_ok(ns: &str) -> bool {
    let ns = ns.replace('/', "\\").to_ascii_lowercase();
    let ns = ns.trim_start_matches('\\');
    // \\.\root\cimv2 ou \\machine\root\cimv2
    let ns = match ns.split_once('\\') {
        Some((host, rest)) if !host.eq_ignore_ascii_case("root") => rest,
        _ => ns,
    };
    matches!(ns, "root\\cimv2" | "root\\default" | "cimv2")
}

unsafe extern "win64" fn connect_server(
    _this: *mut Com,
    resource: *const u16,
    _user: *const u16,
    _password: *const u16,
    _locale: *const u16,
    _flags: i32,
    _authority: *const u16,
    _ctx: usize,
    out: *mut *mut Com,
) -> i32 {
    if out.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    *out = std::ptr::null_mut();
    let ns = read_wide(resource).unwrap_or_default();
    if !namespace_ok(&ns) {
        crate::trace!("WMI : espace de noms {ns} non servi");
        return WBEM_E_INVALID_NAMESPACE;
    }
    *out = new_com(What::Services);
    S_OK
}

unsafe extern "win64" fn open_namespace(
    _this: *mut Com,
    ns: *const u16,
    _flags: i32,
    _ctx: usize,
    out: *mut *mut Com,
    result: *mut usize,
) -> i32 {
    if !result.is_null() {
        *result = 0;
    }
    let ns = read_wide(ns).unwrap_or_default();
    if out.is_null() || !namespace_ok(&format!("root\\{ns}")) && !namespace_ok(&ns) {
        return WBEM_E_INVALID_NAMESPACE;
    }
    *out = new_com(What::Services);
    S_OK
}

unsafe extern "win64" fn get_object(
    _this: *mut Com,
    path: *const u16,
    _flags: i32,
    _ctx: usize,
    out: *mut *mut Com,
    result: *mut usize,
) -> i32 {
    if !result.is_null() {
        *result = 0;
    }
    if out.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    *out = std::ptr::null_mut();
    let Some(path) = read_wide(path).filter(|p| !p.is_empty()) else {
        return WBEM_E_INVALID_OBJECT_PATH;
    };
    match object_at(&path) {
        Ok(o) => {
            *out = new_object(o);
            S_OK
        }
        Err(hr) => hr,
    }
}

unsafe fn put_enum(out: *mut *mut Com, list: Vec<Object>) -> i32 {
    if out.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    *out = new_com(What::Enum(Mutex::new((list, 0))));
    S_OK
}

unsafe extern "win64" fn create_class_enum(
    _this: *mut Com,
    superclass: *const u16,
    _flags: i32,
    _ctx: usize,
    out: *mut *mut Com,
) -> i32 {
    let base = read_wide(superclass).unwrap_or_default();
    let list = CLASSES
        .iter()
        .filter(|c| base.is_empty() || c.eq_ignore_ascii_case(&base))
        .filter_map(|c| class_object(c))
        .collect();
    put_enum(out, list)
}

unsafe extern "win64" fn create_instance_enum(
    _this: *mut Com,
    class: *const u16,
    _flags: i32,
    _ctx: usize,
    out: *mut *mut Com,
) -> i32 {
    let class = read_wide(class).unwrap_or_default();
    match instances(&class) {
        Some(list) => put_enum(out, list),
        None => WBEM_E_INVALID_CLASS,
    }
}

unsafe extern "win64" fn exec_query(
    _this: *mut Com,
    language: *const u16,
    query: *const u16,
    _flags: i32,
    _ctx: usize,
    out: *mut *mut Com,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    let lang = read_wide(language).unwrap_or_default();
    if !lang.eq_ignore_ascii_case("WQL") {
        return WBEM_E_INVALID_QUERY_TYPE;
    }
    let q = read_wide(query).unwrap_or_default();
    match run_query(&q) {
        Ok(list) => {
            crate::trace!("WMI : {q} → {} objet(s)", list.len());
            put_enum(out, list)
        }
        Err(hr) => {
            crate::trace!("WMI : requête refusée {q} ({hr:#x})");
            hr
        }
    }
}

fn enum_state(this: *mut Com) -> &'static Mutex<(Vec<Object>, usize)> {
    match unsafe { &(*this).what } {
        What::Enum(m) => unsafe { &*(m as *const _) },
        _ => unreachable!("table d'énumérateur sur un autre objet"),
    }
}

unsafe extern "win64" fn e_reset(this: *mut Com) -> i32 {
    lock(enum_state(this)).1 = 0;
    S_OK
}

unsafe extern "win64" fn e_next(
    this: *mut Com,
    _timeout: i32,
    count: u32,
    objects: *mut *mut Com,
    returned: *mut u32,
) -> i32 {
    if objects.is_null() || returned.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    let mut st = lock(enum_state(this));
    let mut n = 0u32;
    while n < count && st.1 < st.0.len() {
        *objects.add(n as usize) = new_object(st.0[st.1].clone());
        st.1 += 1;
        n += 1;
    }
    *returned = n;
    if n == count {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn e_clone(this: *mut Com, out: *mut *mut Com) -> i32 {
    if out.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    let st = lock(enum_state(this));
    *out = new_com(What::Enum(Mutex::new((st.0.clone(), st.1))));
    S_OK
}

unsafe extern "win64" fn e_skip(this: *mut Com, _timeout: i32, count: u32) -> i32 {
    let mut st = lock(enum_state(this));
    let left = st.0.len() - st.1;
    st.1 += (count as usize).min(left);
    if count as usize <= left {
        S_OK
    } else {
        S_FALSE
    }
}

fn obj_state(this: *mut Com) -> &'static Mutex<(Object, usize, i32)> {
    match unsafe { &(*this).what } {
        What::Object(m) => unsafe { &*(m as *const _) },
        _ => unreachable!("table d'objet sur un autre objet"),
    }
}

fn bstr(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe { super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32) }
}

/// SAFEARRAY de BSTR (bornes 0..n), chaînes allouées une à une.
unsafe fn string_array(list: &[String]) -> usize {
    let sa = super::oleaut32::safe_array_create_vector(VT_BSTR, 0, list.len() as u32);
    let mut p: *mut c_void = std::ptr::null_mut();
    if sa != 0 && super::oleaut32::safe_array_access_data(sa, &mut p) == 0 {
        for (i, s) in list.iter().enumerate() {
            (p as *mut *mut u16).add(i).write_unaligned(bstr(s));
        }
        super::oleaut32::safe_array_unaccess_data(sa);
    }
    sa
}

/// Écrit une valeur dans un VARIANT (24 octets, disposition x64).
unsafe fn write_variant(out: *mut u8, v: &Val) {
    std::ptr::write_bytes(out, 0, 24);
    let set = |vt: u16| (out as *mut u16).write_unaligned(vt);
    let data = out.add(8);
    match v {
        Val::Null => set(VT_NULL),
        Val::Str(s) | Val::Date(s) => {
            set(VT_BSTR);
            (data as *mut *mut u16).write_unaligned(bstr(s));
        }
        Val::U64(n) => {
            set(VT_BSTR);
            (data as *mut *mut u16).write_unaligned(bstr(&n.to_string()));
        }
        Val::I32(n) => {
            set(VT_I4);
            (data as *mut i32).write_unaligned(*n);
        }
        Val::U16(n) => {
            set(VT_I4);
            (data as *mut i32).write_unaligned(*n as i32);
        }
        Val::U32(n) => {
            set(VT_I4);
            (data as *mut i32).write_unaligned(*n as i32);
        }
        Val::U8(n) => {
            set(VT_UI1);
            *data = *n;
        }
        Val::Bool(b) => {
            set(VT_BOOL);
            (data as *mut i16).write_unaligned(if *b { -1 } else { 0 });
        }
        Val::Strs(list) => {
            set(VT_ARRAY | VT_BSTR);
            (data as *mut usize).write_unaligned(string_array(list));
        }
    }
}

unsafe fn put_prop(p: &Prop, val: *mut u8, kind: *mut i32, flavor: *mut i32) {
    if !val.is_null() {
        write_variant(val, &p.2);
    }
    if !kind.is_null() {
        kind.write_unaligned(p.1);
    }
    if !flavor.is_null() {
        // WBEM_FLAVOR_ORIGIN_SYSTEM (0x40) pour les propriétés système.
        flavor.write_unaligned(if p.0.starts_with("__") { 0x40 } else { 0 });
    }
}

unsafe extern "win64" fn o_get(
    this: *mut Com,
    name: *const u16,
    _flags: i32,
    val: *mut u8,
    kind: *mut i32,
    flavor: *mut i32,
) -> i32 {
    let Some(name) = read_wide(name) else {
        return WBEM_E_INVALID_PARAMETER;
    };
    let st = lock(obj_state(this));
    let o = &st.0;
    let found = o.get(&name).cloned().or_else(|| {
        o.system()
            .into_iter()
            .find(|p| p.0.eq_ignore_ascii_case(&name))
    });
    match found {
        Some(p) => {
            put_prop(&p, val, kind, flavor);
            S_OK
        }
        None => WBEM_E_NOT_FOUND,
    }
}

/// Lit un VARIANT d'entrée de Put (chaînes et entiers seulement).
unsafe fn read_variant(v: *const u8) -> Option<Val> {
    if v.is_null() {
        return Some(Val::Null);
    }
    let vt = (v as *const u16).read_unaligned();
    let data = v.add(8);
    Some(match vt {
        0 | VT_NULL => Val::Null,
        VT_BSTR => Val::Str(read_bstr((data as *const *const u16).read_unaligned())),
        VT_I4 => Val::I32((data as *const i32).read_unaligned()),
        VT_BOOL => Val::Bool((data as *const i16).read_unaligned() != 0),
        VT_UI1 => Val::U8(*data),
        _ => return None,
    })
}

unsafe fn read_bstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = super::oleaut32::sys_string_len(p) as usize;
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// Put : modifie la copie locale de l'objet (rien n'est réécrit dans le
/// système, comme une instance non envoyée par PutInstance).
unsafe extern "win64" fn o_put(
    this: *mut Com,
    name: *const u16,
    _flags: i32,
    val: *const u8,
    kind: i32,
) -> i32 {
    let (Some(name), Some(v)) = (read_wide(name), read_variant(val)) else {
        return WBEM_E_INVALID_PARAMETER;
    };
    if name.starts_with("__") {
        return WBEM_E_ACCESS_DENIED;
    }
    let mut st = lock(obj_state(this));
    match st
        .0
        .props
        .iter_mut()
        .find(|p| p.0.eq_ignore_ascii_case(&name))
    {
        Some(p) => p.2 = v,
        None => {
            let kind = if kind != 0 { kind } else { CIM_STRING };
            st.0.props.push((name, kind, v));
        }
    }
    S_OK
}

unsafe extern "win64" fn o_delete(this: *mut Com, name: *const u16) -> i32 {
    let Some(name) = read_wide(name) else {
        return WBEM_E_INVALID_PARAMETER;
    };
    let mut st = lock(obj_state(this));
    match st
        .0
        .props
        .iter_mut()
        .find(|p| p.0.eq_ignore_ascii_case(&name))
    {
        Some(p) => {
            p.2 = Val::Null;
            S_OK
        }
        None => WBEM_E_NOT_FOUND,
    }
}

unsafe extern "win64" fn o_get_names(
    this: *mut Com,
    _qualifier: *const u16,
    flags: i32,
    _qualifier_val: *const u8,
    names: *mut usize,
) -> i32 {
    if names.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    let list: Vec<String> = lock(obj_state(this))
        .0
        .listed(flags)
        .into_iter()
        .map(|p| p.0)
        .collect();
    *names = string_array(&list);
    S_OK
}

unsafe extern "win64" fn o_begin_enumeration(this: *mut Com, flags: i32) -> i32 {
    let mut st = lock(obj_state(this));
    st.1 = 0;
    st.2 = flags;
    S_OK
}

unsafe extern "win64" fn o_next(
    this: *mut Com,
    _flags: i32,
    name: *mut *mut u16,
    val: *mut u8,
    kind: *mut i32,
    flavor: *mut i32,
) -> i32 {
    let mut st = lock(obj_state(this));
    let list = st.0.listed(st.2);
    let Some(p) = list.get(st.1) else {
        return WBEM_S_NO_MORE_DATA;
    };
    st.1 += 1;
    if !name.is_null() {
        *name = bstr(&p.0);
    }
    put_prop(p, val, kind, flavor);
    S_OK
}

unsafe extern "win64" fn o_end_enumeration(this: *mut Com) -> i32 {
    lock(obj_state(this)).1 = usize::MAX / 2;
    S_OK
}

unsafe extern "win64" fn o_clone(this: *mut Com, out: *mut *mut Com) -> i32 {
    if out.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    *out = new_object(lock(obj_state(this)).0.clone());
    S_OK
}

fn mof_value(v: &Val) -> String {
    match v {
        Val::Str(s) | Val::Date(s) => {
            format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
        }
        Val::Strs(l) => format!(
            "{{{}}}",
            l.iter()
                .map(|s| format!("\"{}\"", s.replace('\\', "\\\\")))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => other.text().unwrap_or_default(),
    }
}

unsafe extern "win64" fn o_get_object_text(this: *mut Com, _flags: i32, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    let st = lock(obj_state(this));
    let o = &st.0;
    let mut text = format!("\ninstance of {}\n{{\n", o.class);
    for p in o.props.iter().filter(|p| p.2 != Val::Null) {
        text.push_str(&format!("\t{} = {};\n", p.0, mof_value(&p.2)));
    }
    text.push_str("};\n");
    *out = bstr(&text);
    S_OK
}

unsafe extern "win64" fn o_spawn_instance(this: *mut Com, _flags: i32, out: *mut *mut Com) -> i32 {
    if out.is_null() {
        return WBEM_E_INVALID_PARAMETER;
    }
    let mut o = lock(obj_state(this)).0.clone();
    o.instance = true;
    for p in &mut o.props {
        p.2 = Val::Null;
    }
    *out = new_object(o);
    S_OK
}

unsafe extern "win64" fn o_compare_to(this: *mut Com, _flags: i32, other: *mut Com) -> i32 {
    if other.is_null() || *(other as *const *const usize) != tables().object.as_ptr() {
        return WBEM_E_INVALID_PARAMETER;
    }
    let a = lock(obj_state(this)).0.props.clone();
    let b = lock(obj_state(other)).0.props.clone();
    if a == b {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn o_get_property_origin(
    this: *mut Com,
    name: *const u16,
    out: *mut *mut u16,
) -> i32 {
    let Some(name) = read_wide(name) else {
        return WBEM_E_INVALID_PARAMETER;
    };
    let st = lock(obj_state(this));
    if out.is_null() || st.0.get(&name).is_none() {
        return WBEM_E_NOT_FOUND;
    }
    *out = bstr(&st.0.class);
    S_OK
}

unsafe extern "win64" fn o_inherits_from(this: *mut Com, ancestor: *const u16) -> i32 {
    let a = read_wide(ancestor).unwrap_or_default();
    let class = lock(obj_state(this)).0.class.clone();
    // Classes Win32_* : dérivées de CIM_* au sens WMI.
    let base = class.trim_start_matches("Win32_");
    if a.eq_ignore_ascii_case(&format!("CIM_{base}")) {
        S_OK
    } else {
        WBEM_E_NOT_FOUND
    }
}

unsafe extern "win64" fn o_begin_method_enumeration(_this: *mut Com, _flags: i32) -> i32 {
    S_OK
}

unsafe extern "win64" fn o_next_method(
    _this: *mut Com,
    _flags: i32,
    name: *mut *mut u16,
    ins: *mut usize,
    outs: *mut usize,
) -> i32 {
    for p in [name as *mut usize, ins, outs] {
        if !p.is_null() {
            *p = 0;
        }
    }
    WBEM_S_NO_MORE_DATA
}

/// CoCreateInstance(CLSID_WbemLocator).
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    let iid = &*(iid as *const _ as *const Guid);
    if *clsid != CLSID_WBEM_LOCATOR {
        return None;
    }
    if out.is_null() {
        return Some(E_POINTER);
    }
    let loc = new_com(What::Locator);
    let hr = query_interface(loc, iid, out);
    release(loc);
    Some(hr)
}

#[cfg(test)]
mod tests;
