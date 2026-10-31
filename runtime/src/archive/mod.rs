//! Archives natives de Winlux : on navigue dans une archive comme dans un
//! dossier, on y ajoute, renomme ou supprime des fichiers, on l'extrait ou on
//! en crée une par clic droit.
//!
//! Fonctionnement : à l'ouverture, l'archive est déployée dans un cache
//! (`~/.cache/winlux/archives/<empreinte>/root`). `paths::to_linux` résout
//! alors `C:\...\photos.zip\dossier\x.jpg` vers ce cache, si bien que toutes
//! les API Win32 de fichiers fonctionnent telles quelles dans l'archive.
//! Après une modification, l'Explorer demande un « commit » : si le contenu
//! du cache a changé, l'archive est reconstruite puis remplacée atomiquement.
//!
//! Codecs écrits de zéro : ZIP (stocké, DEFLATE, ZIP64, ZipCrypto), TAR,
//! gzip. Les formats tiers (7z, RAR, xz, zstd, bzip2, cab…) passent par les
//! outils système s'ils sont installés ; sinon ils ne sont pas proposés.

pub mod crc32;
pub mod deflate;
pub mod ffi;
pub mod gzip;
pub mod inflate;
pub mod jobs;
pub mod tar;
pub mod tools;
pub mod util;
pub mod zip;

use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use util::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
    Gz,
    Bz2,
    Xz,
    Zst,
    SevenZ,
    Rar,
    /// cab, deb, rpm, lzh… : lecture seule via 7-Zip ou bsdtar.
    Other,
}

impl Format {
    pub fn from_name(name: &str) -> Option<Format> {
        let n = name.to_lowercase();
        let ends = |s: &str| n.ends_with(s);
        Some(if ends(".tar.gz") || ends(".tgz") {
            Format::TarGz
        } else if ends(".tar.bz2") || ends(".tbz2") || ends(".tbz") {
            Format::TarBz2
        } else if ends(".tar.xz") || ends(".txz") {
            Format::TarXz
        } else if ends(".tar.zst") || ends(".tzst") {
            Format::TarZst
        } else if ends(".tar") {
            Format::Tar
        } else if ends(".zip") {
            Format::Zip
        } else if ends(".7z") {
            Format::SevenZ
        } else if ends(".rar") {
            Format::Rar
        } else if ends(".gz") {
            Format::Gz
        } else if ends(".bz2") {
            Format::Bz2
        } else if ends(".xz") {
            Format::Xz
        } else if ends(".zst") {
            Format::Zst
        } else if ends(".cab") || ends(".deb") || ends(".rpm") || ends(".lzh") || ends(".arj") {
            Format::Other
        } else {
            return None;
        })
    }

    /// Extension ajoutée lors d'une création d'archive.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::TarBz2 => "tar.bz2",
            Format::TarXz => "tar.xz",
            Format::TarZst => "tar.zst",
            Format::Gz => "gz",
            Format::Bz2 => "bz2",
            Format::Xz => "xz",
            Format::Zst => "zst",
            Format::SevenZ => "7z",
            Format::Rar => "rar",
            Format::Other => "",
        }
    }

    fn filter_tool(
        self,
    ) -> Option<(
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    )> {
        match self {
            Format::TarBz2 | Format::Bz2 => Some(("bzip2", &["-dc"], &["-c"])),
            Format::TarXz | Format::Xz => Some(("xz", &["-dc"], &["-c", "-T0"])),
            Format::TarZst | Format::Zst => Some(("zstd", &["-dc", "-q"], &["-c", "-q", "-T0"])),
            _ => None,
        }
    }

    fn single_file(self) -> bool {
        matches!(self, Format::Gz | Format::Bz2 | Format::Xz | Format::Zst)
    }

    /// (lisible, modifiable) sur cette machine.
    pub fn support(self) -> (bool, bool) {
        match self {
            Format::Zip | Format::Tar | Format::TarGz | Format::Gz => (true, true),
            Format::TarBz2
            | Format::TarXz
            | Format::TarZst
            | Format::Bz2
            | Format::Xz
            | Format::Zst => {
                let ok = tools::find(&[self.filter_tool().unwrap().0]).is_some();
                (ok, ok)
            }
            Format::SevenZ => (
                tools::sevenzip().is_some() || tools::bsdtar().is_some(),
                tools::sevenzip().is_some(),
            ),
            Format::Rar => (
                tools::unrar().is_some()
                    || tools::sevenzip().is_some()
                    || tools::unar().is_some()
                    || tools::bsdtar().is_some(),
                false,
            ),
            Format::Other => (
                tools::sevenzip().is_some() || tools::bsdtar().is_some(),
                false,
            ),
        }
    }
}

