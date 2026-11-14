//! Installeurs « web » : un auto-extractible qui ne porte qu'un petit
//! téléchargeur (Paint.NET : `SetupDownloader.exe`, appli .NET Framework
//! WinForms) chargé d'aller chercher le vrai paquet sur Internet (#896).
//!
//! Le CLR intégré ne sait pas exécuter ces téléchargeurs. Winlux s'appuie
//! sur un catalogue des paquets complets publiés par l'éditeur (version,
//! adresse officielle, SHA-256). Rien n'est téléchargé sans l'accord de
//! l'utilisateur : sans `--allow-download`, le runtime décrit ce qu'il
//! ferait (lignes `download.*` sur la sortie standard) et rend le code 4 ;
//! le lanceur du double-clic affiche la question puis relance.

use super::sevenz::Entry;
use std::path::{Path, PathBuf};

/// Code de sortie : téléchargement à confirmer par l'utilisateur.
pub const NEEDS_CONSENT: i32 = 4;

/// Paquet complet connu, correspondant à un installeur web précis.
pub struct Package {
    /// Nom de produit lu dans les ressources de l'installeur web.
    pub product: &'static str,
    /// Version de fichier exacte de l'installeur web.
    pub file_version: &'static str,
    /// Nom affiché et version publiée.
    pub name: &'static str,
    pub version: &'static str,
    pub publisher: &'static str,
    /// Adresse HTTPS officielle de l'éditeur et empreinte du fichier.
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

/// Paquets complets connus. Une nouvelle version d'un installeur web
/// demande une ligne de plus, empreinte relevée sur le fichier publié.
pub const CATALOG: &[Package] = &[Package {
    product: "paint.net",
    file_version: "5.112.9563.32325",
    name: "Paint.NET",
    version: "5.1.12",
    publisher: "dotPDN LLC",
    url: "https://github.com/paintdotnet/release/releases/download/v5.1.12/paint.net.5.1.12.install.x64.zip",
    sha256: "3cd861b5af3f85bd28666d4a9017d03de237c08ad0103ee9d92b2cd921f8c867",
    size: 84_238_734,
}];

/// L'archive ne porte qu'un téléchargeur (aucun paquet, aucun programme
/// de l'appli) : un `SetupDownloader*.exe` ou `*Downloader*.exe`.
pub fn is_downloader(entries: &[Entry]) -> bool {
    let files: Vec<String> = entries
        .iter()
        .filter(|e| !e.dir)
        .map(|e| e.name.to_ascii_lowercase())
        .collect();
    let downloader = files.iter().any(|n| {
        let base = n.rsplit('\\').next().unwrap_or(n);
        base.ends_with(".exe") && base.contains("downloader")
    });
    let payload = files
        .iter()
        .any(|n| n.ends_with(".msi") || n.ends_with(".msix") || n.ends_with(".cab"));
    downloader && !payload
}

/// Paquet du catalogue correspondant à cet installeur web.
pub fn lookup(sfx: &[u8]) -> Option<&'static Package> {
    let product = crate::netports::version_string(sfx, "ProductName")?;
    let version = crate::netports::version_string(sfx, "FileVersion")?;
    CATALOG.iter().find(|p| {
        p.product.eq_ignore_ascii_case(product.trim()) && p.file_version == version.trim()
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let mut parts = rest.split('/');
    let host = parts.next().unwrap_or_default();
    // github.com/éditeur/dépôt : le dépôt dit qui publie.
    match (host, parts.next(), parts.next()) {
        ("github.com", Some(owner), Some(repo)) => format!("github.com/{owner}/{repo}"),
        _ => host.to_string(),
    }
}

/// Description de ce qui sera téléchargé, pour la question du lanceur.
pub fn describe(p: &Package) -> String {
    format!(
        "download.name={}\ndownload.version={}\ndownload.publisher={}\ndownload.source={}\ndownload.size_mb={}\n",
        p.name,
        p.version,
        p.publisher,
        host(p.url),
        p.size.div_ceil(1024 * 1024)
    )
}

/// Télécharge en HTTPS uniquement (redirections comprises), sans shell ;
/// la taille est bornée à celle du catalogue.
fn download(p: &Package, dest: &Path) -> Result<(), String> {
    let status = std::process::Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-filesize",
            &(p.size + 1).to_string(),
            "--retry",
            "2",
            "--output",
        ])
        .arg(dest)
        .arg(p.url)
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("curl : {e}"))?;
    if !status.success() {
        return Err(format!(
            "téléchargement impossible depuis {} (code {})",
            host(p.url),
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

/// Vérifie l'empreinte du fichier téléchargé.
pub fn verify(p: &Package, data: &[u8]) -> Result<(), String> {
    let got = hex(&crate::dlls::bcrypt::sha256(data));
    if got != p.sha256 {
        return Err(format!(
            "empreinte inattendue pour {} {} : {got} au lieu de {}",
            p.name, p.version, p.sha256
        ));
    }
    Ok(())
}

/// Seul programme d'installation contenu dans le .zip téléchargé.
fn installer_in(dir: &Path) -> Option<PathBuf> {
    let exes: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
        .collect();
    (exes.len() == 1).then(|| exes[0].clone())
}

/// Installeur web : demande l'accord, ou télécharge, vérifie et installe.
pub fn run(sfx: &[u8], exe: &Path, args: &[String], allow_download: bool) -> i32 {
    let Some(p) = lookup(sfx) else {
        let name = crate::netports::version_string(sfx, "ProductName")
            .unwrap_or_else(|| exe.display().to_string());
        eprintln!(
            "setup : « {} » est un installeur web : il ne contient qu'un téléchargeur, que Winlux ne sait pas encore exécuter, et Winlux ne connaît pas le paquet complet de cette version. Téléchargez l'installeur complet (hors ligne) sur le site de l'éditeur.",
            name.trim()
        );
        return 1;
    };
    if !allow_download {
        print!("{}", describe(p));
        eprintln!(
            "setup : cet installeur télécharge {} {} ; Winlux peut récupérer le paquet officiel depuis {} ({} Mo) après votre accord.",
            p.name,
            p.version,
            host(p.url),
            p.size.div_ceil(1024 * 1024)
        );
        return NEEDS_CONSENT;
    }
    let work = crate::paths::setup_work_dir("web");
    let _ = std::fs::remove_dir_all(&work);
    let result = (|| -> Result<i32, String> {
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let zip = work.join("paquet.zip");
        eprintln!(
            "setup : téléchargement de {} {} depuis {}…",
            p.name,
            p.version,
            host(p.url)
        );
        download(p, &zip)?;
        let data = std::fs::read(&zip).map_err(|e| e.to_string())?;
        verify(p, &data)?;
        drop(data);
        eprintln!("setup : empreinte SHA-256 vérifiée");
        let out = work.join("paquet");
        crate::archive::zip::extract(&zip, &out, None, &crate::archive::util::Progress::default())
            .map_err(|e| format!("{e:?}"))?;
        let inner = installer_in(&out)
            .ok_or("le paquet ne contient pas un programme d'installation unique")?;
        // Le paquet complet porte son MSI : installé par le chemin de #733.
        super::sevenz::chain(&inner, args, false)
            .ok_or_else(|| "le paquet téléchargé n'est pas un installeur reconnu".to_string())
    })();
    let _ = std::fs::remove_dir_all(&work);
    match result {
        Ok(rc) => rc,
        Err(e) => {
            eprintln!("setup : échec de l'installation : {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> Entry {
        Entry {
            name: name.into(),
            dir: false,
            data: Vec::new(),
        }
    }

    #[test]
    fn telechargeur_seul_reconnu() {
        let web = [
            entry("SetupShim.exe"),
            entry("x64\\SetupDownloader\\SetupDownloader.exe"),
            entry("x64\\SetupDownloader\\Newtonsoft.Json.dll"),
        ];
        assert!(is_downloader(&web));
        // Installeur complet : il porte son paquet.
        let full = [
            entry("SetupShim.exe"),
            entry("x64\\SetupDownloader\\SetupDownloader.exe"),
            entry("x64\\PaintDotNet_x64.msi"),
        ];
        assert!(!is_downloader(&full));
        assert!(!is_downloader(&[entry("7zFM.exe")]));
    }

    #[test]
    fn description_et_source() {
        let p = &CATALOG[0];
        let d = describe(p);
        assert!(d.contains("download.name=Paint.NET\n"));
        assert!(d.contains("download.version=5.1.12\n"));
        assert!(d.contains("download.source=github.com/paintdotnet/release\n"));
        assert!(d.contains("download.size_mb=81\n"));
        assert!(p.url.starts_with("https://"));
    }

    #[test]
    fn empreinte_refusee() {
        assert!(verify(&CATALOG[0], b"pas le bon fichier").is_err());
    }
}
