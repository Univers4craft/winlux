//! Installateurs Squirrel.Windows (`Setup.exe` de l'appli de bureau Claude,
//! de Slack, Discord, Teams classique…), installés par Winlux lui-même.
//!
//! Le `Setup.exe` est un petit programme 32 bits qui porte une archive ZIP
//! dans sa ressource `DATA` n° 131 : `Update.exe` (l'outil .NET de mise à
//! jour), un fichier `RELEASES` et le paquet `<Id>-<version>-full.nupkg`.
//! Ce paquet, lui aussi un ZIP, contient l'application dans `lib/net45/` et
//! sa description dans `<Id>.nuspec`.
//!
//! Squirrel installe dans `%LocalAppData%\<Id>\app-<version>\`, copie
//! `Update.exe` à côté et crée les raccourcis. Winlux fait de même sans
//! exécuter le code Windows de l'installateur ; les raccourcis visent
//! directement le programme de l'application, puisque `Update.exe --processStart`
//! n'est qu'un relais vers lui.

use crate::archive::zip::{self, ZipEntry};
use std::fs::File;
use std::io::{Read, Write};

/// Numéro de la ressource `DATA` qui porte l'archive (IDR_UPDATE_ZIP).
const RESOURCE_ID: u32 = 131;

fn rd16(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}
fn rd32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

/// Ressource d'un PE (32 ou 64 bits) désignée par un type nommé et un
/// numéro : (décalage dans le fichier, taille) de ses données.
pub fn find_resource(data: &[u8], type_name: &str, id: u32) -> Option<(usize, usize)> {
    let pe = rd32(data, 0x3C)? as usize;
    if data.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let coff = pe + 4;
    let nsec = rd16(data, coff + 2)? as usize;
    let opt_size = rd16(data, coff + 16)? as usize;
    let opt = coff + 20;
    let dirs = match rd16(data, opt)? {
        0x10B => opt + 96,
        0x20B => opt + 112,
        _ => return None,
    };
    let rsrc_rva = rd32(data, dirs + 2 * 8)? as usize;
    if rsrc_rva == 0 {
        return None;
    }
    let sections = opt + opt_size;
    let to_file = |rva: usize| -> Option<usize> {
        (0..nsec).find_map(|i| {
            let s = sections + i * 40;
            let va = rd32(data, s + 12)? as usize;
            let vsize = (rd32(data, s + 8)? as usize).max(rd32(data, s + 16)? as usize);
            let raw = rd32(data, s + 20)? as usize;
            (rva >= va && rva < va + vsize).then_some(raw + rva - va)
        })
    };
    let root = to_file(rsrc_rva)?;
    // Entrées d'un répertoire : (nom ou numéro, décalage, sous-répertoire ?).
    let entries = |dir: usize| -> Vec<(Result<String, u32>, usize, bool)> {
        let named = rd16(data, dir + 12).unwrap_or(0) as usize;
        let ids = rd16(data, dir + 14).unwrap_or(0) as usize;
        (0..(named + ids).min(4096))
            .filter_map(|i| {
                let e = dir + 16 + i * 8;
                let name = rd32(data, e)?;
                let off = rd32(data, e + 4)?;
                let key = if name & 0x8000_0000 != 0 {
                    let at = root + (name & 0x7FFF_FFFF) as usize;
                    let len = rd16(data, at)? as usize;
                    let units: Vec<u16> = (0..len)
                        .filter_map(|k| rd16(data, at + 2 + k * 2))
                        .collect();
                    Ok(String::from_utf16_lossy(&units))
                } else {
                    Err(name)
                };
                Some((
                    key,
                    root + (off & 0x7FFF_FFFF) as usize,
                    off & 0x8000_0000 != 0,
                ))
            })
            .collect()
    };
    let (_, types, _) = entries(root)
        .into_iter()
        .find(|(k, _, sub)| *sub && matches!(k, Ok(n) if n.eq_ignore_ascii_case(type_name)))?;
    let (_, names, _) = entries(types)
        .into_iter()
        .find(|(k, _, sub)| *sub && *k == Err(id))?;
    let (_, leaf, _) = entries(names).into_iter().find(|(_, _, sub)| !sub)?;
    let rva = rd32(data, leaf)? as usize;
    let size = rd32(data, leaf + 4)? as usize;
    let at = to_file(rva)?;
    (at + size <= data.len()).then_some((at, size))
}

