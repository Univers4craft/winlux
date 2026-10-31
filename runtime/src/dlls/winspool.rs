//! `winspool.drv` maison : API spooler Win32 au-dessus de CUPS (pont IPP de
//! `ipp.rs`) et DC imprimante GDI rendus en PDF (`print_pdf.rs`).
//!
//! Une imprimante virtuelle « Imprimer en PDF (Winlux) » est toujours
//! présente : elle enregistre le document dans `Documents` (ou dans le fichier
//! demandé par l'application), même quand CUPS n'est pas installé.

use super::ipp::{self, Client, CupsPrinter};
use super::print_pdf::{self, DocInfo, Page};
use crate::dlls::gui::{self, DrawCmd, FontSpec};
use crate::dlls::lock;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Nom de l'imprimante virtuelle PDF.
pub const PDF_PRINTER: &str = "Imprimer en PDF (Winlux)";

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_LEVEL: u32 = 124;
const ERROR_INVALID_PRINTER_NAME: u32 = 1801;
const ERROR_PRINTER_ALREADY_EXISTS: u32 = 1802;
const ERROR_SPL_NO_STARTDOC: u32 = 3003;
const ERROR_PRINT_CANCELLED: u32 = 63;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_NOT_READY: u32 = 21;

// ---------- Formats de papier ----------

/// Format de papier : identifiant `DMPAPER_*`, dimensions en 1/10 mm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paper {
    pub id: i16,
    pub name: &'static str,
    pub width: i32,
    pub height: i32,
    pub ipp: &'static str,
}

pub const PAPERS: [Paper; 9] = [
    Paper {
        id: 9,
        name: "A4",
        width: 2100,
        height: 2970,
        ipp: "iso_a4_210x297mm",
    },
    Paper {
        id: 1,
        name: "Lettre US",
        width: 2159,
        height: 2794,
        ipp: "na_letter_8.5x11in",
    },
    Paper {
        id: 5,
        name: "Légal US",
        width: 2159,
        height: 3556,
        ipp: "na_legal_8.5x14in",
    },
    Paper {
        id: 8,
        name: "A3",
        width: 2970,
        height: 4200,
        ipp: "iso_a3_297x420mm",
    },
    Paper {
        id: 11,
        name: "A5",
        width: 1480,
        height: 2100,
        ipp: "iso_a5_148x210mm",
    },
    Paper {
        id: 13,
        name: "B5 (JIS)",
        width: 1820,
        height: 2570,
        ipp: "jis_b5_182x257mm",
    },
    Paper {
        id: 7,
        name: "Exécutif",
        width: 1841,
        height: 2667,
        ipp: "na_executive_7.25x10.5in",
    },
    Paper {
        id: 27,
        name: "Enveloppe DL",
        width: 1100,
        height: 2200,
        ipp: "iso_dl_110x220mm",
    },
    Paper {
        id: 20,
        name: "Enveloppe n° 10",
        width: 1048,
        height: 2413,
        ipp: "na_number-10_4.125x9.5in",
    },
];

pub fn paper(id: i16) -> Paper {
    PAPERS
        .iter()
        .copied()
        .find(|p| p.id == id)
        .unwrap_or(PAPERS[0])
}

// ---------- DEVMODE ----------

pub const DM_ORIENTATION: u32 = 0x1;
pub const DM_PAPERSIZE: u32 = 0x2;
pub const DM_SCALE: u32 = 0x10;
pub const DM_COPIES: u32 = 0x100;
pub const DM_DEFAULTSOURCE: u32 = 0x200;
pub const DM_PRINTQUALITY: u32 = 0x400;
pub const DM_COLOR: u32 = 0x800;
pub const DM_DUPLEX: u32 = 0x1000;
pub const DM_COLLATE: u32 = 0x8000;
pub const DM_FORMNAME: u32 = 0x10000;

/// Taille de `DEVMODEW` / `DEVMODEA` (sans données privées du pilote).
pub const DEVMODEW_SIZE: usize = 220;
pub const DEVMODEA_SIZE: usize = 156;

/// Réglages d'impression (`DEVMODE`) indépendants du jeu de caractères.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevMode {
    pub device: String,
    /// 1 portrait, 2 paysage.
    pub orientation: i16,
    pub paper: i16,
    pub scale: i16,
    pub copies: i16,
    /// Qualité : < 0 = `DMRES_*` (-1 brouillon … -4 haute), > 0 = dpi.
    pub quality: i16,
    /// 1 monochrome, 2 couleur.
    pub color: i16,
    /// 1 recto, 2 recto-verso bord long, 3 bord court.
    pub duplex: i16,
    pub collate: i16,
}

impl DevMode {
    pub fn for_printer(p: &PrinterEntry) -> DevMode {
        DevMode {
            device: p.name.clone(),
            orientation: 1,
            paper: p.default_paper,
            scale: 100,
            copies: 1,
            quality: -3,
            color: if p.color { 2 } else { 1 },
            duplex: 1,
            collate: 1,
        }
    }

    pub fn landscape(&self) -> bool {
        self.orientation == 2
    }

    /// Résolution utilisée pour le DC imprimante.
    pub fn dpi(&self) -> i32 {
        match self.quality {
            q if q >= 72 => i32::from(q).min(1200),
            -4 => 600,
            _ => 300,
        }
    }

    pub fn ipp_quality(&self) -> i32 {
        match self.quality {
            -1 => 3,
            -4 => 5,
            q if q >= 600 => 5,
            _ => 4,
        }
    }

    pub fn job_options(&self) -> ipp::JobOptions {
        ipp::JobOptions {
            copies: i32::from(self.copies.max(1)),
            landscape: self.landscape(),
            duplex: self.duplex,
            color: self.color != 1,
            quality: self.ipp_quality(),
            media: paper(self.paper).ipp.to_string(),
            collate: self.collate != 0,
        }
    }

    const FIELDS: u32 = DM_ORIENTATION
        | DM_PAPERSIZE
        | DM_SCALE
        | DM_COPIES
        | DM_DEFAULTSOURCE
        | DM_PRINTQUALITY
        | DM_COLOR
        | DM_DUPLEX
        | DM_COLLATE
        | DM_FORMNAME;

    /// Sérialise en `DEVMODEW` (`wide`) ou `DEVMODEA`.
    pub fn to_bytes(&self, wide: bool) -> Vec<u8> {
        let (size, cs) = if wide {
            (DEVMODEW_SIZE, 2)
        } else {
            (DEVMODEA_SIZE, 1)
        };
        let mut b = vec![0u8; size];
        put_fixed_str(&mut b[..32 * cs], &self.device, wide);
        let base = 32 * cs; // dmSpecVersion
        let w16 = |b: &mut [u8], off: usize, v: i16| {
            b[off..off + 2].copy_from_slice(&v.to_le_bytes());
        };
        w16(&mut b, base, 0x0401);
        w16(&mut b, base + 2, 0x0100);
        w16(&mut b, base + 4, size as i16);
        b[base + 8..base + 12].copy_from_slice(&Self::FIELDS.to_le_bytes());
        let p = paper(self.paper);
        let u = base + 12; // union impression
        w16(&mut b, u, self.orientation);
        w16(&mut b, u + 2, self.paper);
        w16(&mut b, u + 4, p.height as i16);
        w16(&mut b, u + 6, p.width as i16);
        w16(&mut b, u + 8, self.scale);
        w16(&mut b, u + 10, self.copies);
        w16(&mut b, u + 12, 7); // DMBIN_AUTO
        w16(&mut b, u + 14, self.quality);
        w16(&mut b, u + 16, self.color);
        w16(&mut b, u + 18, self.duplex);
        w16(&mut b, u + 20, self.dpi() as i16);
        w16(&mut b, u + 22, 1); // DMTT_BITMAP
        w16(&mut b, u + 24, self.collate);
        let form = u + 26;
        put_fixed_str(&mut b[form..form + 32 * cs], p.name, wide);
        b
    }

    /// Fusionne un `DEVMODE` invité (seuls les champs annoncés par `dmFields`).
    ///
    /// # Sécurité
    /// `ptr` doit pointer sur un `DEVMODE` lisible du jeu de caractères indiqué.
    pub unsafe fn merge(&mut self, ptr: *const u8, wide: bool) {
        if ptr.is_null() {
            return;
        }
        let cs = if wide { 2 } else { 1 };
        let base = 32 * cs;
        let r16 = |off: usize| -> i16 { std::ptr::read_unaligned(ptr.add(off) as *const i16) };
        let size = r16(base + 4) as u16 as usize;
        let fields = std::ptr::read_unaligned(ptr.add(base + 8) as *const u32);
        let u = base + 12;
        // Un DEVMODE tronqué (anciennes versions) ne porte que ses premiers champs.
        let has = |flag: u32, end: usize| fields & flag != 0 && size >= end;
        if has(DM_ORIENTATION, u + 2) && matches!(r16(u), 1 | 2) {
            self.orientation = r16(u);
        }
        if has(DM_PAPERSIZE, u + 4) && PAPERS.iter().any(|p| p.id == r16(u + 2)) {
            self.paper = r16(u + 2);
        }
        if has(DM_SCALE, u + 10) && r16(u + 8) > 0 {
            self.scale = r16(u + 8);
        }
        if has(DM_COPIES, u + 12) && r16(u + 10) > 0 {
            self.copies = r16(u + 10).min(999);
        }
        if has(DM_PRINTQUALITY, u + 16) && r16(u + 14) != 0 {
            self.quality = r16(u + 14);
        }
        if has(DM_COLOR, u + 18) && matches!(r16(u + 16), 1 | 2) {
            self.color = r16(u + 16);
        }
        if has(DM_DUPLEX, u + 20) && matches!(r16(u + 18), 1..=3) {
            self.duplex = r16(u + 18);
        }
        if has(DM_COLLATE, u + 26) && matches!(r16(u + 24), 0 | 1) {
            self.collate = r16(u + 24);
        }
        if fields & DM_FORMNAME != 0 && fields & DM_PAPERSIZE == 0 && size >= u + 26 + 32 * cs {
            let form = read_fixed_str(ptr.add(u + 26), 32, wide);
            if let Some(p) = PAPERS.iter().find(|p| p.name.eq_ignore_ascii_case(&form)) {
                self.paper = p.id;
            }
        }
    }
}

fn put_fixed_str(dst: &mut [u8], s: &str, wide: bool) {
    if wide {
        for (i, u) in s.encode_utf16().take(dst.len() / 2 - 1).enumerate() {
            dst[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
    } else {
        let a = crate::dlls::encode_ansi(s);
        let n = a.len().min(dst.len() - 1);
        dst[..n].copy_from_slice(&a[..n]);
    }
}

unsafe fn read_fixed_str(p: *const u8, chars: usize, wide: bool) -> String {
    if wide {
        let s = std::slice::from_raw_parts(p as *const u16, chars);
        let n = s.iter().position(|&c| c == 0).unwrap_or(chars);
        String::from_utf16_lossy(&s[..n])
    } else {
        let s = std::slice::from_raw_parts(p, chars);
        let n = s.iter().position(|&c| c == 0).unwrap_or(chars);
        crate::dlls::decode_ansi(&s[..n])
    }
}

unsafe fn guest_str(p: *const c_void, wide: bool) -> Option<String> {
    if p.is_null() {
        None
    } else if wide {
        Some(crate::dlls::wstr(p as *const u16))
    } else {
        Some(crate::dlls::cstr(p as *const u8))
    }
}

// ---------- Registre des imprimantes ----------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Cups,
}

/// Imprimante vue par le spooler Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrinterEntry {
    pub name: String,
    pub kind: Kind,
    pub info: String,
    pub location: String,
    pub model: String,
    /// URI du périphérique (affiché comme « port »).
    pub port: String,
    /// `PRINTER_STATUS_*`.
    pub status: u32,
    pub jobs: u32,
    pub color: bool,
    pub duplex: bool,
    pub papers: Vec<i16>,
    pub default_paper: i16,
    pub dpis: Vec<i32>,
    pub is_default: bool,
}