// ---------- Extraction ----------

fn external_extract(archive: &Path, dest: &Path, pw: Option<&str>, progress: &Progress) -> Res<()> {
    std::fs::create_dir_all(dest)?;
    let pw_arg = |prefix: &str| format!("{prefix}{}", pw.unwrap_or(""));
    if let Some(sz) = tools::sevenzip() {
        let mut c = Command::new(sz);
        c.arg("x")
            .arg("-y")
            .arg("-bsp1")
            .arg("-bso0")
            .arg(pw_arg("-p"));
        c.arg(format!("-o{}", dest.display()))
            .arg("--")
            .arg(archive);
        return tools::run_archiver(c, pw.is_some(), progress);
    }
    if Format::from_name(&archive.to_string_lossy()) == Some(Format::Rar) {
        if let Some(u) = tools::unrar() {
            let mut c = Command::new(u);
            c.arg("x").arg("-o+").arg("-y");
            c.arg(if pw.is_some() {
                pw_arg("-p")
            } else {
                "-p-".into()
            });
            c.arg("--").arg(archive).arg(format!("{}/", dest.display()));
            return tools::run_archiver(c, pw.is_some(), progress);
        }
        if let Some(u) = tools::unar() {
            let mut c = Command::new(u);
            c.arg("-f").arg("-q").arg("-D").arg("-o").arg(dest);
            if let Some(p) = pw {
                c.arg("-p").arg(p);
            }
            c.arg(archive);
            return tools::run_archiver(c, pw.is_some(), progress);
        }
    }
    if let Some(b) = tools::bsdtar() {
        let mut c = Command::new(b);
        c.arg("-xf").arg(archive).arg("-C").arg(dest);
        if let Some(p) = pw {
            c.arg("--passphrase").arg(p);
        }
        return tools::run_archiver(c, pw.is_some(), progress);
    }
    Err(ArcError::Unsupported(
        "aucun outil d'archive installé".into(),
    ))
}

