//! Texte lisible d'une page HTML, pour le contrôle navigateur (#2228).
//!
//! Le contrôle n'est pas un moteur web : il montre des contrats de
//! licence. Il en garde le texte mot pour mot, avec ses paragraphes, ses
//! titres et ses listes, et décode les entités ; il laisse de côté les
//! scripts, les styles et l'en-tête. Noms d'entités : spécification
//! HTML 4.01 du W3C (jeux Latin-1, symboles et caractères spéciaux).

/// Entités Latin-1 de HTML 4.01, de U+00A0 à U+00FF, dans l'ordre.
const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

/// Autres entités courantes dans un contrat (HTML 4.01, caractères
/// spéciaux et symboles).
const OTHERS: [(&str, char); 23] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("OElig", 'Œ'),
    ("oelig", 'œ'),
    ("Scaron", 'Š'),
    ("scaron", 'š'),
    ("Yuml", 'Ÿ'),
    ("ndash", '–'),
    ("mdash", '—'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("sbquo", '‚'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("bdquo", '„'),
    ("bull", '•'),
    ("hellip", '…'),
    ("trade", '™'),
    ("euro", '€'),
    ("ensp", ' '),
];

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let v = match num.strip_prefix(['x', 'X']) {
            Some(h) => u32::from_str_radix(h, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(v);
    }
    if let Some(i) = LATIN1.iter().position(|n| *n == name) {
        return char::from_u32(0xA0 + i as u32);
    }
    OTHERS.iter().find(|(n, _)| *n == name).map(|&(_, c)| c)
}

/// Décode les entités de `s` ; une entité inconnue reste telle quelle.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .map(|e| e + 1)
            .unwrap_or(rest.len());
        let name = &rest[1..end];
        match entity(name) {
            Some(c) if !name.is_empty() => {
                out.push(if c == '\u{a0}' { ' ' } else { c });
                rest = &rest[end..];
                rest = rest.strip_prefix(';').unwrap_or(rest);
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Texte d'une page : BOM UTF-16 ou UTF-8, sinon UTF-8 s'il est valide,
/// sinon la page ANSI du système (Windows-1252 en Europe de l'Ouest).
pub fn decode_page(bytes: &[u8]) -> String {
    if let Some(b) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (pairs, _) = b.as_chunks::<2>();
        let w: Vec<u16> = pairs.iter().map(|c| u16::from_le_bytes(*c)).collect();
        return String::from_utf16_lossy(&w);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => super::super::decode_ansi(b),
    }
}

/// Balises qui coupent le texte en blocs.
const BLOCKS: [&str; 20] = [
    "p",
    "div",
    "br",
    "li",
    "ul",
    "ol",
    "tr",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "blockquote",
    "pre",
    "dl",
    "dt",
    "dd",
];

/// Balises dont le contenu n'est pas du texte affiché.
const HIDDEN: [&str; 5] = ["script", "style", "head", "title", "noscript"];

/// Texte affichable d'un document HTML : un paragraphe par bloc, séparés
/// par une ligne vide, puces « • » pour les éléments de liste.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut line = String::new();
    let mut hidden: Option<String> = None;
    let mut pre = 0usize;
    let mut rest = html;
    // Séparation due avant la ligne en cours : 1 retour, 2 ligne vide.
    let mut sep = 0u8;
    // Coupe de bloc de force `kind` : la ligne en cours part avec la
    // séparation qui la précédait, la suivante prendra au moins `kind`.
    let cut = |out: &mut String, line: &mut String, sep: &mut u8, kind: u8| {
        let t = line.trim();
        if t.is_empty() {
            *sep = (*sep).max(kind);
        } else {
            if !out.is_empty() {
                out.push_str(if *sep >= 2 { "\r\n\r\n" } else { "\r\n" });
            }
            out.push_str(t);
            *sep = kind;
        }
        line.clear();
    };
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if hidden.is_none() {
                push_text(&mut line, rest, pre > 0);
            }
            break;
        };
        if hidden.is_none() {
            push_text(&mut line, &rest[..lt], pre > 0);
        }
        rest = &rest[lt..];
        if let Some(c) = rest.strip_prefix("<!--") {
            rest = c.find("-->").map(|e| &c[e + 3..]).unwrap_or("");
            continue;
        }
        let Some(gt) = rest.find('>') else {
            break;
        };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some(h) = &hidden {
            if closing && *h == name {
                hidden = None;
            }
            continue;
        }
        if !closing && HIDDEN.contains(&name.as_str()) && !tag.ends_with('/') {
            hidden = Some(name);
            continue;
        }
        if name == "pre" {
            pre = if closing {
                pre.saturating_sub(1)
            } else {
                pre + 1
            };
        }
        if BLOCKS.contains(&name.as_str()) {
            let kind = if matches!(name.as_str(), "br" | "li" | "tr" | "dt" | "dd") {
                1
            } else {
                2
            };
            cut(&mut out, &mut line, &mut sep, kind);
            if !closing && name == "li" {
                line.push_str("• ");
            }
        } else if matches!(name.as_str(), "td" | "th") && !closing && !line.trim().is_empty() {
            line.push_str("  ");
        }
    }
    cut(&mut out, &mut line, &mut sep, 0);
    out
}

/// Ajoute un morceau de texte : blancs repliés en une espace (sauf dans
/// <pre>), entités décodées.
fn push_text(line: &mut String, raw: &str, pre: bool) {
    if pre {
        line.push_str(&decode_entities(raw));
        return;
    }
    let mut collapsed = String::with_capacity(raw.len());
    let mut space = line.ends_with(' ') || line.is_empty();
    for c in raw.chars() {
        if c.is_whitespace() {
            if !space {
                collapsed.push(' ');
                space = true;
            }
        } else {
            collapsed.push(c);
            space = false;
        }
    }
    line.push_str(&decode_entities(&collapsed));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entites_et_mots_gardes() {
        // Les formes relevées sur le contrat de WinRAR (#2228, passe E).
        assert_eq!(
            decode_entities("Version de d&eacute;monstration, le &laquo;conc&eacute;dant&raquo;"),
            "Version de démonstration, le «concédant»"
        );
        assert_eq!(
            decode_entities("relatif &agrave; l'archiveur"),
            "relatif à l'archiveur"
        );
        assert_eq!(
            decode_entities("&#233;t&#xE9; &copy; &amp; &inconnu; R&D"),
            "été © & &inconnu; R&D"
        );
        assert_eq!(decode_entities("a&nbsp;b"), "a b");
    }

    #[test]
    fn page_en_texte_par_blocs() {
        let html = "<html><head><title>Licence</title><style>p{color:red}</style></head>\
            <body><h1>Contrat</h1><p>Premier   paragraphe\r\n sur deux lignes.</p>\
            <!-- note --><script>var x = '<p>';</script>\
            <ul><li>un</li><li><b>deux</b> &eacute;l&eacute;ments</li></ul>\
            Ligne<br>suivante</body></html>";
        assert_eq!(
            html_to_text(html),
            "Contrat\r\n\r\nPremier paragraphe sur deux lignes.\r\n\r\n• un\r\n• deux éléments\r\n\r\nLigne\r\nsuivante"
        );
    }

    #[test]
    fn page_decodee_selon_son_codage() {
        assert_eq!(decode_page("été".as_bytes()), "été");
        assert_eq!(decode_page(&[0xEF, 0xBB, 0xBF, b'a']), "a");
        assert_eq!(decode_page(&[0xFF, 0xFE, b'a', 0, 0xE9, 0]), "aé");
    }
}
