//! Contrat de licence d'un installateur (#2123, #2124, #2125, #2126).
//!
//! Sous Windows, un paquet qui porte un contrat de licence l'affiche avant
//! de copier quoi que ce soit, et l'installation ne continue que si
//! l'utilisateur l'accepte. L'assistant d'installation de Winlux fait de
//! même : `winlux --setup --license FICHIER` écrit ici le texte du contrat
//! (RTF converti en texte), quel que soit le type de paquet :
//! - .msi : contrôle `ScrollableText` de la boîte de licence (table Control) ;
//! - NSIS : page `license` (`LicenseData`) de l'en-tête ;
//! - Inno Setup : `LicenseFile` (champ LicenseText de l'en-tête) ;
//! - bundle WiX Burn : `LicenseFile` ou `LicenseUrl` de l'interface standard.
//!
//! Le format RTF est lu d'après la spécification publique « Rich Text
//! Format (RTF) Specification 1.9.1 ».

use std::path::Path;

/// Texte du contrat porté par le paquet, s'il en porte un.
pub fn of_package(path: &Path) -> Option<String> {
    let text = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("msi"))
    {
        crate::msi::install::license_text(path)?
    } else if let Some(t) = super::burn::license(path) {
        t
    } else {
        let data = std::fs::read(path).ok()?;
        match super::detect(&data)? {
            super::Kind::Nsis => super::nsis::license(&data)?,
            super::Kind::Inno => super::inno::license(&data)?,
            _ => return None,
        }
    };
    let text = tidy(&text);
    (!text.trim().is_empty()).then_some(text)
}

/// Installation refusée par l'utilisateur (ERROR_INSTALL_USEREXIT).
pub const REFUSED: i32 = 1602;

/// Option d'installation sans interface : `/S` (NSIS), `/SILENT` et
/// `/VERYSILENT` (Inno), `/quiet`, `/passive`, `/q…` (Burn, msiexec).
/// Comme sous Windows, celui qui la passe accepte le contrat du paquet.
pub fn silent_flag(args: &[String]) -> bool {
    args.iter().any(|a| {
        let l = a.to_ascii_lowercase();
        let Some(o) = l.strip_prefix('/').or_else(|| l.strip_prefix('-')) else {
            return false;
        };
        matches!(o, "s" | "silent" | "verysilent" | "quiet" | "passive") || o.starts_with('q')
    })
}

/// Lancé par un installateur déjà accepté (programme lancé par NSIS, Inno,
/// Burn : `WINLUX_SETUP_DEPTH`), un sous-installateur suit son parent.
fn nested() -> bool {
    std::env::var("WINLUX_SETUP_DEPTH")
        .ok()
        .and_then(|d| d.parse::<u32>().ok())
        .is_some_and(|d| d > 0)
}