/// Archive Squirrel d'un `Setup.exe` : (décalage, taille) du ZIP.
pub fn find_package(data: &[u8]) -> Option<(usize, usize)> {
    let (at, size) = find_resource(data, "DATA", RESOURCE_ID)?;
    (data.get(at..at + 4)? == b"PK\x03\x04").then_some((at, size))
}

fn bad(e: impl std::fmt::Display) -> String {
    format!("installateur Squirrel illisible : {e}")
}

/// Description du paquet (`.nuspec`).
#[derive(Default, Debug, PartialEq)]
pub struct Nuspec {
    pub id: String,
    pub version: String,
    pub title: String,
    pub authors: String,
}

fn xml_tag(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    text.find(&open)
        .and_then(|a| {
            let rest = &text[a + open.len()..];
            rest.find(&close).map(|b| rest[..b].trim().to_string())
        })
        .map(|s| {
            s.replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
        })
        .unwrap_or_default()
}

pub fn parse_nuspec(text: &str) -> Nuspec {
    Nuspec {
        id: xml_tag(text, "id"),
        version: xml_tag(text, "version"),
        title: xml_tag(text, "title"),
        authors: xml_tag(text, "authors"),
    }
}

/// Copie temporaire (supprimée au `Drop`).
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn temp_file(tag: &str) -> std::path::PathBuf {
    // Compteur : deux ouvertures dans le même processus (fil d'installation
    // et lecture des informations) ne partagent pas leurs copies.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("winlux-squirrel-{}-{n}-{tag}", std::process::id()))
}

/// Ouvre le ZIP porté par le `Setup.exe` et le paquet complet qu'il contient.
struct Opened {
    outer: File,
    outer_entries: Vec<ZipEntry>,
    nupkg: File,
    nupkg_entries: Vec<ZipEntry>,
    spec: Nuspec,
    _temps: Vec<Temp>,
}

fn open(data: &[u8]) -> Result<Opened, String> {
    let (at, size) = find_package(data).ok_or_else(|| bad("archive absente"))?;
    let outer_path = temp_file("setup.zip");
    std::fs::write(&outer_path, &data[at..at + size]).map_err(bad)?;
    let mut temps = vec![Temp(outer_path.clone())];
    let mut outer = File::open(&outer_path).map_err(bad)?;
    let outer_entries = zip::list(&mut outer).map_err(bad)?;
    let full = outer_entries
        .iter()
        .find(|e| e.name.to_ascii_lowercase().ends_with("-full.nupkg"))
        .ok_or_else(|| bad("paquet complet (-full.nupkg) absent"))?
        .clone();
    let nupkg_path = temp_file("full.nupkg");
    {
        let mut out = File::create(&nupkg_path).map_err(bad)?;
        let mut r = zip::open_entry(&mut outer, &full, None).map_err(bad)?;
        std::io::copy(&mut r, &mut out).map_err(bad)?;
    }
    temps.push(Temp(nupkg_path.clone()));
    let mut nupkg = File::open(&nupkg_path).map_err(bad)?;
    let nupkg_entries = zip::list(&mut nupkg).map_err(bad)?;
    let spec_entry = nupkg_entries
        .iter()
        .find(|e| !e.name.contains('/') && e.name.to_ascii_lowercase().ends_with(".nuspec"))
        .ok_or_else(|| bad("description .nuspec absente"))?
        .clone();
    let mut text = String::new();
    zip::open_entry(&mut nupkg, &spec_entry, None)
        .map_err(bad)?
        .read_to_string(&mut text)
        .map_err(bad)?;
    let spec = parse_nuspec(&text);
    if spec.id.is_empty() || spec.version.is_empty() {
        return Err(bad("identifiant ou version absents du .nuspec"));
    }
    Ok(Opened {
        outer,
        outer_entries,
        nupkg,
        nupkg_entries,
        spec,
        _temps: temps,
    })
}

fn display_name(spec: &Nuspec) -> String {
    if spec.title.is_empty() {
        spec.id.clone()
    } else {
        spec.title.clone()
    }
}

