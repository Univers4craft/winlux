//! Fichiers texte entre Windows et Linux : détection de l'encodage (BOM
//! UTF-8/UTF-16, UTF-16 sans BOM, UTF-8, page ANSI de la locale) et des fins
//! de ligne CRLF, conversion sans perdre les accents.
//!
//! `winlux --text linux FICHIER…`   → UTF-8, fins de ligne LF
//! `winlux --text windows [--utf16|--ansi] FICHIER…` → CRLF (UTF-8 par défaut)
//! `winlux --text info FICHIER…`    → encodage et fins de ligne détectés

use super::codepage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// Page de code à un octet (1252, 850…).
    Ansi(u32),
}

impl Encoding {
    pub fn label(&self) -> String {
        match self {
            Encoding::Utf8 => "UTF-8".into(),
            Encoding::Utf8Bom => "UTF-8 (BOM)".into(),
            Encoding::Utf16Le => "UTF-16 LE".into(),
            Encoding::Utf16Be => "UTF-16 BE".into(),
            Encoding::Ansi(cp) => format!("Windows-{cp}"),
        }
    }
}

/// Devine l'encodage d'un fichier texte.
pub fn detect(data: &[u8], ansi_cp: u32) -> Encoding {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Encoding::Utf8Bom;
    }
    if data.starts_with(&[0xFF, 0xFE]) {
        return Encoding::Utf16Le;
    }
    if data.starts_with(&[0xFE, 0xFF]) {
        return Encoding::Utf16Be;
    }
    // UTF-16 sans BOM : texte surtout ASCII, un octet nul sur deux.
    if data.len() >= 4 && data.len().is_multiple_of(2) {
        let pairs = data.len() / 2;
        let odd_zero = data.chunks(2).filter(|c| c[1] == 0 && c[0] != 0).count();
        let even_zero = data.chunks(2).filter(|c| c[0] == 0 && c[1] != 0).count();
        if odd_zero * 10 >= pairs * 7 {
            return Encoding::Utf16Le;
        }
        if even_zero * 10 >= pairs * 7 {
            return Encoding::Utf16Be;
        }
    }
    if std::str::from_utf8(data).is_ok() {
        Encoding::Utf8
    } else {
        Encoding::Ansi(ansi_cp)
    }
}

/// Décode un fichier selon l'encodage détecté.
pub fn decode(data: &[u8], enc: Encoding) -> String {
    let utf16 = |body: &[u8], le: bool| -> String {
        let units: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match enc {
        Encoding::Utf8 => String::from_utf8_lossy(data).into_owned(),
        Encoding::Utf8Bom => String::from_utf8_lossy(&data[3..]).into_owned(),
        Encoding::Utf16Le => utf16(data.strip_prefix(&[0xFF, 0xFE]).unwrap_or(data), true),
        Encoding::Utf16Be => utf16(data.strip_prefix(&[0xFE, 0xFF]).unwrap_or(data), false),
        Encoding::Ansi(cp) => codepage::decode(cp, data, false).unwrap_or_default(),
    }
}

/// Fins de ligne : (CRLF, LF seuls).
pub fn line_endings(text: &str) -> (usize, usize) {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    (crlf, lf)
}

/// Texte pour Linux : UTF-8 sans BOM, fins de ligne LF.
pub fn to_linux(data: &[u8], ansi_cp: u32) -> Vec<u8> {
    let text = decode(data, detect(data, ansi_cp));
    text.replace("\r\n", "\n").into_bytes()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Utf8,
    Utf16,
    Ansi(u32),
}

/// Texte pour Windows : fins de ligne CRLF dans l'encodage voulu. Renvoie
/// aussi le nombre de caractères absents de la page ANSI (remplacés par ?).
pub fn to_windows(data: &[u8], ansi_cp: u32, target: Target) -> (Vec<u8>, usize) {
    let text = decode(data, detect(data, ansi_cp));
    let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
    match target {
        Target::Utf8 => (text.into_bytes(), 0),
        Target::Utf16 => {
            let mut out = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                out.extend_from_slice(&u.to_le_bytes());
            }
            (out, 0)
        }
        Target::Ansi(cp) => {
            let lost = text
                .chars()
                .filter(|&c| codepage::encode_char(cp, c).is_none())
                .count();
            let enc = codepage::encode(cp, &text, b'?', true).unwrap_or(codepage::Encoded {
                bytes: Vec::new(),
                used_default: false,
            });
            (enc.bytes, lost)
        }
    }
}

fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".winlux-tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path)
}