impl PrinterEntry {
    fn pdf() -> PrinterEntry {
        PrinterEntry {
            name: PDF_PRINTER.into(),
            kind: Kind::Pdf,
            info: "Enregistre le document au format PDF".into(),
            location: "Documents".into(),
            model: "Winlux PDF".into(),
            port: "PDF:".into(),
            status: 0,
            jobs: 0,
            color: true,
            duplex: false,
            papers: PAPERS.iter().map(|p| p.id).collect(),
            default_paper: 9,
            dpis: vec![300, 600],
            is_default: false,
        }
    }

    fn from_cups(p: &CupsPrinter) -> PrinterEntry {
        let mut papers: Vec<i16> = PAPERS
            .iter()
            .filter(|paper| p.media.is_empty() || p.media.iter().any(|m| m == paper.ipp))
            .map(|paper| paper.id)
            .collect();
        if papers.is_empty() {
            papers = vec![9, 1];
        }
        let default_paper = PAPERS
            .iter()
            .find(|paper| paper.ipp == p.media_default)
            .map(|paper| paper.id)
            .unwrap_or(papers[0]);
        let mut status = match p.state {
            4 => 0x400, // PRINTER_STATUS_PRINTING
            5 => 0x1,   // PRINTER_STATUS_PAUSED
            _ => 0,
        };
        if !p.accepting {
            status |= 0x80; // PRINTER_STATUS_OFFLINE
        }
        let mut dpis: Vec<i32> = p.resolutions.iter().map(|r| r.0).collect();
        if dpis.is_empty() {
            dpis = vec![300, 600];
        }
        PrinterEntry {
            name: p.name.clone(),
            kind: Kind::Cups,
            info: if p.info.is_empty() {
                p.name.clone()
            } else {
                p.info.clone()
            },
            location: p.location.clone(),
            model: p.make_model.clone(),
            port: p.device_uri.clone(),
            status,
            jobs: p.queued_jobs.max(0) as u32,
            color: p.color,
            duplex: p.duplex,
            papers,
            default_paper,
            dpis,
            is_default: false,
        }
    }

    /// Marges physiques (pixels) : aucune en PDF, 1/6 de pouce sur papier.
    pub fn margin(&self, dpi: i32) -> i32 {
        match self.kind {
            Kind::Pdf => 0,
            Kind::Cups => dpi / 6,
        }
    }

    pub fn status_text(&self) -> &'static str {
        if self.status & 0x80 != 0 {
            "Hors connexion"
        } else if self.status & 0x1 != 0 {
            "En pause"
        } else if self.status & 0x400 != 0 {
            "Impression en cours"
        } else {
            "Prête"
        }
    }
}

/// Serveur CUPS imposé (tests) ; `Some(None)` désactive CUPS.
static CUPS_OVERRIDE: Mutex<Option<Option<ipp::Server>>> = Mutex::new(None);

fn client() -> Option<Client> {
    if let Some(forced) = lock(&CUPS_OVERRIDE).clone() {
        return forced.map(Client::new);
    }
    if std::env::var_os("WINLUX_NO_CUPS").is_some() {
        return None;
    }
    Some(Client::detect())
}

/// Sérialise les tests qui changent le serveur CUPS imposé.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn set_cups_server(server: Option<ipp::Server>) {
    *lock(&CUPS_OVERRIDE) = Some(server);
    invalidate_cache();
}

struct Cache {
    at: Instant,
    printers: Vec<PrinterEntry>,
    cups_error: Option<String>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

fn invalidate_cache() {
    *lock(&CACHE) = None;
}

fn set_message(msg: impl Into<String>) {
    *lock(&LAST_ERROR) = msg.into();
}

/// Dernier message du spooler (affiché par Paramètres).
pub fn last_message() -> String {
    lock(&LAST_ERROR).clone()
}

fn config_path() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(d).join("winlux/printers.conf"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/winlux/printers.conf"))
}

fn stored_default() -> Option<String> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("default="))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn store_default(name: &str) -> bool {
    let Some(path) = config_path() else {
        return false;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, format!("default={name}\n")).is_ok()
}

/// Liste des imprimantes (PDF + files CUPS), mise en cache deux secondes pour
/// que le couple « taille puis remplissage » d'`EnumPrinters` reste cohérent.
pub fn printers() -> Vec<PrinterEntry> {
    let mut cache = lock(&CACHE);
    if let Some(c) = cache.as_ref() {
        if c.at.elapsed() < Duration::from_secs(2) {
            return c.printers.clone();
        }
    }
    let mut list = vec![PrinterEntry::pdf()];
    let mut cups_default = None;
    let mut cups_error = None;
    if let Some(c) = client() {
        match c.printers() {
            Ok(found) => {
                list.extend(found.iter().map(PrinterEntry::from_cups));
                cups_default = c.default_printer().ok().flatten();
            }
            Err(e) => cups_error = Some(e.to_string()),
        }
    }
    let wanted = stored_default()
        .filter(|d| list.iter().any(|p| &p.name == d))
        .or(cups_default.filter(|d| list.iter().any(|p| &p.name == d)))
        .or_else(|| list.get(1).or(list.first()).map(|p| p.name.clone()));
    for p in &mut list {
        p.is_default = Some(&p.name) == wanted.as_ref();
    }
    *cache = Some(Cache {
        at: Instant::now(),
        printers: list.clone(),
        cups_error,
    });
    list
}

/// Erreur CUPS du dernier inventaire (service absent…).
pub fn cups_status() -> Option<String> {
    printers();
    lock(&CACHE).as_ref().and_then(|c| c.cups_error.clone())
}

pub fn find_printer(name: &str) -> Option<PrinterEntry> {
    printers()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

pub fn default_printer() -> Option<PrinterEntry> {
    printers().into_iter().find(|p| p.is_default)
}

pub fn set_default(name: &str) -> bool {
    let Some(p) = find_printer(name) else {
        return false;
    };
    let ok = store_default(&p.name);
    if p.kind == Kind::Cups {
        // Défaut système si l'utilisateur est administrateur d'impression.
        if let Some(c) = client() {
            let _ = c.set_default(&p.name);
        }
    }
    invalidate_cache();
    ok
}

/// Ajoute une file CUPS vers `uri` (réseau `ipp://…`, USB `usb://…`).
pub fn add_printer(name: &str, uri: &str, info: &str, location: &str) -> Result<(), String> {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.len() > 127 {
        return Err("nom d'imprimante invalide".into());
    }
    if uri.is_empty() || !uri.contains(':') {
        return Err("adresse d'imprimante invalide".into());
    }
    let c = client().ok_or("CUPS est désactivé")?;
    let result = c.add_printer(&name, uri, info, location);
    invalidate_cache();
    result.map_err(|e| e.to_string())
}

pub fn delete_printer(name: &str) -> Result<(), String> {
    if name == PDF_PRINTER {
        return Err("l'imprimante PDF intégrée ne peut pas être supprimée".into());
    }
    let c = client().ok_or("CUPS est désactivé")?;
    let r = c.delete_printer(name).map_err(|e| e.to_string());
    invalidate_cache();
    r
}

/// Imprimantes détectées (USB et réseau) non encore installées.
pub fn discover() -> Result<Vec<ipp::Device>, String> {
    let c = client().ok_or("CUPS est désactivé")?;
    let installed: Vec<String> = printers().into_iter().map(|p| p.port).collect();
    c.devices(5)
        .map(|d| {
            d.into_iter()
                .filter(|dev| !installed.contains(&dev.uri))
                .collect()
        })
        .map_err(|e| e.to_string())
}

/// Travail en file d'attente.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: u32,
    pub printer: String,
    pub document: String,
    pub user: String,
    /// `JOB_STATUS_*`.
    pub status: u32,
    pub pages_printed: u32,
    pub size: u32,
    pub submitted: i64,
}

pub fn jobs(printer: &str) -> Vec<Job> {
    let Some(p) = find_printer(printer) else {
        return Vec::new();
    };
    if p.kind == Kind::Pdf {
        return Vec::new();
    }
    let Some(c) = client() else {
        return Vec::new();
    };
    c.jobs(Some(&p.name))
        .unwrap_or_default()
        .into_iter()
        .map(|j| Job {
            id: j.id as u32,
            printer: p.name.clone(),
            document: j.name,
            user: j.user,
            status: match j.state {
                4 | 6 => 0x1, // retenu/arrêté → en pause
                5 => 0x10,    // JOB_STATUS_PRINTING
                7 | 8 => 0x4, // annulation
                9 => 0x80,    // imprimé
                _ => 0,
            },
            pages_printed: j.pages_done.max(0) as u32,
            size: (j.size_k.max(0) as u32).saturating_mul(1024),
            submitted: j.created,
        })
        .collect()
}

pub fn cancel_job(printer: &str, id: u32) -> bool {
    let Some(c) = client() else {
        return false;
    };
    let ok = c.cancel_job(printer, id as i32).is_ok();
    invalidate_cache();
    ok
}

// ---------- Envoi des documents ----------

static NEXT_LOCAL_JOB: AtomicUsize = AtomicUsize::new(1);

fn output_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("WINLUX_PRINT_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let docs = home.join("Documents");
    if docs.is_dir() {
        docs
    } else {
        home
    }
}

/// Fichier de sortie libre pour `title` (`Rapport.pdf`, `Rapport (2).pdf`…).
fn unique_output(title: &str, ext: &str) -> PathBuf {
    let clean: String = title
        .chars()
        .map(|c| if "/\\:*?\"<>|\0".contains(c) { '_' } else { c })
        .collect();
    let clean = clean.trim().trim_matches('.');
    let base = if clean.is_empty() { "Document" } else { clean };
    let base: String = base.chars().take(120).collect();
    let dir = output_dir();
    let _ = std::fs::create_dir_all(&dir);
    let mut path = dir.join(format!("{base}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base} ({n}).{ext}"));
        n += 1;
    }
    path
}

/// Type MIME reconnu d'un flux brut (`WritePrinter`).
fn sniff_format(data: &[u8]) -> &'static str {
    if data.starts_with(b"%PDF") {
        "application/pdf"
    } else if data.starts_with(b"%!") {
        "application/postscript"
    } else {
        "application/vnd.cups-raw"
    }
}