/// Dossier Squirrel : `%LocalAppData%\<Id>`, sous le profil du compte
/// (`C:\Users\<nom>\AppData\Local\<Id>`), comme le voit l'application.
fn default_root(spec: &Nuspec) -> String {
    crate::paths::profile_path(&format!("AppData\\Local\\{}", spec.id))
}

fn root_dir(spec: &Nuspec, opts: &super::Options) -> String {
    opts.dir.clone().unwrap_or_else(|| default_root(spec))
}

/// Programme graphique Windows de l'architecture de Winlux (x64, ARM64) ou
/// 32 bits x86, que WOW64 fait tourner (paquets Squirrel « -x86 »).
fn is_gui_program(head: &[u8]) -> bool {
    let Some((x64, gui)) = super::sevenz::is_gui_pe64(head) else {
        return false;
    };
    gui && (x64 || super::sevenz::pe_machine(head) == Some(0x14c))
}

pub fn info(data: &[u8]) -> Result<super::Info, String> {
    let o = open(data)?;
    Ok(super::Info {
        kind: super::Kind::Squirrel,
        name: display_name(&o.spec),
        version: o.spec.version.clone(),
        publisher: o.spec.authors.clone(),
        default_dir: default_root(&o.spec),
    })
}

/// Programme principal : celui qui porte le nom de l'application, sinon le
/// plus gros programme graphique (64 ou 32 bits) à la racine (les relais de
/// Squirrel exclus).
fn main_program(candidates: &[(String, u64)], spec: &Nuspec) -> Option<String> {
    let wanted = [display_name(spec).to_lowercase(), spec.id.to_lowercase()];
    let ok = |n: &str| {
        let l = n.to_lowercase();
        l != "squirrel.exe"
            && l != "update.exe"
            && !l.contains("executionstub")
            && !l.starts_with("crashpad")
    };
    let pool: Vec<&(String, u64)> = candidates.iter().filter(|(n, _)| ok(n)).collect();
    pool.iter()
        .find(|(n, _)| {
            wanted
                .iter()
                .any(|w| n.to_lowercase() == format!("{w}.exe"))
        })
        .or_else(|| pool.iter().max_by_key(|(_, size)| *size))
        .map(|(n, _)| n.clone())
}