fn stem_of(archive: &Path) -> String {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    for ext in [".gz", ".bz2", ".xz", ".zst"] {
        if lower.ends_with(ext) && name.len() > ext.len() {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name
}

/// Extrait `archive` dans `dest` (créé au besoin).
pub fn extract_to(archive: &Path, dest: &Path, pw: Option<&str>, progress: &Progress) -> Res<()> {
    let fmt = Format::from_name(&archive.to_string_lossy())
        .ok_or_else(|| ArcError::Unsupported("format inconnu".into()))?;
    if !fmt.support().0 {
        return Err(ArcError::Unsupported(format!(
            "outil manquant pour .{}",
            fmt.extension()
        )));
    }
    std::fs::create_dir_all(dest)?;
    let size = std::fs::metadata(archive)?.len();
    let dummy = Progress::default();
    match fmt {
        Format::Zip => {
            if zip::needs_external(archive)? {
                if tools::sevenzip().is_none() && tools::bsdtar().is_none() {
                    return Err(ArcError::Unsupported(
                        "ZIP chiffré en AES ou compressé en LZMA/BZip2 : 7-Zip requis".into(),
                    ));
                }
                return external_extract(archive, dest, pw, progress);
            }
            zip::extract(archive, dest, pw, progress)
        }
        Format::Tar => {
            progress.set_total(size.max(1));
            let f = std::fs::File::open(archive)?;
            tar::extract(BufReader::new(Counted { inner: f, progress }), dest, &dummy)
        }
        Format::TarGz | Format::Gz => {
            progress.set_total(size.max(1));
            let f = std::fs::File::open(archive)?;
            let gz = gzip::GzDecoder::new(BufReader::with_capacity(
                256 * 1024,
                Counted { inner: f, progress },
            ))?;
            if fmt == Format::Gz {
                let mut out = BufWriter::new(std::fs::File::create(dest.join(stem_of(archive)))?);
                std::io::copy(&mut BufReader::new(gz), &mut out)?;
                out.flush()?;
                Ok(())
            } else {
                tar::extract(BufReader::new(gz), dest, &dummy)
            }
        }
        Format::TarBz2
        | Format::TarXz
        | Format::TarZst
        | Format::Bz2
        | Format::Xz
        | Format::Zst => {
            progress.set_total(size.max(1));
            let (name, dec, _) = fmt.filter_tool().unwrap();
            let tool = tools::find(&[name]).unwrap();
            let (mut child, out, feeder) = tools::filter_read(&tool, dec, archive, progress)?;
            let r = if fmt.single_file() {
                let mut w = BufWriter::new(std::fs::File::create(dest.join(stem_of(archive)))?);
                std::io::copy(&mut BufReader::new(out), &mut w)
                    .map(|_| ())
                    .map_err(ArcError::from)
                    .and_then(|_| w.flush().map_err(ArcError::from))
            } else {
                tar::extract(BufReader::new(out), dest, &dummy)
            };
            let _ = feeder.join();
            let st = child.wait()?;
            r?;
            if !st.success() {
                return Err(ArcError::Io(std::io::Error::other(format!(
                    "{name} a échoué"
                ))));
            }
            Ok(())
        }
        Format::SevenZ | Format::Rar | Format::Other => {
            external_extract(archive, dest, pw, progress)
        }
    }
}

// ---------- Création ----------

/// Construit la liste (nom dans l'archive, chemin) des sources, récursivement.
fn collect_items(sources: &[PathBuf]) -> Res<Vec<(String, PathBuf)>> {
    let mut items = Vec::new();
    for s in sources {
        let base = s
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| ArcError::Unsupported("source invalide".into()))?;
        let meta = std::fs::symlink_metadata(s)?;
        items.push((base.clone(), s.clone()));
        if meta.is_dir() {
            for e in walk(s)? {
                items.push((format!("{base}/{}", archive_name(&e.rel)), s.join(&e.rel)));
            }
        }
    }
    Ok(items)
}

/// Crée l'archive `out` (format déduit de l'extension) à partir de `sources`.
pub fn create(out: &Path, sources: &[PathBuf], pw: Option<&str>, progress: &Progress) -> Res<()> {
    let fmt = Format::from_name(&out.to_string_lossy())
        .ok_or_else(|| ArcError::Unsupported("format inconnu".into()))?;
    if !fmt.support().1 {
        return Err(ArcError::ReadOnly);
    }
    if pw.is_some() && !matches!(fmt, Format::Zip | Format::SevenZ) {
        return Err(ArcError::Unsupported(
            "mot de passe : seuls ZIP et 7z le permettent".into(),
        ));
    }
    match fmt {
        Format::Zip => zip::create(out, &collect_items(sources)?, pw, progress),
        Format::Tar => {
            let w = BufWriter::new(std::fs::File::create(out)?);
            tar::create(w, &collect_items(sources)?, progress)?
                .flush()
                .map_err(Into::into)
        }
        Format::TarGz => {
            let w = BufWriter::new(std::fs::File::create(out)?);
            let gz = gzip::GzEncoder::new(w)?;
            tar::create(gz, &collect_items(sources)?, progress)?
                .finish()?
                .flush()
                .map_err(Into::into)
        }
        Format::Gz => {
            let [src] = sources else {
                return Err(ArcError::Unsupported("un seul fichier par .gz".into()));
            };
            progress.set_total(std::fs::metadata(src)?.len().max(1));
            let mut gz = gzip::GzEncoder::new(BufWriter::new(std::fs::File::create(out)?))?;
            std::io::copy(
                &mut Counted {
                    inner: std::fs::File::open(src)?,
                    progress,
                },
                &mut gz,
            )?;
            gz.finish()?.flush().map_err(Into::into)
        }
        Format::TarBz2
        | Format::TarXz
        | Format::TarZst
        | Format::Bz2
        | Format::Xz
        | Format::Zst => {
            let (name, _, enc) = fmt.filter_tool().unwrap();
            let tool = tools::find(&[name]).unwrap();
            let mut child = tools::filter_write(&tool, enc, out)?;
            let stdin = child.stdin.take().unwrap();
            let r = if fmt.single_file() {
                match sources {
                    [src] => {
                        progress.set_total(std::fs::metadata(src)?.len().max(1));
                        let mut w = BufWriter::new(stdin);
                        std::io::copy(
                            &mut Counted {
                                inner: std::fs::File::open(src)?,
                                progress,
                            },
                            &mut w,
                        )
                        .map(|_| ())
                        .map_err(ArcError::from)
                    }
                    _ => Err(ArcError::Unsupported(
                        "un seul fichier par fichier compressé".into(),
                    )),
                }
            } else {
                tar::create(BufWriter::new(stdin), &collect_items(sources)?, progress)
                    .and_then(|mut w| w.flush().map_err(Into::into))
            };
            let st = child.wait()?;
            r?;
            if !st.success() {
                return Err(ArcError::Io(std::io::Error::other(format!(
                    "{name} a échoué"
                ))));
            }
            Ok(())
        }
        Format::SevenZ => {
            let sz = tools::sevenzip().ok_or(ArcError::ReadOnly)?;
            let _ = std::fs::remove_file(out);
            let mut c = Command::new(sz);
            c.arg("a").arg("-t7z").arg("-y").arg("-bsp1").arg("-bso0");
            if let Some(p) = pw {
                c.arg(format!("-p{p}")).arg("-mhe=on");
            }
            c.arg("--").arg(out);
            for s in sources {
                c.arg(s);
            }
            tools::run_archiver(c, pw.is_some(), progress)
        }
        Format::Rar | Format::Other => Err(ArcError::ReadOnly),
    }
}

// ---------- Cache des archives ouvertes ----------

fn cache_base() -> PathBuf {
    if let Some(d) = std::env::var_os("WINLUX_ARCHIVE_CACHE") {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(d).join("winlux/archives");
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cache/winlux/archives"))
        .unwrap_or_else(|| std::env::temp_dir().join("winlux-archives"))
}

fn fnv(s: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in s {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn cache_dir(archive: &Path) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    cache_base().join(format!("{:016x}", fnv(archive.as_os_str().as_bytes())))
}

fn stamp(archive: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(archive).ok()?;
    Some(format!(
        "{} {} {}\n{}",
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        archive.display()
    ))
}

fn manifest(root: &Path) -> String {
    use std::os::unix::fs::MetadataExt;
    let mut s = String::new();
    if let Ok(list) = walk(root) {
        for e in list {
            let kind = if e.meta.is_dir() {
                'd'
            } else if e.meta.file_type().is_symlink() {
                'l'
            } else {
                'f'
            };
            s.push_str(&format!(
                "{}\t{kind}\t{}\t{}.{}\t{:o}\n",
                e.rel.display(),
                e.meta.len(),
                e.meta.mtime(),
                e.meta.mtime_nsec(),
                e.meta.mode() & 0o777
            ));
        }
    }
    s
}

/// Archives ouvertes par ce processus avec un mot de passe (pour le commit).
fn passwords() -> &'static Mutex<HashMap<PathBuf, String>> {
    static P: std::sync::OnceLock<Mutex<HashMap<PathBuf, String>>> = std::sync::OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Verrou global : jamais deux reconstructions ou ouvertures simultanées.
static LOCK: Mutex<()> = Mutex::new(());

/// Le cache de cette archive est-il à jour ?
pub fn cache_valid(archive: &Path) -> bool {
    let d = cache_dir(archive);
    match (std::fs::read_to_string(d.join("stamp")), stamp(archive)) {
        (Ok(a), Some(b)) => a == b && d.join("root").is_dir(),
        _ => false,
    }
}

/// Déploie l'archive dans le cache (si nécessaire) et renvoie sa racine.
pub fn open(archive: &Path, pw: Option<&str>, progress: &Progress) -> Res<PathBuf> {
    let archive = std::fs::canonicalize(archive)?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = cache_dir(&archive);
    if cache_valid(&archive) {
        return Ok(d.join("root"));
    }
    let tmp = d.join("root.tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    if let Err(e) = extract_to(&archive, &tmp, pw, progress) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let root = d.join("root");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::rename(&tmp, &root)?;
    std::fs::write(d.join("manifest"), manifest(&root))?;
    std::fs::write(d.join("stamp"), stamp(&archive).unwrap_or_default())?;
    if let Some(p) = pw {
        passwords()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(archive.clone(), p.to_string());
    }
    Ok(root)
}

/// Reconstruit l'archive si son contenu déployé a changé. Renvoie `true`
/// si l'archive a été réécrite.
pub fn commit(archive: &Path, progress: &Progress) -> Res<bool> {
    let archive = std::fs::canonicalize(archive)?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !cache_valid(&archive) {
        return Ok(false);
    }
    let d = cache_dir(&archive);
    let root = d.join("root");
    let now = manifest(&root);
    if std::fs::read_to_string(d.join("manifest")).ok().as_deref() == Some(now.as_str()) {
        return Ok(false);
    }
    let fmt = Format::from_name(&archive.to_string_lossy()).unwrap_or(Format::Other);
    let pw = passwords()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&archive)
        .cloned();
    if !fmt.support().1 {
        // Format en lecture seule : on restaure le contenu d'origine.
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        extract_to(&archive, &root, pw.as_deref(), &Progress::default())?;
        std::fs::write(d.join("manifest"), manifest(&root))?;
        return Err(ArcError::ReadOnly);
    }
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&root)?
        .flatten()
        .map(|e| e.path())
        .collect();
    sources.sort();
    if fmt.single_file() && sources.len() != 1 {
        return Err(ArcError::Unsupported(
            "un fichier compressé ne contient qu'un seul fichier".into(),
        ));
    }
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = archive.with_file_name(format!(".{name}.winlux-{}", std::process::id()));
    // On garde l'extension pour que le format soit reconnu.
    let tmp = tmp.with_file_name(format!(
        "{}.{}",
        tmp.file_name().unwrap().to_string_lossy(),
        fmt.extension()
    ));
    let result = if fmt == Format::Zip || fmt == Format::SevenZ || fmt.single_file() {
        // Le contenu est à la racine de l'archive, sans dossier englobant.
        if fmt == Format::Zip {
            let items: Vec<_> = walk(&root)?
                .into_iter()
                .map(|e| (archive_name(&e.rel), root.join(&e.rel)))
                .collect();
            zip::create(&tmp, &items, pw.as_deref(), progress)
        } else {
            create(&tmp, &sources, pw.as_deref(), progress)
        }
    } else {
        let items: Vec<_> = walk(&root)?
            .into_iter()
            .map(|e| (archive_name(&e.rel), root.join(&e.rel)))
            .collect();
        write_tar_family(fmt, &tmp, &items, progress)
    };
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Ok(m) = std::fs::metadata(&archive) {
        let _ = std::fs::set_permissions(&tmp, m.permissions());
    }
    std::fs::rename(&tmp, &archive)?;
    std::fs::write(d.join("manifest"), manifest(&root))?;
    std::fs::write(d.join("stamp"), stamp(&archive).unwrap_or_default())?;
    Ok(true)
}

fn write_tar_family(
    fmt: Format,
    out: &Path,
    items: &[(String, PathBuf)],
    progress: &Progress,
) -> Res<()> {
    match fmt {
        Format::Tar => {
            let w = BufWriter::new(std::fs::File::create(out)?);
            tar::create(w, items, progress)?.flush().map_err(Into::into)
        }
        Format::TarGz => {
            let gz = gzip::GzEncoder::new(BufWriter::new(std::fs::File::create(out)?))?;
            tar::create(gz, items, progress)?
                .finish()?
                .flush()
                .map_err(Into::into)
        }
        _ => {
            let (name, _, enc) = fmt.filter_tool().ok_or(ArcError::ReadOnly)?;
            let tool = tools::find(&[name]).ok_or(ArcError::ReadOnly)?;
            let mut child = tools::filter_write(&tool, enc, out)?;
            let stdin = child.stdin.take().unwrap();
            let r = tar::create(BufWriter::new(stdin), items, progress)
                .and_then(|mut w| w.flush().map_err(Into::into));
            let st = child.wait()?;
            r?;
            if st.success() {
                Ok(())
            } else {
                Err(ArcError::Io(std::io::Error::other(format!(
                    "{name} a échoué"
                ))))
            }
        }
    }
}

/// Résout un chemin qui traverse une ou plusieurs archives ouvertes :
/// `/home/u/a.zip/doc/x.txt` → `<cache>/root/doc/x.txt`.
pub fn resolve(path: &Path) -> Option<PathBuf> {
    resolve_depth(path, 0)
}

fn resolve_depth(path: &Path, depth: u32) -> Option<PathBuf> {
    if depth > 8 {
        return None;
    }
    let mut prefix = PathBuf::new();
    let comps: Vec<_> = path.components().collect();
    for (i, c) in comps.iter().enumerate() {
        prefix.push(c.as_os_str());
        if i + 1 == comps.len() {
            break;
        }
        if Format::from_name(&c.as_os_str().to_string_lossy()).is_none() {
            continue;
        }
        let Ok(m) = std::fs::metadata(&prefix) else {
            return None;
        };
        if !m.is_file() {
            continue;
        }
        let canon = std::fs::canonicalize(&prefix).ok()?;
        if !cache_valid(&canon) {
            return None;
        }
        let mut mapped = cache_dir(&canon).join("root");
        for rest in &comps[i + 1..] {
            mapped.push(rest.as_os_str());
        }
        if mapped.exists() {
            return Some(mapped);
        }
        return resolve_depth(&mapped, depth + 1).or(Some(mapped));
    }
    None
}

/// Archives traversées par `path` (de la plus extérieure à la plus intérieure).
pub fn chain(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut cur = path.to_path_buf();
    for _ in 0..8 {
        let mut prefix = PathBuf::new();
        let comps: Vec<_> = cur.components().collect();
        let mut next = None;
        for (i, c) in comps.iter().enumerate() {
            prefix.push(c.as_os_str());
            if Format::from_name(&c.as_os_str().to_string_lossy()).is_none() || !prefix.is_file() {
                continue;
            }
            let Ok(canon) = std::fs::canonicalize(&prefix) else {
                break;
            };
            if !cache_valid(&canon) {
                break;
            }
            out.push(canon.clone());
            let mut mapped = cache_dir(&canon).join("root");
            for rest in &comps[i + 1..] {
                mapped.push(rest.as_os_str());
            }
            next = Some(mapped);
            break;
        }
        match next {
            Some(n) => cur = n,
            None => break,
        }
    }
    out
}

/// Chemin de destination libre : `base`, puis `base (2)`, `base (3)`…
pub fn unique_path(base: &Path) -> PathBuf {
    if std::fs::symlink_metadata(base).is_err() {
        return base.to_path_buf();
    }
    let name = base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match Format::from_name(&name) {
        Some(f) if !f.extension().is_empty() && name.len() > f.extension().len() + 1 => {
            let e = f.extension();
            (&name[..name.len() - e.len() - 1], format!(".{e}"))
        }
        _ => (name.as_str(), String::new()),
    };
    for k in 2..1000 {
        let p = base.with_file_name(format!("{stem} ({k}){ext}"));
        if std::fs::symlink_metadata(&p).is_err() {
            return p;
        }
    }
    base.to_path_buf()
}

/// Nom de dossier par défaut pour « Extraire tout » : `photos.tar.gz` → `photos`.
pub fn default_folder_name(archive: &Path) -> String {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".into());
    match Format::from_name(&name) {
        Some(f) if !f.extension().is_empty() && name.len() > f.extension().len() + 1 => {
            name[..name.len() - f.extension().len() - 1].to_string()
        }
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("winlux-arc-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn formats() {
        assert_eq!(Format::from_name("a.TAR.GZ"), Some(Format::TarGz));
        assert_eq!(Format::from_name("a.tgz"), Some(Format::TarGz));
        assert_eq!(Format::from_name("a.gz"), Some(Format::Gz));
        assert_eq!(Format::from_name("a.7z"), Some(Format::SevenZ));
        assert_eq!(Format::from_name("a.txt"), None);
        assert_eq!(default_folder_name(Path::new("/x/photos.tar.gz")), "photos");
        assert_eq!(default_folder_name(Path::new("/x/a.zip")), "a");
    }

    #[test]
    fn open_modify_commit_cycle() {
        let d = tmp("cycle");
        std::env::set_var("WINLUX_ARCHIVE_CACHE", d.join("cache"));
        let src = d.join("projet");
        std::fs::create_dir_all(src.join("doc")).unwrap();
        std::fs::write(src.join("doc/a.txt"), "un").unwrap();
        for ext in ["zip", "tar", "tar.gz"] {
            let arc = d.join(format!("p.{ext}"));
            create(&arc, std::slice::from_ref(&src), None, &Progress::default()).unwrap();
            let root = open(&arc, None, &Progress::default()).unwrap();
            assert_eq!(
                std::fs::read_to_string(root.join("projet/doc/a.txt")).unwrap(),
                "un"
            );
            // Le chemin « dans » l'archive se résout vers le cache.
            let virt = arc.join("projet/doc/a.txt");
            assert_eq!(resolve(&virt).unwrap(), root.join("projet/doc/a.txt"));
            assert!(!commit(&arc, &Progress::default()).unwrap());
            // Modification : ajout, renommage, suppression.
            std::fs::write(root.join("projet/nouveau.txt"), "deux").unwrap();
            std::fs::rename(root.join("projet/doc"), root.join("projet/docs")).unwrap();
            assert!(commit(&arc, &Progress::default()).unwrap());
            assert_eq!(chain(&virt), vec![std::fs::canonicalize(&arc).unwrap()]);
            let out = d.join(format!("verif-{ext}"));
            extract_to(&arc, &out, None, &Progress::default()).unwrap();
            assert_eq!(
                std::fs::read_to_string(out.join("projet/nouveau.txt")).unwrap(),
                "deux"
            );
            assert!(out.join("projet/docs/a.txt").is_file());
            assert!(!out.join("projet/doc").exists());
        }
        std::env::remove_var("WINLUX_ARCHIVE_CACHE");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn external_filters_when_available() {
        let d = tmp("ext");
        let src = d.join("f.txt");
        std::fs::write(&src, "contenu ".repeat(1000)).unwrap();
        for ext in ["tar.xz", "tar.bz2", "xz", "bz2", "gz"] {
            let fmt = Format::from_name(ext).unwrap_or(Format::Gz);
            let fmt = Format::from_name(&format!("a.{ext}")).unwrap_or(fmt);
            if !fmt.support().1 {
                continue;
            }
            let arc = d.join(format!("a.{ext}"));
            create(&arc, std::slice::from_ref(&src), None, &Progress::default()).unwrap();
            let out = d.join(format!("o-{ext}"));
            extract_to(&arc, &out, None, &Progress::default()).unwrap();
            let name = if fmt.single_file() { "a" } else { "f.txt" };
            assert_eq!(
                std::fs::read_to_string(out.join(name)).unwrap(),
                "contenu ".repeat(1000),
                "{ext}"
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn unique_names() {
        let d = tmp("uniq");
        std::fs::write(d.join("a.tar.gz"), "").unwrap();
        assert_eq!(unique_path(&d.join("a.tar.gz")), d.join("a (2).tar.gz"));
        assert_eq!(unique_path(&d.join("b.zip")), d.join("b.zip"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