/// Envoie un document : fichier demandé, PDF virtuel ou travail CUPS.
/// Renvoie l'identifiant du travail et le chemin écrit le cas échéant.
pub fn submit(
    printer: &PrinterEntry,
    title: &str,
    output: Option<&str>,
    devmode: &DevMode,
    data: Vec<u8>,
) -> Result<(u32, Option<PathBuf>), String> {
    let format = sniff_format(&data);
    let target = match output {
        Some(o) if !o.is_empty() && !o.eq_ignore_ascii_case("FILE:") => {
            Some(crate::paths::to_linux(o))
        }
        _ if printer.kind == Kind::Pdf => Some(unique_output(
            title,
            if format == "application/pdf" {
                "pdf"
            } else {
                "prn"
            },
        )),
        _ => None,
    };
    if let Some(path) = target {
        std::fs::write(&path, &data)
            .map_err(|e| format!("écriture de {} impossible : {e}", path.display()))?;
        let id = NEXT_LOCAL_JOB.fetch_add(1, Ordering::Relaxed) as u32;
        set_message(format!("Document enregistré : {}", path.display()));
        crate::log_info!(
            "winspool",
            "winlux: impression « {title} » enregistrée dans {}",
            path.display()
        );
        return Ok((id, Some(path)));
    }
    let c = client().ok_or("CUPS est désactivé")?;
    let id = c
        .print_job(&printer.name, title, format, &devmode.job_options(), data)
        .map_err(|e| e.to_string())?;
    invalidate_cache();
    set_message(format!("Travail {id} envoyé à {}", printer.name));
    crate::log_info!(
        "winspool",
        "winlux: travail CUPS {id} « {title} » envoyé à {}",
        printer.name
    );
    Ok((id as u32, None))
}

// ---------- Handles d'imprimante (OpenPrinter) ----------

struct RawDoc {
    title: String,
    output: Option<String>,
    data: Vec<u8>,
    job: u32,
}

struct PrinterHandle {
    /// `None` : handle du serveur d'impression local.
    printer: Option<PrinterEntry>,
    devmode: Option<DevMode>,
    doc: Option<RawDoc>,
}

fn handles() -> &'static Mutex<HashMap<usize, PrinterHandle>> {
    static H: OnceLock<Mutex<HashMap<usize, PrinterHandle>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x0051_0000);

fn new_handle(printer: Option<PrinterEntry>, devmode: Option<DevMode>) -> usize {
    let h = NEXT_HANDLE.fetch_add(4, Ordering::Relaxed);
    lock(handles()).insert(
        h,
        PrinterHandle {
            printer,
            devmode,
            doc: None,
        },
    );
    h
}

fn handle_printer(h: usize) -> Option<PrinterEntry> {
    lock(handles()).get(&h).and_then(|p| p.printer.clone())
}

fn handle_devmode(h: usize) -> Option<DevMode> {
    let map = lock(handles());
    let ph = map.get(&h)?;
    ph.devmode
        .clone()
        .or_else(|| ph.printer.as_ref().map(DevMode::for_printer))
}

unsafe fn open_printer(
    name: Option<String>,
    out: *mut usize,
    defaults: *const u8,
    wide: bool,
) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let printer = match name.as_deref() {
        None | Some("") => None,
        // « Nom,Job 12 » et « Nom,XcvPort » : on garde le nom d'imprimante.
        Some(n) => {
            let n = n.split(',').next().unwrap_or(n).trim();
            match find_printer(n) {
                Some(p) => Some(p),
                None => {
                    crate::process::set_last_error(ERROR_INVALID_PRINTER_NAME);
                    return 0;
                }
            }
        }
    };
    // PRINTER_DEFAULTS { pDatatype, pDevMode, DesiredAccess }
    let devmode = if defaults.is_null() {
        None
    } else {
        let dm = std::ptr::read_unaligned(defaults.add(8) as *const *const u8);
        printer.as_ref().map(|p| {
            let mut d = DevMode::for_printer(p);
            d.merge(dm, wide);
            d
        })
    };
    let h = new_handle(printer, devmode);
    std::ptr::write_unaligned(out, h);
    1
}

pub unsafe extern "win64" fn open_printer_a(
    name: *const u8,
    out: *mut usize,
    defaults: *const u8,
) -> i32 {
    open_printer(guest_str(name.cast(), false), out, defaults, false)
}

pub unsafe extern "win64" fn open_printer_w(
    name: *const u16,
    out: *mut usize,
    defaults: *const u8,
) -> i32 {
    open_printer(guest_str(name.cast(), true), out, defaults, true)
}

pub unsafe extern "win64" fn open_printer2_w(
    name: *const u16,
    out: *mut usize,
    defaults: *const u8,
    _options: *const c_void,
) -> i32 {
    open_printer_w(name, out, defaults)
}

pub extern "win64" fn close_printer(h: usize) -> i32 {
    match lock(handles()).remove(&h) {
        Some(_) => 1,
        None => {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            0
        }
    }
}