/// Installation demandée avec interface (ni option silencieuse, ni
/// assistant) d'un paquet qui porte un contrat : Winlux n'a pas d'interface
/// pour le faire accepter ici, l'installation est refusée net, avec un
/// message, plutôt que faite sans acceptation (#2123). Rend le code à
/// renvoyer, `None` si l'installation peut se faire.
pub fn refuse_unaccepted(path: &Path, args: &[String]) -> Option<i32> {
    if silent_flag(args) || nested() {
        return None;
    }
    of_package(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    eprintln!(
        "winlux : « {name} » contient un contrat de licence à accepter. Ouvrez-le par \
         double-clic (assistant d'installation), ou lisez-le avec `winlux --setup --license` \
         puis installez sans interface (/S, /VERYSILENT, /quiet ou /qn), ce qui vaut \
         acceptation. Rien n'a été installé (code {REFUSED})."
    );
    Some(REFUSED)
}

/// Texte lu dans un fichier de licence (octets bruts) : RTF, UTF-16 ou
/// UTF-8 avec marque, UTF-8, sinon la page de code Windows 1252.
pub fn from_bytes(b: &[u8]) -> String {
    if let Some(rest) = b.strip_prefix(b"\xEF\xBB\xBF") {
        return from_text(&String::from_utf8_lossy(rest));
    }
    if let Some(rest) = b.strip_prefix(b"\xFF\xFE") {
        return from_text(&super::utf16_text(rest, true));
    }
    if is_rtf(&String::from_utf8_lossy(&b[..b.len().min(8)])) {
        let latin: String = b.iter().map(|&c| c as char).collect();
        return rtf_to_text(&latin, true);
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => crate::intl::codepage::decode(1252, b, false).unwrap_or_default(),
    }
}

/// Texte déjà décodé : RTF converti, texte brut gardé tel quel.
pub fn from_text(s: &str) -> String {
    if is_rtf(s) {
        rtf_to_text(s, false)
    } else {
        s.to_string()
    }
}

fn is_rtf(s: &str) -> bool {
    s.trim_start().starts_with("{\\rtf")
}

/// Fins de ligne unifiées, espaces de fin retirés, pas plus d'une ligne
/// vide de suite.
fn tidy(s: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in s.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 || out.is_empty() {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// Groupes dont le contenu n'est pas du texte affiché.
const SKIPPED: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "header",
    "headerl",
    "headerr",
    "headerf",
    "footer",
    "footerl",
    "footerr",
    "footerf",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "generator",
    "xmlnstbl",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "object",
    "filetbl",
    "revtbl",
    "fldinst",
    "bkmkstart",
    "bkmkend",
];

#[derive(Clone, Copy)]
struct Group {
    skip: bool,
    uc: usize,
}

/// RTF → texte. `bytes` : chaque caractère de `s` est un octet du fichier
/// (texte littéral à décoder dans la page de code du document).
pub fn rtf_to_text(s: &str, bytes: bool) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut cp = 1252u32;
    let mut stack: Vec<Group> = Vec::new();
    let mut g = Group { skip: false, uc: 1 };
    // Caractères de repli à sauter après un \uN.
    let mut skip_fallback = 0usize;
    let mut i = 0;
    let flush = |pending: &mut Vec<u8>, out: &mut String, cp: u32| {
        if !pending.is_empty() {
            let cp = crate::intl::codepage::resolve(cp);
            match crate::intl::codepage::decode(cp, pending, false) {
                Ok(t) => out.push_str(&t),
                Err(()) => out.extend(pending.iter().map(|&b| b as char)),
            }
            pending.clear();
        }
    };
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '{' => {
                flush(&mut pending, &mut out, cp);
                stack.push(g);
                skip_fallback = 0;
            }
            '}' => {
                flush(&mut pending, &mut out, cp);
                g = stack.pop().unwrap_or(g);
                skip_fallback = 0;
            }
            '\\' => {
                let Some(&n) = chars.get(i) else { break };
                if n == '\'' {
                    let hex: String = chars.iter().skip(i + 1).take(2).collect();
                    i += 3;
                    if skip_fallback > 0 {
                        skip_fallback -= 1;
                    } else if let (false, Ok(b)) = (g.skip, u8::from_str_radix(&hex, 16)) {
                        pending.push(b);
                    }
                    continue;
                }
                flush(&mut pending, &mut out, cp);
                if !n.is_ascii_alphabetic() {
                    i += 1;
                    let text = match n {
                        '\\' | '{' | '}' => Some(n),
                        '~' => Some('\u{a0}'),
                        '_' => Some('-'),
                        '\n' | '\r' => Some('\n'),
                        '*' => {
                            g.skip = true;
                            None
                        }
                        _ => None,
                    };
                    if let Some(t) = text {
                        if skip_fallback > 0 {
                            skip_fallback -= 1;
                        } else if !g.skip {
                            out.push(t);
                        }
                    }
                    continue;
                }
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let pstart = i;
                if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let param: Option<i32> = chars[pstart..i].iter().collect::<String>().parse().ok();
                if chars.get(i) == Some(&' ') {
                    i += 1;
                }
                if SKIPPED.contains(&word.as_str()) {
                    g.skip = true;
                    continue;
                }
                let text: Option<&str> = match word.as_str() {
                    "ansicpg" => {
                        if let Some(p) = param.filter(|&p| p > 0) {
                            cp = p as u32;
                        }
                        None
                    }
                    "uc" => {
                        g.uc = param.unwrap_or(1).clamp(0, 8) as usize;
                        None
                    }
                    "u" => {
                        if let Some(p) = param {
                            let v = if p < 0 { p + 65536 } else { p } as u32;
                            if !g.skip {
                                out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                            }
                            skip_fallback = g.uc;
                        }
                        None
                    }
                    "par" | "line" | "sect" | "page" | "row" => Some("\n"),
                    "tab" | "cell" => Some("\t"),
                    "emdash" => Some("\u{2014}"),
                    "endash" => Some("\u{2013}"),
                    "bullet" => Some("\u{2022}"),
                    "lquote" => Some("\u{2018}"),
                    "rquote" => Some("\u{2019}"),
                    "ldblquote" => Some("\u{201c}"),
                    "rdblquote" => Some("\u{201d}"),
                    "emspace" | "enspace" | "qmspace" => Some(" "),
                    _ => None,
                };
                if let Some(t) = text {
                    if !g.skip {
                        out.push_str(t);
                    }
                }
            }
            '\r' | '\n' => {}
            _ => {
                if skip_fallback > 0 {
                    skip_fallback -= 1;
                    continue;
                }
                if g.skip {
                    continue;
                }
                if bytes && (c as u32) > 0x7F && (c as u32) <= 0xFF {
                    pending.push(c as u8);
                } else {
                    flush(&mut pending, &mut out, cp);
                    out.push(c);
                }
            }
        }
    }
    flush(&mut pending, &mut out, cp);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_en_texte() {
        let rtf = r#"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fswiss Arial;}}{\colortbl;\red0\green0\blue0;}
{\*\generator Riched20;}\viewkind4\uc1\pard\b CONTRAT DE LICENCE\b0\par
L'utilisateur accepte\~les termes. Caf\'e9 \u8364?\par
\pard{\field{\*\fldinst HYPERLINK "https://exemple.org"}{\fldrslt https://exemple.org}}\par}"#;
        let t = tidy(&from_text(rtf));
        assert_eq!(
            t,
            "CONTRAT DE LICENCE\nL'utilisateur accepte\u{a0}les termes. Café €\nhttps://exemple.org"
        );
    }

    #[test]
    fn rtf_page_de_code_et_octets() {
        // \ansicpg1251 : \'cf = П ; octet littéral 0xE9 lu en 1252 par défaut.
        let rtf = b"{\\rtf1\\ansi\\ansicpg1251 \\'cf\\'f0\\'e8\\par}";
        assert_eq!(tidy(&from_bytes(rtf)), "\u{41f}\u{440}\u{438}");
        let raw = b"{\\rtf1\\ansi Caf\xe9\\par}";
        assert_eq!(tidy(&from_bytes(raw)), "Café");
    }

    #[test]
    fn options_silencieuses() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for a in [
            "/S",
            "/VERYSILENT",
            "/silent",
            "/quiet",
            "/passive",
            "/qn",
            "/qb",
            "-q",
        ] {
            assert!(silent_flag(&v(&[a])), "{a}");
        }
        assert!(!silent_flag(&v(&[])));
        assert!(!silent_flag(&v(&[
            "/D=C:\\App",
            "/norestart",
            "/install",
            "/sp-"
        ])));
    }

    #[test]
    fn texte_brut_et_utf16() {
        assert_eq!(from_bytes("Licence\nlibre".as_bytes()), "Licence\nlibre");
        assert_eq!(from_bytes(b"Caf\xe9"), "Café");
        let mut u = vec![0xFF, 0xFE];
        u.extend(super::super::utf16z("Accord"));
        assert_eq!(from_bytes(&u), "Accord");
        assert_eq!(tidy("\n\na\r\n\r\n\r\nb  \n"), "a\n\nb");
    }
}