pub fn install(
    data: &[u8],
    opts: &super::Options,
    inst: &mut super::Installer,
) -> Result<(), String> {
    let mut o = open(data)?;
    let name = display_name(&o.spec);
    let root = root_dir(&o.spec, opts);
    let app = format!("{root}\\app-{}", o.spec.version);
    inst.log
        .log(&format!("{name} {} (Squirrel) → {app}", o.spec.version));
    inst.mkdirs_win(&app).map_err(|e| format!("{app} : {e}"))?;

    let mut programs = Vec::new();
    for e in o.nupkg_entries.clone() {
        let Some(rel) = e.name.strip_prefix("lib/net45/") else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        let win = format!("{app}\\{}", rel.replace('/', "\\"));
        if e.is_dir() {
            inst.mkdirs_win(win.trim_end_matches('\\'))
                .map_err(|er| format!("{win} : {er}"))?;
            continue;
        }
        let (_, mut out) = inst
            .create_file(&win)
            .map_err(|er| format!("{win} : {er}"))?;
        let mut r = zip::open_entry(&mut o.nupkg, &e, None).map_err(bad)?;
        // Les premiers octets disent s'il s'agit d'un programme graphique.
        let mut head = vec![0u8; 4096];
        let mut got = 0;
        while got < head.len() {
            let n = r.read(&mut head[got..]).map_err(bad)?;
            if n == 0 {
                break;
            }
            got += n;
        }
        out.write_all(&head[..got]).map_err(bad)?;
        std::io::copy(&mut r, &mut out).map_err(|er| format!("{win} : {er}"))?;
        if !rel.contains('/')
            && rel.to_lowercase().ends_with(".exe")
            && is_gui_program(&head[..got])
        {
            programs.push((rel.to_string(), e.usize));
        }
    }
    // Update.exe à la racine, comme Squirrel (l'appli le cherche en « ..\Update.exe »).
    if let Some(u) = o
        .outer_entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case("Update.exe"))
        .cloned()
    {
        let win = format!("{root}\\Update.exe");
        let (_, mut out) = inst
            .create_file(&win)
            .map_err(|er| format!("{win} : {er}"))?;
        let mut r = zip::open_entry(&mut o.outer, &u, None).map_err(bad)?;
        std::io::copy(&mut r, &mut out).map_err(|er| format!("{win} : {er}"))?;
    }

    let exe = main_program(&programs, &o.spec)
        .ok_or_else(|| bad("aucun programme graphique Windows dans le paquet"))?;
    let target = format!("{app}\\{exe}");
    let menu = super::Folders::programs();
    let company = o
        .spec
        .authors
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let link = if company.is_empty() {
        format!("{menu}\\{name}.lnk")
    } else {
        format!("{menu}\\{company}\\{name}.lnk")
    };
    inst.shortcut(&link, &target, "", &app, &name)
        .map_err(|e| format!("{link} : {e}"))?;
    if !opts.no_desktop {
        let link = format!("{}\\{name}.lnk", super::Folders::desktop());
        inst.shortcut(&link, &target, "", &app, &name)
            .map_err(|e| format!("{link} : {e}"))?;
    }
    inst.manifest.id = o.spec.id.clone();
    inst.manifest.name = name;
    inst.manifest.version = o.spec.version.clone();
    inst.manifest.publisher = company;
    inst.manifest.location = root;
    // Squirrel lance l'application à la fin de l'installation, avec
    // `--squirrel-firstrun` (documentation publique de Squirrel.Windows) ;
    // sans attendre, comme le Setup.exe d'origine (#1730).
    let outcome = super::run::program(&target, "--squirrel-firstrun", &app, false, false);
    inst.log.log(&super::run::describe(
        &format!("{target} --squirrel-firstrun"),
        &outcome,
    ));
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// ZIP « stocké » (sans compression) en mémoire.
    pub fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let crc = crate::archive::crc32::crc32(data);
            let off = out.len() as u32;
            let mut h = Vec::new();
            h.extend(0x0403_4b50u32.to_le_bytes());
            h.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            h.extend(crc.to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            h.extend(name.as_bytes());
            out.extend(h);
            out.extend(*data);
            let mut c = Vec::new();
            c.extend(0x0201_4b50u32.to_le_bytes());
            c.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            c.extend(crc.to_le_bytes());
            c.extend((data.len() as u32).to_le_bytes());
            c.extend((data.len() as u32).to_le_bytes());
            c.extend((name.len() as u16).to_le_bytes());
            c.extend([0u8; 12]);
            c.extend(off.to_le_bytes());
            c.extend(name.as_bytes());
            central.extend(c);
        }
        let cd_off = out.len() as u32;
        out.extend(&central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(cd_off.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    /// PE32 minimal avec une section .rsrc : DATA / 131 / 1033 → `payload`.
    pub fn setup_exe(payload: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0] = b'M';
        b[1] = b'Z';
        b[0x3C] = 0x80;
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        b[coff..coff + 2].copy_from_slice(&0x14cu16.to_le_bytes());
        b[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        b[coff + 16..coff + 18].copy_from_slice(&224u16.to_le_bytes());
        let opt = coff + 20;
        b[opt..opt + 2].copy_from_slice(&0x10Bu16.to_le_bytes());
        let rsrc_rva = 0x1000u32;
        b[opt + 96 + 16..opt + 96 + 20].copy_from_slice(&rsrc_rva.to_le_bytes());
        // Répertoire de ressources.
        let mut r = Vec::new();
        let dir = |r: &mut Vec<u8>, named: u16, ids: u16| {
            r.extend([0u8; 12]);
            r.extend(named.to_le_bytes());
            r.extend(ids.to_le_bytes());
        };
        dir(&mut r, 1, 0); // 0 : racine
        r.extend((0x8000_0000u32 | 0x60).to_le_bytes()); // nom « DATA » en 0x60
        r.extend((0x8000_0000u32 | 0x18).to_le_bytes());
        dir(&mut r, 0, 1); // 0x18 : types
        r.extend(RESOURCE_ID.to_le_bytes());
        r.extend((0x8000_0000u32 | 0x30).to_le_bytes());
        dir(&mut r, 0, 1); // 0x30 : langues
        r.extend(1033u32.to_le_bytes());
        r.extend(0x48u32.to_le_bytes());
        r.resize(0x48, 0);
        r.extend((rsrc_rva + 0x80).to_le_bytes()); // données en 0x80
        r.extend((payload.len() as u32).to_le_bytes());
        r.extend([0u8; 8]);
        r.resize(0x60, 0);
        r.extend(4u16.to_le_bytes());
        for c in "DATA".encode_utf16() {
            r.extend(c.to_le_bytes());
        }
        r.resize(0x80, 0);
        r.extend(payload);
        let sec = opt + 224;
        b[sec..sec + 5].copy_from_slice(b".rsrc");
        b[sec + 8..sec + 12].copy_from_slice(&(r.len() as u32).to_le_bytes());
        b[sec + 12..sec + 16].copy_from_slice(&rsrc_rva.to_le_bytes());
        b[sec + 16..sec + 20].copy_from_slice(&(r.len() as u32).to_le_bytes());
        b[sec + 20..sec + 24].copy_from_slice(&0x400u32.to_le_bytes());
        b.extend(r);
        b
    }

    /// En-tête d'un programme graphique 64 bits (assez pour is_gui_pe64).
    pub fn gui64() -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[0] = b'M';
        b[1] = b'Z';
        b[0x3C] = 0x80;
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        b[0x80 + 24 + 68] = 2;
        b
    }

    pub fn sample_setup() -> Vec<u8> {
        let spec = b"<?xml version=\"1.0\"?><package><metadata><id>DemoApp</id><version>1.2.3</version><title>Demo &amp; Co</title><authors>Demo SA</authors></metadata></package>";
        let gui = gui64();
        let nupkg = stored_zip(&[
            ("DemoApp.nuspec", spec),
            ("lib/net45/demo & co.exe", &gui),
            ("lib/net45/squirrel.exe", &gui),
            ("lib/net45/resources/app.asar", b"asar"),
        ]);
        let outer = stored_zip(&[
            ("DemoApp-1.2.3-full.nupkg", &nupkg),
            ("Update.exe", b"MZ update"),
            ("RELEASES", b"x"),
        ]);
        setup_exe(&outer)
    }

    #[test]
    fn ressource_et_paquet_trouves() {
        let s = sample_setup();
        let (at, _) = find_package(&s).expect("archive DATA/131");
        assert_eq!(&s[at..at + 4], b"PK\x03\x04");
        assert!(find_resource(&s, "DATA", 132).is_none());
        assert_eq!(super::super::detect(&s), Some(super::super::Kind::Squirrel));
    }

    #[test]
    fn description_et_programme_principal() {
        let o = open(&sample_setup()).unwrap();
        assert_eq!(
            o.spec,
            Nuspec {
                id: "DemoApp".into(),
                version: "1.2.3".into(),
                title: "Demo & Co".into(),
                authors: "Demo SA".into()
            }
        );
        let progs = vec![
            ("squirrel.exe".to_string(), 9_999u64),
            ("demo & co.exe".to_string(), 10),
        ];
        assert_eq!(
            main_program(&progs, &o.spec).as_deref(),
            Some("demo & co.exe")
        );
        let progs = vec![
            ("a.exe".to_string(), 5u64),
            ("big.exe".to_string(), 50),
            ("Update.exe".to_string(), 99),
        ];
        assert_eq!(main_program(&progs, &o.spec).as_deref(), Some("big.exe"));
    }

    #[test]
    fn programme_graphique_32_bits_accepte() {
        let gui64 = gui64();
        assert!(is_gui_program(&gui64));
        let mut gui32 = gui64.clone();
        gui32[0x84..0x86].copy_from_slice(&0x14cu16.to_le_bytes());
        assert!(is_gui_program(&gui32), "Discord x86 : programme 32 bits");
        let mut console = gui32.clone();
        console[0x80 + 24 + 68] = 3;
        assert!(!is_gui_program(&console));
        let mut ia64 = gui64.clone();
        ia64[0x84..0x86].copy_from_slice(&0x200u16.to_le_bytes());
        assert!(!is_gui_program(&ia64));
        assert!(!is_gui_program(b"MZ"));
    }

    #[test]
    fn dossier_sous_le_profil() {
        let o = open(&sample_setup()).unwrap();
        let root = default_root(&o.spec);
        assert!(root.starts_with("C:\\Users\\"), "{root}");
        assert!(root.ends_with("\\AppData\\Local\\DemoApp"), "{root}");
        assert_eq!(info(&sample_setup()).unwrap().default_dir, root);
    }
}