/// Commande `winlux --text`.
pub fn cli(args: &[String]) -> i32 {
    let ansi = codepage::resolve(codepage::CP_ACP);
    let Some((mode, rest)) = args.split_first() else {
        eprintln!("usage : winlux --text linux|windows|info [--utf16|--ansi] FICHIER…");
        return 2;
    };
    let mut target = Target::Utf8;
    let mut files = Vec::new();
    for a in rest {
        match a.as_str() {
            "--utf16" => target = Target::Utf16,
            "--ansi" => target = Target::Ansi(ansi),
            "--utf8" => target = Target::Utf8,
            f => files.push(std::path::PathBuf::from(f)),
        }
    }
    if files.is_empty() || !matches!(mode.as_str(), "linux" | "windows" | "info") {
        eprintln!("usage : winlux --text linux|windows|info [--utf16|--ansi] FICHIER…");
        return 2;
    }
    let mut status = 0;
    for f in &files {
        let data = match std::fs::read(f) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{} : {e}", f.display());
                status = 1;
                continue;
            }
        };
        if data.contains(&0) && detect(&data, ansi) == Encoding::Utf8 {
            eprintln!("{} : fichier binaire, ignoré", f.display());
            status = 1;
            continue;
        }
        match mode.as_str() {
            "info" => {
                let enc = detect(&data, ansi);
                let (crlf, lf) = line_endings(&decode(&data, enc));
                let eol = match (crlf, lf) {
                    (0, 0) => "aucune",
                    (_, 0) => "CRLF (Windows)",
                    (0, _) => "LF (Linux)",
                    _ => "mélangées",
                };
                println!("{}\t{}\t{eol}", f.display(), enc.label());
            }
            "linux" => {
                if let Err(e) = write_atomic(f, &to_linux(&data, ansi)) {
                    eprintln!("{} : {e}", f.display());
                    status = 1;
                }
            }
            _ => {
                let (out, lost) = to_windows(&data, ansi, target);
                if lost > 0 {
                    eprintln!(
                        "{} : {lost} caractère(s) absent(s) de Windows-{ansi}, remplacé(s) par « ? »",
                        f.display()
                    );
                }
                if let Err(e) = write_atomic(f, &out) {
                    eprintln!("{} : {e}", f.display());
                    status = 1;
                }
            }
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloc_notes_windows_vers_linux() {
        // « Été\r\nçà » en Windows-1252.
        let ansi = b"\xc9t\xe9\r\n\xe7\xe0";
        assert_eq!(detect(ansi, 1252), Encoding::Ansi(1252));
        assert_eq!(to_linux(ansi, 1252), "Été\nçà".as_bytes());
        // Même texte en UTF-16 LE avec BOM (Bloc-notes « Unicode »).
        let mut u16le = vec![0xFF, 0xFE];
        for u in "Été\r\nçà".encode_utf16() {
            u16le.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(detect(&u16le, 1252), Encoding::Utf16Le);
        assert_eq!(to_linux(&u16le, 1252), "Été\nçà".as_bytes());
        // UTF-16 sans BOM.
        let bare: Vec<u8> = "Bonjour\r\n"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(detect(&bare, 1252), Encoding::Utf16Le);
        // UTF-8 avec BOM.
        assert_eq!(
            to_linux("\u{feff}Noël\r\n".as_bytes(), 1252),
            "Noël\n".as_bytes()
        );
    }

    #[test]
    fn linux_vers_windows() {
        let linux = "Été\nçà\n".as_bytes();
        let (utf8, _) = to_windows(linux, 1252, Target::Utf8);
        assert_eq!(utf8, "Été\r\nçà\r\n".as_bytes());
        let (ansi, lost) = to_windows(linux, 1252, Target::Ansi(1252));
        assert_eq!(
            (ansi.as_slice(), lost),
            (&b"\xc9t\xe9\r\n\xe7\xe0\r\n"[..], 0)
        );
        let (u16, _) = to_windows(linux, 1252, Target::Utf16);
        assert_eq!(&u16[..4], &[0xFF, 0xFE, 0xC9, 0x00]);
        // Déjà en CRLF : pas de « \r\r\n ».
        let (again, _) = to_windows(&utf8, 1252, Target::Utf8);
        assert_eq!(again, utf8);
        let (_, lost) = to_windows("日本".as_bytes(), 1252, Target::Ansi(1252));
        assert_eq!(lost, 2);
        assert_eq!(line_endings("a\r\nb\nc"), (1, 1));
    }
}