unsafe fn start_doc_printer(h: usize, level: u32, info: *const u8, wide: bool) -> u32 {
    if level != 1 && level != 2 {
        crate::process::set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    if info.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let read = |off: usize| std::ptr::read_unaligned(info.add(off) as *const *const c_void);
    let title = guest_str(read(0), wide).unwrap_or_else(|| "Document".into());
    let output = guest_str(read(8), wide);
    let mut map = lock(handles());
    let Some(ph) = map.get_mut(&h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if ph.printer.is_none() {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    let job = NEXT_LOCAL_JOB.fetch_add(1, Ordering::Relaxed) as u32;
    ph.doc = Some(RawDoc {
        title,
        output,
        data: Vec::new(),
        job,
    });
    job
}

pub unsafe extern "win64" fn start_doc_printer_a(h: usize, level: u32, info: *const u8) -> u32 {
    start_doc_printer(h, level, info, false)
}

pub unsafe extern "win64" fn start_doc_printer_w(h: usize, level: u32, info: *const u8) -> u32 {
    start_doc_printer(h, level, info, true)
}

fn with_raw_doc(h: usize, f: impl FnOnce(&mut RawDoc)) -> i32 {
    let mut map = lock(handles());
    match map.get_mut(&h) {
        None => {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            0
        }
        Some(PrinterHandle { doc: None, .. }) => {
            crate::process::set_last_error(ERROR_SPL_NO_STARTDOC);
            0
        }
        Some(PrinterHandle { doc: Some(d), .. }) => {
            f(d);
            1
        }
    }
}

pub extern "win64" fn start_page_printer(h: usize) -> i32 {
    with_raw_doc(h, |_| {})
}

pub extern "win64" fn end_page_printer(h: usize) -> i32 {
    with_raw_doc(h, |_| {})
}

pub unsafe extern "win64" fn write_printer(
    h: usize,
    buf: *const u8,
    len: u32,
    written: *mut u32,
) -> i32 {
    if buf.is_null() && len > 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let data = if len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(buf, len as usize)
    };
    let ok = with_raw_doc(h, |d| d.data.extend_from_slice(data));
    if ok != 0 && !written.is_null() {
        std::ptr::write_unaligned(written, len);
    }
    ok
}

pub extern "win64" fn end_doc_printer(h: usize) -> i32 {
    let (doc, printer, devmode) = {
        let mut map = lock(handles());
        let Some(ph) = map.get_mut(&h) else {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            return 0;
        };
        let Some(doc) = ph.doc.take() else {
            crate::process::set_last_error(ERROR_SPL_NO_STARTDOC);
            return 0;
        };
        let Some(printer) = ph.printer.clone() else {
            return 0;
        };
        let devmode = ph
            .devmode
            .clone()
            .unwrap_or_else(|| DevMode::for_printer(&printer));
        (doc, printer, devmode)
    };
    let _ = doc.job;
    match submit(
        &printer,
        &doc.title,
        doc.output.as_deref(),
        &devmode,
        doc.data,
    ) {
        Ok(_) => 1,
        Err(e) => {
            set_message(e);
            crate::process::set_last_error(ERROR_NOT_READY);
            0
        }
    }
}

pub extern "win64" fn abort_printer(h: usize) -> i32 {
    let mut map = lock(handles());
    match map.get_mut(&h) {
        Some(ph) => {
            ph.doc = None;
            1
        }
        None => {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            0
        }
    }
}

// ---------- Construction des structures *_INFO ----------

/// Tampon de structures Win32 : partie fixe puis chaînes et DEVMODE, avec
/// les pointeurs internes corrigés à l'écriture.
struct Packer {
    wide: bool,
    fixed: Vec<u8>,
    tail: Vec<u8>,
    /// (position du pointeur dans `fixed`, position de la cible dans `tail`).
    fixups: Vec<(usize, usize)>,
}

impl Packer {
    fn new(wide: bool) -> Packer {
        Packer {
            wide,
            fixed: Vec::new(),
            tail: Vec::new(),
            fixups: Vec::new(),
        }
    }

    fn u32(&mut self, v: u32) {
        self.fixed.extend_from_slice(&v.to_le_bytes());
    }

    fn align(&mut self) {
        while !self.fixed.len().is_multiple_of(8) {
            self.fixed.push(0);
        }
    }

    fn null(&mut self) {
        self.align();
        self.fixed.extend_from_slice(&0usize.to_le_bytes());
    }

    fn blob(&mut self, bytes: &[u8]) {
        self.align();
        while !self.tail.len().is_multiple_of(8) {
            self.tail.push(0);
        }
        self.fixups.push((self.fixed.len(), self.tail.len()));
        self.fixed.extend_from_slice(&0usize.to_le_bytes());
        self.tail.extend_from_slice(bytes);
    }

    fn str(&mut self, s: &str) {
        let mut bytes: Vec<u8> = if self.wide {
            s.encode_utf16().flat_map(u16::to_le_bytes).collect()
        } else {
            crate::dlls::encode_ansi(s)
        };
        bytes.extend_from_slice(if self.wide { &[0, 0] } else { &[0] });
        self.blob(&bytes);
    }

    fn size(&self) -> usize {
        let fixed = (self.fixed.len() + 7) & !7;
        fixed + self.tail.len()
    }

    /// Écrit dans `buf` (taille vérifiée par l'appelant).
    unsafe fn write(&self, buf: *mut u8) {
        let fixed = (self.fixed.len() + 7) & !7;
        std::ptr::write_bytes(buf, 0, fixed);
        std::ptr::copy_nonoverlapping(self.fixed.as_ptr(), buf, self.fixed.len());
        std::ptr::copy_nonoverlapping(self.tail.as_ptr(), buf.add(fixed), self.tail.len());
        for (at, target) in &self.fixups {
            let p = buf as usize + fixed + target;
            std::ptr::write_unaligned(buf.add(*at) as *mut usize, p);
        }
    }
}

fn printer_attributes(p: &PrinterEntry) -> u32 {
    let mut a = 0x1 | 0x40 | 0x800; // QUEUED | LOCAL | ENABLE_BIDI
    if p.is_default {
        a |= 0x4;
    }
    if p.port.starts_with("ipp") || p.port.starts_with("dnssd") || p.port.starts_with("socket") {
        a |= 0x10; // NETWORK
    }
    a
}

/// Ajoute un `PRINTER_INFO_<level>` au tampon ; `false` si niveau inconnu.
fn pack_printer(pk: &mut Packer, level: u32, p: &PrinterEntry, devmode: &DevMode) -> bool {
    match level {
        1 => {
            pk.u32(0x0080_0000); // PRINTER_ENUM_ICON8
            pk.str(&format!("{},{},{}", p.name, p.model, p.location));
            pk.str(&p.name);
            pk.str(&p.info);
        }
        2 => {
            pk.null(); // pServerName (local)
            pk.str(&p.name);
            pk.str(&p.name); // pShareName
            pk.str(&p.port);
            pk.str(&p.model);
            pk.str(&p.info);
            pk.str(&p.location);
            let dm = devmode.to_bytes(pk.wide);
            pk.blob(&dm);
            pk.str(""); // pSepFile
            pk.str("winprint");
            pk.str("RAW");
            pk.str("");
            pk.null(); // pSecurityDescriptor
            pk.u32(printer_attributes(p));
            pk.u32(1); // Priority
            pk.u32(1); // DefaultPriority
            pk.u32(0); // StartTime
            pk.u32(0); // UntilTime
            pk.u32(p.status);
            pk.u32(p.jobs);
            pk.u32(0); // AveragePPM
        }
        4 => {
            pk.str(&p.name);
            pk.null();
            pk.u32(printer_attributes(p));
            pk.align();
        }
        5 => {
            pk.str(&p.name);
            pk.str(&p.port);
            pk.u32(printer_attributes(p));
            pk.u32(15000);
            pk.u32(45000);
            pk.align();
        }
        6 => pk.u32(p.status),
        8 | 9 => {
            let dm = devmode.to_bytes(pk.wide);
            pk.blob(&dm);
        }
        _ => return false,
    }
    true
}

/// Remplit un tampon invité ; renvoie `TRUE` ou `FALSE` +
/// `ERROR_INSUFFICIENT_BUFFER`, avec la taille requise dans `needed`.
unsafe fn deliver(pk: &Packer, buf: *mut u8, cb: u32, needed: *mut u32) -> i32 {
    let size = pk.size() as u32;
    if !needed.is_null() {
        std::ptr::write_unaligned(needed, size);
    }
    if buf.is_null() || cb < size {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    pk.write(buf);
    1
}

#[allow(clippy::too_many_arguments)]
unsafe fn enum_printers(
    flags: u32,
    _name: *const c_void,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
    wide: bool,
) -> i32 {
    if !returned.is_null() {
        std::ptr::write_unaligned(returned, 0);
    }
    let mut list = printers();
    if flags & 0x1 != 0 && flags & 0x6 == 0 {
        list.retain(|p| p.is_default); // PRINTER_ENUM_DEFAULT
    }
    let mut pk = Packer::new(wide);
    for p in &list {
        if !pack_printer(&mut pk, level, p, &DevMode::for_printer(p)) {
            crate::process::set_last_error(ERROR_INVALID_LEVEL);
            return 0;
        }
        if matches!(level, 6) {
            // PRINTER_INFO_6 : DWORD seul, pas d'alignement 8.
            continue;
        }
        pk.align();
    }
    if deliver(&pk, buf, cb, needed) == 0 {
        return 0;
    }
    if !returned.is_null() {
        std::ptr::write_unaligned(returned, list.len() as u32);
    }
    1
}

pub unsafe extern "win64" fn enum_printers_a(
    flags: u32,
    name: *const u8,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    enum_printers(flags, name.cast(), level, buf, cb, needed, returned, false)
}

pub unsafe extern "win64" fn enum_printers_w(
    flags: u32,
    name: *const u16,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    enum_printers(flags, name.cast(), level, buf, cb, needed, returned, true)
}

unsafe fn get_printer(
    h: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    wide: bool,
) -> i32 {
    let Some(p) = handle_printer(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    // État à jour (le cache peut dater de l'ouverture du handle).
    let p = find_printer(&p.name).unwrap_or(p);
    let dm = handle_devmode(h).unwrap_or_else(|| DevMode::for_printer(&p));
    let mut pk = Packer::new(wide);
    if !pack_printer(&mut pk, level, &p, &dm) {
        crate::process::set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    deliver(&pk, buf, cb, needed)
}

pub unsafe extern "win64" fn get_printer_a(
    h: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    get_printer(h, level, buf, cb, needed, false)
}

pub unsafe extern "win64" fn get_printer_w(
    h: usize,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    get_printer(h, level, buf, cb, needed, true)
}

/// `SetPrinter` : niveau 0 avec commande (pause, reprise, purge).
pub unsafe extern "win64" fn set_printer(
    h: usize,
    level: u32,
    _info: *const u8,
    command: u32,
) -> i32 {
    let Some(p) = handle_printer(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if level != 0 {
        crate::process::set_last_error(ERROR_NOT_SUPPORTED);
        return 0;
    }
    if p.kind == Kind::Pdf {
        return 1;
    }
    let Some(c) = client() else {
        crate::process::set_last_error(ERROR_NOT_READY);
        return 0;
    };
    let result = match command {
        1 => c.pause_printer(&p.name),
        2 => c.resume_printer(&p.name),
        3 => {
            for j in jobs(&p.name) {
                let _ = c.cancel_job(&p.name, j.id as i32);
            }
            Ok(())
        }
        _ => Ok(()),
    };
    invalidate_cache();
    match result {
        Ok(()) => 1,
        Err(e) => {
            set_message(e.to_string());
            crate::process::set_last_error(ERROR_ACCESS_DENIED);
            0
        }
    }
}

unsafe fn get_default_printer(buf: *mut c_void, size: *mut u32, wide: bool) -> i32 {
    if size.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(p) = default_printer() else {
        crate::process::set_last_error(ERROR_FILE_NOT_FOUND);
        return 0;
    };
    let cap = std::ptr::read_unaligned(size);
    let n = if wide {
        crate::dlls::put_w(&p.name, buf.cast(), cap)
    } else {
        let a = crate::dlls::encode_ansi(&p.name);
        if buf.is_null() || (cap as usize) <= a.len() {
            a.len() as u32 + 1
        } else {
            std::ptr::copy_nonoverlapping(a.as_ptr(), buf as *mut u8, a.len());
            *(buf as *mut u8).add(a.len()) = 0;
            a.len() as u32
        }
    };
    if buf.is_null() || n >= cap {
        std::ptr::write_unaligned(size, n.max(1));
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::write_unaligned(size, n + 1);
    1
}

pub unsafe extern "win64" fn get_default_printer_a(buf: *mut u8, size: *mut u32) -> i32 {
    get_default_printer(buf.cast(), size, false)
}

pub unsafe extern "win64" fn get_default_printer_w(buf: *mut u16, size: *mut u32) -> i32 {
    get_default_printer(buf.cast(), size, true)
}

pub unsafe extern "win64" fn set_default_printer_a(name: *const u8) -> i32 {
    let name = guest_str(name.cast(), false).unwrap_or_default();
    set_default_common(&name)
}

pub unsafe extern "win64" fn set_default_printer_w(name: *const u16) -> i32 {
    let name = guest_str(name.cast(), true).unwrap_or_default();
    set_default_common(&name)
}

fn set_default_common(name: &str) -> i32 {
    // Nom vide : Windows choisit lui-même une imprimante par défaut.
    let name = if name.is_empty() {
        match printers()
            .into_iter()
            .nth(1)
            .or_else(|| printers().into_iter().next())
        {
            Some(p) => p.name,
            None => return 0,
        }
    } else {
        name.split(',').next().unwrap_or(name).to_string()
    };
    if set_default(&name) {
        1
    } else {
        crate::process::set_last_error(ERROR_INVALID_PRINTER_NAME);
        0
    }
}

/// `AddPrinter(serveur, 2, PRINTER_INFO_2)` : crée une file CUPS.
unsafe fn add_printer_common(level: u32, info: *const u8, wide: bool) -> usize {
    if level != 2 || info.is_null() {
        crate::process::set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    let s = |off: usize| {
        guest_str(
            std::ptr::read_unaligned(info.add(off) as *const *const c_void),
            wide,
        )
        .unwrap_or_default()
    };
    let (name, port, comment, location) = (s(8), s(24), s(40), s(48));
    if find_printer(&name).is_some() {
        crate::process::set_last_error(ERROR_PRINTER_ALREADY_EXISTS);
        return 0;
    }
    match add_printer(&name, &port, &comment, &location) {
        Ok(()) => {
            let p = find_printer(&name);
            new_handle(p, None)
        }
        Err(e) => {
            set_message(e);
            crate::process::set_last_error(ERROR_ACCESS_DENIED);
            0
        }
    }
}

pub unsafe extern "win64" fn add_printer_a(
    _server: *const u8,
    level: u32,
    info: *const u8,
) -> usize {
    add_printer_common(level, info, false)
}

pub unsafe extern "win64" fn add_printer_w(
    _server: *const u16,
    level: u32,
    info: *const u8,
) -> usize {
    add_printer_common(level, info, true)
}

pub extern "win64" fn delete_printer_api(h: usize) -> i32 {
    let Some(p) = handle_printer(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    match delete_printer(&p.name) {
        Ok(()) => 1,
        Err(e) => {
            set_message(e);
            crate::process::set_last_error(ERROR_ACCESS_DENIED);
            0
        }
    }
}

fn unix_to_systemtime(t: i64) -> [u16; 8] {
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);
    // Algorithme civil (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let dow = (days + 4).rem_euclid(7);
    [
        y as u16,
        m as u16,
        dow as u16,
        d as u16,
        (secs / 3600) as u16,
        (secs / 60 % 60) as u16,
        (secs % 60) as u16,
        0,
    ]
}

fn pack_job(pk: &mut Packer, j: &Job, position: u32) {
    pk.u32(j.id);
    pk.str(&j.printer);
    pk.str("localhost");
    pk.str(&j.user);
    pk.str(&j.document);
    pk.str("RAW");
    pk.str(match j.status {
        0x10 => "Impression",
        0x1 => "En pause",
        0x4 => "Suppression",
        _ => "",
    });
    pk.u32(j.status);
    pk.u32(1); // Priority
    pk.u32(position);
    pk.u32(0); // TotalPages
    pk.u32(j.pages_printed);
    for v in unix_to_systemtime(j.submitted) {
        pk.fixed.extend_from_slice(&v.to_le_bytes());
    }
    pk.align();
}

#[allow(clippy::too_many_arguments)]
unsafe fn enum_jobs(
    h: usize,
    first: u32,
    count: u32,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
    wide: bool,
) -> i32 {
    if !returned.is_null() {
        std::ptr::write_unaligned(returned, 0);
    }
    let Some(p) = handle_printer(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if level != 1 {
        crate::process::set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    let list: Vec<Job> = jobs(&p.name)
        .into_iter()
        .skip(first as usize)
        .take(count as usize)
        .collect();
    let mut pk = Packer::new(wide);
    for (i, j) in list.iter().enumerate() {
        pack_job(&mut pk, j, first + i as u32 + 1);
    }
    if deliver(&pk, buf, cb, needed) == 0 {
        return 0;
    }
    if !returned.is_null() {
        std::ptr::write_unaligned(returned, list.len() as u32);
    }
    1
}

pub unsafe extern "win64" fn enum_jobs_a(
    h: usize,
    first: u32,
    count: u32,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    enum_jobs(h, first, count, level, buf, cb, needed, returned, false)
}

pub unsafe extern "win64" fn enum_jobs_w(
    h: usize,
    first: u32,
    count: u32,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    returned: *mut u32,
) -> i32 {
    enum_jobs(h, first, count, level, buf, cb, needed, returned, true)
}

unsafe fn get_job(
    h: usize,
    id: u32,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
    wide: bool,
) -> i32 {
    let Some(p) = handle_printer(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if level != 1 {
        crate::process::set_last_error(ERROR_INVALID_LEVEL);
        return 0;
    }
    let list = jobs(&p.name);
    let Some((pos, j)) = list.iter().enumerate().find(|(_, j)| j.id == id) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let mut pk = Packer::new(wide);
    pack_job(&mut pk, j, pos as u32 + 1);
    deliver(&pk, buf, cb, needed)
}

pub unsafe extern "win64" fn get_job_a(
    h: usize,
    id: u32,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    get_job(h, id, level, buf, cb, needed, false)
}

pub unsafe extern "win64" fn get_job_w(
    h: usize,
    id: u32,
    level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    get_job(h, id, level, buf, cb, needed, true)
}

/// `SetJob` : seules les commandes (annuler, supprimer, pause, reprise) sont gérées.
pub unsafe extern "win64" fn set_job(
    h: usize,
    id: u32,
    _level: u32,
    _info: *const u8,
    command: u32,
) -> i32 {
    let Some(p) = handle_printer(h) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    match command {
        0 => 1,
        3 | 5 | 7 => {
            if cancel_job(&p.name, id) {
                1
            } else {
                crate::process::set_last_error(ERROR_INVALID_PARAMETER);
                0
            }
        }
        _ => {
            crate::process::set_last_error(ERROR_NOT_SUPPORTED);
            0
        }
    }
}

// ---------- DeviceCapabilities / DocumentProperties ----------

unsafe fn device_capabilities(
    device: Option<String>,
    capability: u16,
    out: *mut u8,
    devmode: *const u8,
    wide: bool,
) -> i32 {
    let Some(p) = device.as_deref().and_then(find_printer) else {
        crate::process::set_last_error(ERROR_INVALID_PRINTER_NAME);
        return -1;
    };
    let mut dm = DevMode::for_printer(&p);
    dm.merge(devmode, wide);
    let cs = if wide { 2 } else { 1 };
    let put_strings = |out: *mut u8, items: &[&str], chars: usize| {
        if !out.is_null() {
            for (i, s) in items.iter().enumerate() {
                let dst = std::slice::from_raw_parts_mut(out.add(i * chars * cs), chars * cs);
                dst.fill(0);
                put_fixed_str(dst, s, wide);
            }
        }
        items.len() as i32
    };
    let papers: Vec<Paper> = p.papers.iter().map(|&id| paper(id)).collect();
    match capability {
        1 => (DevMode::FIELDS) as i32, // DC_FIELDS
        2 => {
            // DC_PAPERS : tableau de WORD
            if !out.is_null() {
                for (i, pp) in papers.iter().enumerate() {
                    std::ptr::write_unaligned(out.add(i * 2) as *mut i16, pp.id);
                }
            }
            papers.len() as i32
        }
        3 => {
            // DC_PAPERSIZE : POINT en 1/10 mm
            if !out.is_null() {
                for (i, pp) in papers.iter().enumerate() {
                    std::ptr::write_unaligned(out.add(i * 8) as *mut i32, pp.width);
                    std::ptr::write_unaligned(out.add(i * 8 + 4) as *mut i32, pp.height);
                }
            }
            papers.len() as i32
        }
        4 | 5 => {
            // DC_MINEXTENT / DC_MAXEXTENT : POINT compacté dans la valeur de retour
            let (w, h) = if capability == 4 {
                (1000, 1480)
            } else {
                (2970, 4200)
            };
            (h << 16) | w
        }
        6 => {
            // DC_BINS
            if !out.is_null() {
                std::ptr::write_unaligned(out as *mut i16, 7);
            }
            1
        }
        7 => i32::from(p.duplex), // DC_DUPLEX
        8 => (if wide { DEVMODEW_SIZE } else { DEVMODEA_SIZE }) as i32, // DC_SIZE
        9 => 0,                   // DC_EXTRA
        10 => 0x0401,             // DC_VERSION
        11 => 0x0100,             // DC_DRIVER
        12 => put_strings(out, &["Sélection automatique"], 24), // DC_BINNAMES
        13 => {
            // DC_ENUMRESOLUTIONS : paires de LONG
            if !out.is_null() {
                for (i, d) in p.dpis.iter().enumerate() {
                    std::ptr::write_unaligned(out.add(i * 8) as *mut i32, *d);
                    std::ptr::write_unaligned(out.add(i * 8 + 4) as *mut i32, *d);
                }
            }
            p.dpis.len() as i32
        }
        15 => 0x1, // DC_TRUETYPE : DCTT_BITMAP
        16 => {
            let names: Vec<&str> = papers.iter().map(|pp| pp.name).collect();
            put_strings(out, &names, 64) // DC_PAPERNAMES
        }
        17 => 90,  // DC_ORIENTATION : paysage = rotation 90°
        18 => 999, // DC_COPIES
        20 => 1,   // DC_EMF_COMPLIANT
        22 => 1,   // DC_COLLATE
        23 | 24 => {
            let s = if capability == 23 {
                p.model.split(' ').next().unwrap_or("")
            } else {
                &p.model
            };
            if !out.is_null() {
                let len = s.encode_utf16().count() + 1;
                let dst = std::slice::from_raw_parts_mut(out, len * cs + cs);
                put_fixed_str(dst, s, wide);
            }
            s.encode_utf16().count() as i32 + 1
        }
        25 => put_strings(out, &["PDF"], 32), // DC_PERSONALITY
        29 => put_strings(out, &[paper(dm.paper).name], 64), // DC_MEDIAREADY
        32 => i32::from(p.color),             // DC_COLORDEVICE
        33 => 1,                              // DC_NUP
        _ => {
            crate::process::set_last_error(ERROR_NOT_SUPPORTED);
            -1
        }
    }
}

pub unsafe extern "win64" fn device_capabilities_a(
    device: *const u8,
    _port: *const u8,
    capability: u16,
    out: *mut u8,
    devmode: *const u8,
) -> i32 {
    device_capabilities(
        guest_str(device.cast(), false),
        capability,
        out,
        devmode,
        false,
    )
}

pub unsafe extern "win64" fn device_capabilities_w(
    device: *const u16,
    _port: *const u16,
    capability: u16,
    out: *mut u8,
    devmode: *const u8,
) -> i32 {
    device_capabilities(
        guest_str(device.cast(), true),
        capability,
        out,
        devmode,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
unsafe fn document_properties(
    h: usize,
    device: Option<String>,
    out: *mut u8,
    input: *const u8,
    mode: u32,
    wide: bool,
) -> i32 {
    let printer = device
        .as_deref()
        .filter(|d| !d.is_empty())
        .and_then(find_printer)
        .or_else(|| handle_printer(h));
    let Some(p) = printer else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return -1;
    };
    let size = (if wide { DEVMODEW_SIZE } else { DEVMODEA_SIZE }) as i32;
    if mode == 0 {
        return size;
    }
    let mut dm = handle_devmode(h)
        .filter(|d| d.device == p.name)
        .unwrap_or_else(|| DevMode::for_printer(&p));
    if mode & 8 != 0 {
        dm.merge(input, wide); // DM_IN_BUFFER
    }
    if mode & 4 != 0 {
        // DM_IN_PROMPT : propriétés réglées dans la boîte d'impression.
        if !crate::dlls::comdlg32::properties_dialog(&p, &mut dm) {
            return 2; // IDCANCEL
        }
    }
    if mode & 2 != 0 {
        if out.is_null() {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            return -1;
        }
        let bytes = dm.to_bytes(wide);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    }
    1 // IDOK
}

pub unsafe extern "win64" fn document_properties_a(
    _hwnd: usize,
    h: usize,
    device: *const u8,
    out: *mut u8,
    input: *const u8,
    mode: u32,
) -> i32 {
    document_properties(h, guest_str(device.cast(), false), out, input, mode, false)
}

pub unsafe extern "win64" fn document_properties_w(
    _hwnd: usize,
    h: usize,
    device: *const u16,
    out: *mut u8,
    input: *const u8,
    mode: u32,
) -> i32 {
    document_properties(h, guest_str(device.cast(), true), out, input, mode, true)
}

pub unsafe extern "win64" fn advanced_document_properties_w(
    hwnd: usize,
    h: usize,
    device: *const u16,
    out: *mut u8,
    input: *const u8,
) -> i32 {
    let r = document_properties_w(hwnd, h, device, out, input, 2 | 4 | 8);
    i32::from(r == 1)
}

pub unsafe extern "win64" fn advanced_document_properties_a(
    hwnd: usize,
    h: usize,
    device: *const u8,
    out: *mut u8,
    input: *const u8,
) -> i32 {
    let r = document_properties_a(hwnd, h, device, out, input, 2 | 4 | 8);
    i32::from(r == 1)
}

pub unsafe extern "win64" fn printer_properties(_hwnd: usize, h: usize) -> i32 {
    match handle_printer(h) {
        Some(p) => {
            let mut dm = handle_devmode(h).unwrap_or_else(|| DevMode::for_printer(&p));
            if crate::dlls::comdlg32::properties_dialog(&p, &mut dm) {
                if let Some(ph) = lock(handles()).get_mut(&h) {
                    ph.devmode = Some(dm);
                }
            }
            1
        }
        None => {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            0
        }
    }
}

/// `GetPrintProcessorDirectory`/`GetPrinterDriverDirectory` : dossier factice.
pub unsafe extern "win64" fn get_printer_driver_directory_w(
    _server: *const u16,
    _env: *const u16,
    _level: u32,
    buf: *mut u8,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    let s = "C:\\Windows\\System32\\spool\\drivers\\x64";
    let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let size = (w.len() * 2) as u32;
    if !needed.is_null() {
        std::ptr::write_unaligned(needed, size);
    }
    if buf.is_null() || cb < size {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, buf, size as usize);
    1
}

/// Extension Winlux pour Paramètres : imprimantes détectées, une par ligne
/// « uri\tlibellé\tusb|reseau ». Renvoie le nombre de caractères requis.
pub unsafe extern "win64" fn winlux_discover_printers_w(buf: *mut u16, cap: u32) -> u32 {
    let text = match discover() {
        Ok(list) => list
            .iter()
            .map(|d| {
                let label = if d.make_model.is_empty() {
                    &d.info
                } else {
                    &d.make_model
                };
                format!(
                    "{}\t{}\t{}\n",
                    d.uri,
                    label,
                    if d.is_usb() { "usb" } else { "reseau" }
                )
            })
            .collect::<String>(),
        Err(e) => {
            set_message(e);
            String::new()
        }
    };
    crate::dlls::put_w(&text, buf, cap)
}

/// Extension Winlux : ajoute une imprimante (`uri` réseau ou USB) sous `name`.
pub unsafe extern "win64" fn winlux_add_printer_w(name: *const u16, uri: *const u16) -> i32 {
    let name = guest_str(name.cast(), true).unwrap_or_default();
    let uri = guest_str(uri.cast(), true).unwrap_or_default();
    let uri = uri.trim();
    // Adresse saisie sans schéma : imprimante IPP Everywhere sur le réseau.
    let uri = if uri.contains("://") || uri.starts_with("usb:") {
        uri.to_string()
    } else {
        format!("ipp://{uri}/ipp/print")
    };
    let name = if name.trim().is_empty() {
        uri.split("://")
            .nth(1)
            .and_then(|r| r.split(['/', ':', '?']).next())
            .unwrap_or("Imprimante")
            .to_string()
    } else {
        name
    };
    match add_printer(&name, &uri, &name, "") {
        Ok(()) => {
            set_message(format!("Imprimante « {name} » ajoutée."));
            1
        }
        Err(e) => {
            set_message(format!("Ajout impossible : {e}"));
            0
        }
    }
}

/// Extension Winlux : dernier message du spooler (succès ou erreur).
pub unsafe extern "win64" fn winlux_print_status_w(buf: *mut u16, cap: u32) -> u32 {
    let mut msg = last_message();
    if msg.is_empty() {
        if let Some(e) = cups_status() {
            msg = format!("CUPS : {e}. Seule l'impression PDF est disponible.");
        }
    }
    crate::dlls::put_w(&msg, buf, cap)
}

/// Extension Winlux : imprime une page de test sur `name`.
pub unsafe extern "win64" fn winlux_print_test_page_w(name: *const u16) -> i32 {
    let name = guest_str(name.cast(), true).unwrap_or_default();
    let Some(p) = find_printer(&name) else {
        set_message("Imprimante introuvable.");
        return 0;
    };
    let dm = DevMode::for_printer(&p);
    let pdf = test_page(&p, &dm);
    match submit(&p, "Page de test Winlux", None, &dm, pdf) {
        Ok(_) => 1,
        Err(e) => {
            set_message(format!("Page de test non imprimée : {e}"));
            0
        }
    }
}

/// Page de test : cadre, titre et caractéristiques de l'imprimante.
pub fn test_page(p: &PrinterEntry, dm: &DevMode) -> Vec<u8> {
    let geo = Geometry::new(p, dm);
    let font = |pt: i32, weight: i32| FontSpec {
        height: -(pt * geo.dpi / 72),
        weight,
        face: "Arial".into(),
        ..FontSpec::default()
    };
    let m = geo.dpi / 2;
    let mut cmds = vec![
        DrawCmd::FrameRect {
            rect: gui::Rect {
                left: m,
                top: m,
                right: geo.horz_res - m,
                bottom: geo.vert_res - m,
            },
            color: 0x00D4_7800,
            width: (geo.dpi / 50).max(1) as u16,
        },
        DrawCmd::Text {
            x: m * 2,
            y: m * 2,
            text: "Page de test Winlux".into(),
            color: 0x00D4_7800,
            bk: None,
            font: font(28, 700),
        },
    ];
    let lines = [
        format!("Imprimante : {}", p.name),
        format!("Modèle : {}", p.model),
        format!("Port : {}", p.port),
        format!("Papier : {}", paper(dm.paper).name),
        format!("Résolution : {} ppp", geo.dpi),
        format!("Couleur : {}", if p.color { "oui" } else { "non" }),
        "Si vous lisez cette page, l'impression fonctionne.".to_string(),
    ];
    for (i, l) in lines.iter().enumerate() {
        cmds.push(DrawCmd::Text {
            x: m * 2,
            y: m * 2 + geo.dpi * 3 / 4 + i as i32 * geo.dpi / 3,
            text: l.clone(),
            color: 0,
            bk: None,
            font: font(12, 400),
        });
    }
    for (i, c) in [
        0x0000_00FFu32,
        0x0000_FF00,
        0x00FF_0000,
        0x0000_FFFF,
        0x00FF_00FF,
        0x00FF_FF00,
        0,
    ]
    .iter()
    .enumerate()
    {
        let x = m * 2 + i as i32 * geo.dpi * 3 / 5;
        let y = m * 2 + geo.dpi * 4;
        cmds.push(DrawCmd::Ellipse {
            rect: gui::Rect {
                left: x,
                top: y,
                right: x + geo.dpi / 2,
                bottom: y + geo.dpi / 2,
            },
            color: *c,
        });
    }
    geo.render(
        "Page de test Winlux",
        vec![Page {
            cmds,
            width: geo.phys_w,
            height: geo.phys_h,
        }],
        dm,
    )
}

// ---------- DC imprimante (CreateDC / StartDoc / StartPage…) ----------

/// Géométrie d'une page imprimée (pixels périphérique).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub dpi: i32,
    pub phys_w: i32,
    pub phys_h: i32,
    pub margin: i32,
    pub horz_res: i32,
    pub vert_res: i32,
}

impl Geometry {
    pub fn new(p: &PrinterEntry, dm: &DevMode) -> Geometry {
        let dpi = dm.dpi();
        let pp = paper(dm.paper);
        let (mut w, mut h) = ((pp.width * dpi + 127) / 254, (pp.height * dpi + 127) / 254);
        if dm.landscape() {
            std::mem::swap(&mut w, &mut h);
        }
        let margin = p.margin(dpi);
        Geometry {
            dpi,
            phys_w: w,
            phys_h: h,
            margin,
            horz_res: w - 2 * margin,
            vert_res: h - 2 * margin,
        }
    }

    fn render(&self, title: &str, pages: Vec<Page>, dm: &DevMode) -> Vec<u8> {
        print_pdf::render(
            &pages,
            &DocInfo {
                title,
                dpi: self.dpi,
                offset_x: self.margin,
                offset_y: self.margin,
                grayscale: dm.color == 1,
            },
        )
    }
}

struct GdiDoc {
    title: String,
    output: Option<String>,
    pages: Vec<Page>,
    current: Option<Vec<DrawCmd>>,
    job: u32,
}

struct PrinterDc {
    printer: PrinterEntry,
    devmode: DevMode,
    /// DC d'information (`CreateIC`) : mesures seulement, aucun document.
    info_only: bool,
    doc: Option<GdiDoc>,
    abort_proc: usize,
}

fn printer_dcs() -> &'static Mutex<HashMap<usize, PrinterDc>> {
    static D: OnceLock<Mutex<HashMap<usize, PrinterDc>>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Nombre de DC imprimante vivants : évite de verrouiller la table à chaque
/// commande GDI quand rien n'imprime.
static LIVE_PRINTER_DCS: AtomicUsize = AtomicUsize::new(0);

pub fn is_printer_dc(hdc: usize) -> bool {
    LIVE_PRINTER_DCS.load(Ordering::Relaxed) != 0 && lock(printer_dcs()).contains_key(&hdc)
}

/// Crée un DC imprimante pour `device` (nom d'imprimante) ou l'imprimante par
/// défaut. `driver` « DISPLAY » ou un nom inconnu donnent `None`.
///
/// # Sécurité
/// `devmode` est nul ou pointe sur un `DEVMODE` lisible.
pub unsafe fn create_printer_dc(
    driver: Option<String>,
    device: Option<String>,
    devmode: *const u8,
    wide: bool,
    info_only: bool,
) -> Option<usize> {
    let driver = driver.unwrap_or_default();
    if driver.eq_ignore_ascii_case("DISPLAY") {
        return None;
    }
    let printer = match device.as_deref() {
        Some(d) if !d.is_empty() && !d.eq_ignore_ascii_case("DISPLAY") => find_printer(d)?,
        _ if driver.eq_ignore_ascii_case("WINSPOOL") => default_printer()?,
        _ => find_printer(&driver)?,
    };
    let mut dm = DevMode::for_printer(&printer);
    dm.merge(devmode, wide);
    let hdc = gui::create_dc(0);
    lock(printer_dcs()).insert(
        hdc,
        PrinterDc {
            printer,
            devmode: dm,
            info_only,
            doc: None,
            abort_proc: 0,
        },
    );
    LIVE_PRINTER_DCS.fetch_add(1, Ordering::Relaxed);
    Some(hdc)
}

/// Retire un DC imprimante (`DeleteDC`) ; un document ouvert est abandonné.
pub fn delete_printer_dc(hdc: usize) {
    if lock(printer_dcs()).remove(&hdc).is_some() {
        LIVE_PRINTER_DCS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Police par défaut d'un DC imprimante : 10 pt (le stock « système »
/// mesure 13 pixels, illisible à 300 ppp).
fn printer_font(mut font: FontSpec, dpi: i32) -> FontSpec {
    if font == FontSpec::default() {
        font.height = -(10 * dpi / 72);
        font.face = "Courier New".into();
    }
    font
}

/// Intercepte une commande GDI destinée à un DC imprimante. Renvoie la
/// commande si le DC n'imprime pas (dessin normal), `None` si elle est prise.
pub fn capture(hdc: usize, cmd: DrawCmd) -> Option<DrawCmd> {
    if LIVE_PRINTER_DCS.load(Ordering::Relaxed) == 0 {
        return Some(cmd);
    }
    let mut map = lock(printer_dcs());
    let Some(dc) = map.get_mut(&hdc) else {
        return Some(cmd);
    };
    let dpi = dc.devmode.dpi();
    let cmd = match cmd {
        DrawCmd::Text {
            x,
            y,
            text,
            color,
            bk,
            font,
        } => DrawCmd::Text {
            x,
            y,
            text,
            color,
            bk,
            font: printer_font(font, dpi),
        },
        other => other,
    };
    if let Some(doc) = dc.doc.as_mut() {
        // Dessin hors StartPage : on ouvre la page comme les anciens pilotes.
        doc.current.get_or_insert_with(Vec::new).push(cmd);
    }
    None
}

/// Police effective pour les mesures de texte sur un DC imprimante.
pub fn printer_text_font(hdc: usize, font: FontSpec) -> FontSpec {
    match lock(printer_dcs()).get(&hdc) {
        Some(dc) => printer_font(font, dc.devmode.dpi()),
        None => font,
    }
}

/// `GetDeviceCaps` d'un DC imprimante ; `None` pour un autre DC.
pub fn device_caps(hdc: usize, index: i32) -> Option<i32> {
    if LIVE_PRINTER_DCS.load(Ordering::Relaxed) == 0 {
        return None;
    }
    let map = lock(printer_dcs());
    let dc = map.get(&hdc)?;
    let g = Geometry::new(&dc.printer, &dc.devmode);
    let mm = |px: i32| px * 254 / g.dpi / 10;
    Some(match index {
        0 => 0x0400,                             // DRIVERVERSION
        2 => 2,                                  // TECHNOLOGY : DT_RASPRINTER
        4 => mm(g.horz_res),                     // HORZSIZE
        6 => mm(g.vert_res),                     // VERTSIZE
        8 => g.horz_res,                         // HORZRES
        10 => g.vert_res,                        // VERTRES
        12 => 24,                                // BITSPIXEL
        14 => 1,                                 // PLANES
        16 => -1,                                // NUMBRUSHES
        18 => -1,                                // NUMPENS
        22 => 0,                                 // NUMFONTS
        24 => -1,                                // NUMCOLORS
        26 => 0,                                 // PDEVICESIZE
        28 => 0xff,                              // CURVECAPS
        30 => 0xfe,                              // LINECAPS
        32 => 0xff,                              // POLYGONALCAPS
        34 => 0x7807,                            // TEXTCAPS
        36 => 1,                                 // CLIPCAPS
        38 => 0x0001 | 0x0200 | 0x0800 | 0x2000, // RASTERCAPS
        40 | 42 => 300,                          // ASPECTX / ASPECTY
        44 => 424,                               // ASPECTXY
        88 | 90 => g.dpi,                        // LOGPIXELSX / LOGPIXELSY
        104 | 106 => 0,                          // SIZEPALETTE / NUMRESERVED
        108 => 24,                               // COLORRES
        110 => g.phys_w,                         // PHYSICALWIDTH
        111 => g.phys_h,                         // PHYSICALHEIGHT
        112 | 113 => g.margin,                   // PHYSICALOFFSETX/Y
        114 | 115 => 0,                          // SCALINGFACTORX/Y
        116 | 117 => 0,                          // VREFRESH / DESKTOPVERTRES
        118 => g.horz_res,                       // DESKTOPHORZRES
        119 => 0,                                // BLTALIGNMENT
        120 => 0,                                // SHADEBLENDCAPS
        121 => 0x1,                              // COLORMGMTCAPS
        _ => 0,
    })
}

/// `StartDoc` : ouvre un document ; renvoie l'identifiant du travail (> 0).
pub fn start_doc(hdc: usize, title: String, output: Option<String>) -> i32 {
    let mut map = lock(printer_dcs());
    let Some(dc) = map.get_mut(&hdc) else {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return -1; // SP_ERROR
    };
    if dc.info_only {
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        return -1;
    }
    let job = NEXT_LOCAL_JOB.fetch_add(1, Ordering::Relaxed) as u32;
    dc.doc = Some(GdiDoc {
        title,
        output,
        pages: Vec::new(),
        current: None,
        job,
    });
    job as i32
}

pub fn start_page(hdc: usize) -> i32 {
    let mut map = lock(printer_dcs());
    match map.get_mut(&hdc).and_then(|dc| dc.doc.as_mut()) {
        Some(doc) => {
            if doc.current.is_none() {
                doc.current = Some(Vec::new());
            }
            1
        }
        None => {
            crate::process::set_last_error(ERROR_SPL_NO_STARTDOC);
            -1
        }
    }
}

/// Appelle l'`AbortProc` de l'application ; `false` si elle demande l'arrêt.
fn call_abort_proc(hdc: usize, proc_addr: usize) -> bool {
    if proc_addr == 0 {
        return true;
    }
    // SAFETY: adresse fournie par SetAbortProc (fonction invitée win64).
    let f: extern "win64" fn(usize, i32) -> i32 = unsafe { std::mem::transmute(proc_addr) };
    f(hdc, 0) != 0
}

pub fn end_page(hdc: usize) -> i32 {
    let abort = {
        let mut map = lock(printer_dcs());
        let Some(dc) = map.get_mut(&hdc) else {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            return -1;
        };
        let g = Geometry::new(&dc.printer, &dc.devmode);
        let Some(doc) = dc.doc.as_mut() else {
            crate::process::set_last_error(ERROR_SPL_NO_STARTDOC);
            return -1;
        };
        let cmds = doc.current.take().unwrap_or_default();
        doc.pages.push(Page {
            cmds,
            width: g.phys_w,
            height: g.phys_h,
        });
        dc.abort_proc
    };
    if !call_abort_proc(hdc, abort) {
        abort_doc(hdc);
        crate::process::set_last_error(ERROR_PRINT_CANCELLED);
        return -1;
    }
    1
}

pub fn end_doc(hdc: usize) -> i32 {
    let (doc, printer, devmode) = {
        let mut map = lock(printer_dcs());
        let Some(dc) = map.get_mut(&hdc) else {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            return -1;
        };
        let Some(mut doc) = dc.doc.take() else {
            crate::process::set_last_error(ERROR_SPL_NO_STARTDOC);
            return -1;
        };
        // Page commencée mais jamais terminée : elle est imprimée quand même.
        if let Some(cmds) = doc.current.take() {
            if !cmds.is_empty() {
                let g = Geometry::new(&dc.printer, &dc.devmode);
                doc.pages.push(Page {
                    cmds,
                    width: g.phys_w,
                    height: g.phys_h,
                });
            }
        }
        (doc, dc.printer.clone(), dc.devmode.clone())
    };
    let geo = Geometry::new(&printer, &devmode);
    let pdf = geo.render(&doc.title, doc.pages, &devmode);
    match submit(&printer, &doc.title, doc.output.as_deref(), &devmode, pdf) {
        Ok(_) => doc.job.max(1) as i32,
        Err(e) => {
            set_message(e.clone());
            crate::log_warn!("winspool", "winlux: impression échouée : {e}");
            crate::process::set_last_error(ERROR_NOT_READY);
            -1
        }
    }
}

pub fn abort_doc(hdc: usize) -> i32 {
    let mut map = lock(printer_dcs());
    match map.get_mut(&hdc) {
        Some(dc) => {
            dc.doc = None;
            1
        }
        None => {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            -1
        }
    }
}

pub fn set_abort_proc(hdc: usize, proc_addr: usize) -> i32 {
    let mut map = lock(printer_dcs());
    match map.get_mut(&hdc) {
        Some(dc) => {
            dc.abort_proc = proc_addr;
            1
        }
        None => {
            crate::process::set_last_error(ERROR_INVALID_HANDLE);
            -1
        }
    }
}

/// `ResetDC` : nouveaux réglages (orientation, papier…) entre deux pages.
///
/// # Sécurité
/// `devmode` est nul ou pointe sur un `DEVMODE` lisible.
pub unsafe fn reset_dc(hdc: usize, devmode: *const u8, wide: bool) -> bool {
    let mut map = lock(printer_dcs());
    match map.get_mut(&hdc) {
        Some(dc) => {
            dc.devmode.merge(devmode, wide);
            true
        }
        None => false,
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "OpenPrinterA" => fn open_printer_a,
        "OpenPrinterW" => fn open_printer_w,
        "OpenPrinter2W" => fn open_printer2_w,
        "ClosePrinter" => fn close_printer,
        "StartDocPrinterA" => fn start_doc_printer_a,
        "StartDocPrinterW" => fn start_doc_printer_w,
        "StartPagePrinter" => fn start_page_printer,
        "WritePrinter" => fn write_printer,
        "EndPagePrinter" => fn end_page_printer,
        "EndDocPrinter" => fn end_doc_printer,
        "AbortPrinter" => fn abort_printer,
        "EnumPrintersA" => fn enum_printers_a,
        "EnumPrintersW" => fn enum_printers_w,
        "GetPrinterA" => fn get_printer_a,
        "GetPrinterW" => fn get_printer_w,
        "SetPrinterA" => fn set_printer,
        "SetPrinterW" => fn set_printer,
        "GetDefaultPrinterA" => fn get_default_printer_a,
        "GetDefaultPrinterW" => fn get_default_printer_w,
        "SetDefaultPrinterA" => fn set_default_printer_a,
        "SetDefaultPrinterW" => fn set_default_printer_w,
        "AddPrinterA" => fn add_printer_a,
        "AddPrinterW" => fn add_printer_w,
        "DeletePrinter" => fn delete_printer_api,
        "EnumJobsA" => fn enum_jobs_a,
        "EnumJobsW" => fn enum_jobs_w,
        "GetJobA" => fn get_job_a,
        "GetJobW" => fn get_job_w,
        "SetJobA" => fn set_job,
        "SetJobW" => fn set_job,
        "DeviceCapabilitiesA" => fn device_capabilities_a,
        "DeviceCapabilitiesW" => fn device_capabilities_w,
        "DocumentPropertiesA" => fn document_properties_a,
        "DocumentPropertiesW" => fn document_properties_w,
        "AdvancedDocumentPropertiesA" => fn advanced_document_properties_a,
        "AdvancedDocumentPropertiesW" => fn advanced_document_properties_w,
        "PrinterProperties" => fn printer_properties,
        "GetPrinterDriverDirectoryW" => fn get_printer_driver_directory_w,
        "WinluxDiscoverPrintersW" => fn winlux_discover_printers_w,
        "WinluxAddPrinterW" => fn winlux_add_printer_w,
        "WinluxPrintStatusW" => fn winlux_print_status_w,
        "WinluxPrintTestPageW" => fn winlux_print_test_page_w,
    }
}

// ---------- Exports GDI de l'impression (gdi32) ----------

unsafe fn start_doc_common(hdc: usize, info: *const u8, wide: bool) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    // DOCINFO { cbSize, lpszDocName, lpszOutput, lpszDatatype, fwType }
    let read = |off: usize| std::ptr::read_unaligned(info.add(off) as *const *const c_void);
    let title = guest_str(read(8), wide).unwrap_or_else(|| "Document".into());
    let output = guest_str(read(16), wide);
    start_doc(hdc, title, output)
}

pub unsafe extern "win64" fn start_doc_a(hdc: usize, info: *const u8) -> i32 {
    start_doc_common(hdc, info, false)
}

pub unsafe extern "win64" fn start_doc_w(hdc: usize, info: *const u8) -> i32 {
    start_doc_common(hdc, info, true)
}

pub extern "win64" fn start_page_gdi(hdc: usize) -> i32 {
    start_page(hdc)
}

pub extern "win64" fn end_page_gdi(hdc: usize) -> i32 {
    end_page(hdc)
}

pub extern "win64" fn end_doc_gdi(hdc: usize) -> i32 {
    end_doc(hdc)
}

pub extern "win64" fn abort_doc_gdi(hdc: usize) -> i32 {
    abort_doc(hdc)
}

pub extern "win64" fn set_abort_proc_gdi(hdc: usize, proc_addr: usize) -> i32 {
    set_abort_proc(hdc, proc_addr)
}

pub unsafe extern "win64" fn reset_dc_a(hdc: usize, devmode: *const u8) -> usize {
    if reset_dc(hdc, devmode, false) {
        hdc
    } else {
        0
    }
}

pub unsafe extern "win64" fn reset_dc_w(hdc: usize, devmode: *const u8) -> usize {
    if reset_dc(hdc, devmode, true) {
        hdc
    } else {
        0
    }
}

/// `Escape` : anciennes commandes d'impression 16 bits encore utilisées.
pub unsafe extern "win64" fn escape(
    hdc: usize,
    code: i32,
    in_size: i32,
    input: *const u8,
    output: *mut c_void,
) -> i32 {
    const NEWFRAME: i32 = 1;
    const ABORTDOC: i32 = 2;
    const QUERYESCSUPPORT: i32 = 8;
    const SETABORTPROC: i32 = 9;
    const STARTDOC: i32 = 10;
    const ENDDOC: i32 = 11;
    const GETPHYSPAGESIZE: i32 = 12;
    const GETPRINTINGOFFSET: i32 = 13;
    const SUPPORTED: [i32; 8] = [
        NEWFRAME,
        ABORTDOC,
        QUERYESCSUPPORT,
        SETABORTPROC,
        STARTDOC,
        ENDDOC,
        GETPHYSPAGESIZE,
        GETPRINTINGOFFSET,
    ];
    if !is_printer_dc(hdc) {
        return 0;
    }
    let point = |x: i32, y: i32| {
        if !output.is_null() {
            std::ptr::write_unaligned(output as *mut [i32; 2], [x, y]);
        }
        1
    };
    match code {
        QUERYESCSUPPORT if !input.is_null() && in_size >= 4 => {
            let asked = std::ptr::read_unaligned(input as *const i32);
            i32::from(SUPPORTED.contains(&asked))
        }
        NEWFRAME => {
            if end_page(hdc) < 0 {
                return -1;
            }
            start_page(hdc)
        }
        ABORTDOC => abort_doc(hdc),
        SETABORTPROC => set_abort_proc(hdc, input as usize),
        STARTDOC => {
            let title = if input.is_null() {
                "Document".to_string()
            } else {
                crate::dlls::cstr(input)
            };
            start_doc(hdc, title, None)
        }
        ENDDOC => end_doc(hdc),
        GETPHYSPAGESIZE => point(
            device_caps(hdc, 110).unwrap_or(0),
            device_caps(hdc, 111).unwrap_or(0),
        ),
        GETPRINTINGOFFSET => point(
            device_caps(hdc, 112).unwrap_or(0),
            device_caps(hdc, 113).unwrap_or(0),
        ),
        _ => 0,
    }
}

pub fn gdi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "StartDocA" => fn start_doc_a,
        "StartDocW" => fn start_doc_w,
        "StartPage" => fn start_page_gdi,
        "EndPage" => fn end_page_gdi,
        "EndDoc" => fn end_doc_gdi,
        "AbortDoc" => fn abort_doc_gdi,
        "SetAbortProc" => fn set_abort_proc_gdi,
        "ResetDCA" => fn reset_dc_a,
        "ResetDCW" => fn reset_dc_w,
        "Escape" => fn escape,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlls::ipp::tests::fake_cups;
    use crate::dlls::ipp::{Message, Value, TAG_JOB, TAG_OPERATION, TAG_PRINTER};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("winlux-print-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn devmode_aller_retour_a_et_w() {
        let dm = DevMode {
            device: "Laser Bureau".into(),
            orientation: 2,
            paper: 8,
            scale: 100,
            copies: 3,
            quality: -4,
            color: 1,
            duplex: 2,
            collate: 0,
        };
        for wide in [true, false] {
            let bytes = dm.to_bytes(wide);
            assert_eq!(
                bytes.len(),
                if wide { DEVMODEW_SIZE } else { DEVMODEA_SIZE }
            );
            let mut back = DevMode {
                device: "Laser Bureau".into(),
                ..DevMode::for_printer(&PrinterEntry::pdf())
            };
            unsafe { back.merge(bytes.as_ptr(), wide) };
            assert_eq!(back, dm);
            assert_eq!(
                unsafe { read_fixed_str(bytes.as_ptr(), 32, wide) },
                "Laser Bureau"
            );
        }
        assert_eq!(dm.dpi(), 600);
        let o = dm.job_options();
        assert_eq!(o.media, "iso_a3_297x420mm");
        assert!(!o.color);
        assert_eq!(o.quality, 5);
    }

    #[test]
    fn enum_printers_pdf_sans_cups() {
        let _g = lock(&TEST_LOCK);
        set_cups_server(None);
        let mut needed = 0u32;
        let mut count = 0u32;
        let r = unsafe {
            enum_printers_w(
                2,
                std::ptr::null(),
                2,
                std::ptr::null_mut(),
                0,
                &mut needed,
                &mut count,
            )
        };
        assert_eq!(r, 0);
        assert!(needed >= 136 + DEVMODEW_SIZE as u32);
        let mut buf = vec![0u64; needed as usize / 8 + 1];
        let r = unsafe {
            enum_printers_w(
                2,
                std::ptr::null(),
                2,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
                &mut count,
            )
        };
        assert_eq!((r, count), (1, 1));
        let base = buf.as_ptr() as *const u8;
        unsafe {
            let name = *(base.add(8) as *const *const u16);
            assert_eq!(crate::dlls::wstr(name), PDF_PRINTER);
            let dm = *(base.add(56) as *const *const u8);
            assert_eq!(read_fixed_str(dm, 32, true), PDF_PRINTER);
            let attrs = *(base.add(104) as *const u32);
            assert_ne!(attrs & 0x4, 0, "imprimante par défaut");
        }
        // GetDefaultPrinterW : taille demandée puis nom.
        let mut cch = 0u32;
        assert_eq!(
            unsafe { get_default_printer_w(std::ptr::null_mut(), &mut cch) },
            0
        );
        assert_eq!(cch as usize, PDF_PRINTER.encode_utf16().count() + 1);
        let mut name = vec![0u16; cch as usize];
        assert_eq!(
            unsafe { get_default_printer_w(name.as_mut_ptr(), &mut cch) },
            1
        );
        assert_eq!(unsafe { crate::dlls::wstr(name.as_ptr()) }, PDF_PRINTER);
        // DeviceCapabilities : formats et noms.
        let dev = wide(PDF_PRINTER);
        let n = unsafe {
            device_capabilities_w(
                dev.as_ptr(),
                std::ptr::null(),
                2,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert_eq!(n as usize, PAPERS.len());
        let mut names = vec![0u16; 64 * n as usize];
        unsafe {
            device_capabilities_w(
                dev.as_ptr(),
                std::ptr::null(),
                16,
                names.as_mut_ptr().cast(),
                std::ptr::null(),
            );
            assert_eq!(crate::dlls::wstr(names.as_ptr()), "A4");
        }
        set_cups_server(None);
    }

    #[test]
    fn dc_imprimante_vers_pdf() {
        let _g = lock(&TEST_LOCK);
        set_cups_server(None);
        let out = temp_file("dc.pdf");
        let _ = std::fs::remove_file(&out);
        let mut dm = DevMode::for_printer(&PrinterEntry::pdf()).to_bytes(true);
        dm[76..78].copy_from_slice(&2i16.to_le_bytes()); // paysage
        let hdc = unsafe {
            create_printer_dc(
                Some("WINSPOOL".into()),
                Some(PDF_PRINTER.into()),
                dm.as_ptr(),
                true,
                false,
            )
        }
        .unwrap();
        assert!(is_printer_dc(hdc));
        // A4 paysage à 300 ppp, sans marge pour l'imprimante PDF.
        assert_eq!(device_caps(hdc, 8), Some(3508));
        assert_eq!(device_caps(hdc, 10), Some(2480));
        assert_eq!(device_caps(hdc, 88), Some(300));
        assert_eq!(device_caps(hdc, 2), Some(2));
        let job = start_doc(
            hdc,
            "Facture".into(),
            Some(out.to_string_lossy().into_owned()),
        );
        assert!(job > 0);
        for page in 0..2 {
            assert_eq!(start_page(hdc), 1);
            assert!(gui::record(
                hdc,
                DrawCmd::Text {
                    x: 300,
                    y: 300,
                    text: format!("Page {}", page + 1),
                    color: 0,
                    bk: None,
                    font: FontSpec::default(),
                },
            ));
            assert_eq!(end_page(hdc), 1);
        }
        assert!(end_doc(hdc) > 0);
        delete_printer_dc(hdc);
        gui::free_dc(hdc);
        assert!(!is_printer_dc(hdc));
        let pdf = std::fs::read(&out).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"));
        assert!(text.contains("(Page 2) Tj"));
        // Police par défaut du DC imprimante : Courier 10 pt.
        assert!(text.contains("/BaseFont /Courier"));
        assert!(text.contains("/MediaBox [0 0 841.92 595.20]"));
        set_cups_server(None);
    }

    #[test]
    fn spooler_brut_vers_cups() {
        let _g = lock(&TEST_LOCK);
        let (server, h) = fake_cups(
            |req| {
                let mut r = Message::request(0, req.request_id);
                match req.code {
                    ipp::OP_CUPS_GET_PRINTERS => {
                        r.add(TAG_PRINTER, "printer-name", ipp::name("Laser"));
                        r.add(TAG_PRINTER, "printer-state", Value::Enum(3));
                        r.add(TAG_PRINTER, "device-uri", ipp::uri("usb://HP/LaserJet"));
                        r.add_values(
                            TAG_PRINTER,
                            "media-supported",
                            vec![
                                ipp::keyword("iso_a4_210x297mm"),
                                ipp::keyword("na_letter_8.5x11in"),
                            ],
                        );
                        r.add(
                            TAG_PRINTER,
                            "media-default",
                            ipp::keyword("na_letter_8.5x11in"),
                        );
                    }
                    ipp::OP_CUPS_GET_DEFAULT => {
                        r.add(TAG_PRINTER, "printer-name", ipp::name("Laser"));
                    }
                    ipp::OP_PRINT_JOB => {
                        r.add(TAG_JOB, "job-id", Value::Integer(77));
                    }
                    _ => r.code = 0x0400,
                }
                r
            },
            3,
        );
        set_cups_server(Some(server));
        let laser = find_printer("laser").unwrap();
        assert_eq!(laser.kind, Kind::Cups);
        assert_eq!(laser.papers, vec![9, 1]);
        assert_eq!(laser.default_paper, 1);
        assert!(laser.is_default || stored_default().is_some());
        let name = wide("Laser");
        let mut hp = 0usize;
        assert_eq!(
            unsafe { open_printer_w(name.as_ptr(), &mut hp, std::ptr::null()) },
            1
        );
        let doc = wide("Brut");
        let info: [*const u16; 3] = [doc.as_ptr(), std::ptr::null(), std::ptr::null()];
        assert!(unsafe { start_doc_printer_w(hp, 1, info.as_ptr().cast()) } > 0);
        assert_eq!(start_page_printer(hp), 1);
        let mut written = 0u32;
        let data = b"\x1bE PCL brut";
        assert_eq!(
            unsafe { write_printer(hp, data.as_ptr(), data.len() as u32, &mut written) },
            1
        );
        assert_eq!(written as usize, data.len());
        assert_eq!(end_page_printer(hp), 1);
        assert_eq!(end_doc_printer(hp), 1);
        assert_eq!(close_printer(hp), 1);
        let seen = h.join().unwrap();
        let job = &seen[2];
        assert_eq!(job.code, ipp::OP_PRINT_JOB);
        assert_eq!(job.data, data);
        let op = job.groups(TAG_OPERATION).next().unwrap();
        assert_eq!(op.str("document-format"), Some("application/vnd.cups-raw"));
        assert_eq!(op.str("job-name"), Some("Brut"));
        let attrs = job.groups(TAG_JOB).next().unwrap();
        assert_eq!(attrs.str("media"), Some("na_letter_8.5x11in"));
        set_cups_server(None);
    }

    #[test]
    fn date_des_travaux() {
        // 2026-10-31 12:34:56 UTC, un samedi.
        assert_eq!(
            unix_to_systemtime(1_793_450_096),
            [2026, 10, 6, 31, 12, 34, 56, 0]
        );
    }
}
