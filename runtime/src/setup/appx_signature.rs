//! Signature d'un paquet MSIX ou AppX (`AppxSignature.p7x`).
//!
//! Le fichier commence par `PKCX`, suivi d'une structure PKCS #7
//! SignedData en DER. On lit les certificats qu'elle contient et on exige
//! que l'un d'eux porte exactement l'éditeur annoncé par le manifeste
//! (`Identity Publisher`), comme Windows. La chaîne de certificats et la
//! signature elle-même ne sont pas encore vérifiées (#235) : un paquet
//! signé reste donc « éditeur non vérifié » et l'utilisateur doit
//! confirmer, exactement comme pour un paquet sans signature.

/// Ce que l'on sait de la signature d'un paquet.
#[derive(Debug, PartialEq, Eq)]
pub enum Signature {
    /// Pas de `AppxSignature.p7x`.
    Absent,
    /// Signature illisible, ou certificat qui ne correspond pas à l'éditeur
    /// du manifeste : le paquet est refusé, même avec confirmation.
    Invalid(String),
    /// Certificat au nom de l'éditeur, chaîne non vérifiée.
    Unverified { signer: String },
}

/// Début des messages d'erreur qui demandent une confirmation : le lanceur
/// du double-clic le reconnaît (code de sortie 3) et pose la question.
pub const UNVERIFIED: &str = "Éditeur non vérifié";

/// Examine le contenu de `AppxSignature.p7x` (ou son absence).
pub fn examine(p7x: Option<&[u8]>, publisher: &str) -> Signature {
    let Some(data) = p7x else {
        return Signature::Absent;
    };
    let Some(der) = data.strip_prefix(b"PKCX") else {
        return Signature::Invalid("AppxSignature.p7x ne commence pas par PKCX".into());
    };
    let subjects = match certificate_subjects(der) {
        Ok(s) => s,
        Err(e) => return Signature::Invalid(format!("AppxSignature.p7x illisible : {e}")),
    };
    let wanted = parse_dn(publisher);
    match subjects.into_iter().find(|s| same_dn(s, &wanted)) {
        Some(s) => Signature::Unverified {
            signer: format_dn(&s),
        },
        None => Signature::Invalid(format!(
            "aucun certificat de la signature n'est au nom de l'éditeur « {publisher} » : paquet modifié ou falsifié"
        )),
    }
}

/// Texte expliqué à l'utilisateur avant la confirmation.
pub fn warning(sig: &Signature, display_publisher: &str) -> String {
    match sig {
        Signature::Absent => format!(
            "{UNVERIFIED} : ce paquet n'est pas signé. Rien ne garantit qu'il vient de « {display_publisher} » ni qu'il n'a pas été modifié. Ne l'installez que si vous faites confiance à sa provenance."
        ),
        Signature::Unverified { signer } => format!(
            "{UNVERIFIED} : ce paquet est signé au nom de « {signer} », mais Winlux ne vérifie pas encore la chaîne de certificats. Ne l'installez que si vous faites confiance à sa provenance."
        ),
        Signature::Invalid(e) => format!("signature refusée : {e}"),
    }
}

// ============================================================
//  DER minimal
// ============================================================

struct Tlv<'a> {
    tag: u8,
    body: &'a [u8],
    rest: &'a [u8],
}

fn tlv(d: &[u8]) -> Result<Tlv<'_>, String> {
    let (&tag, d) = d.split_first().ok_or("fin de données")?;
    if tag & 0x1F == 0x1F {
        return Err("étiquette DER longue non prise en charge".into());
    }
    let (&first, d) = d.split_first().ok_or("longueur absente")?;
    let (len, d) = if first < 0x80 {
        (first as usize, d)
    } else {
        let n = (first & 0x7F) as usize;
        if n == 0 || n > 4 || d.len() < n {
            return Err("longueur DER invalide".into());
        }
        let len = d[..n].iter().fold(0usize, |a, &b| (a << 8) | b as usize);
        (len, &d[n..])
    };
    if d.len() < len {
        return Err("structure DER tronquée".into());
    }
    Ok(Tlv {
        tag,
        body: &d[..len],
        rest: &d[len..],
    })
}

fn expect(d: &[u8], tag: u8) -> Result<Tlv<'_>, String> {
    let t = tlv(d)?;
    if t.tag != tag {
        return Err(format!(
            "étiquette {:#04x} attendue, {:#04x} trouvée",
            tag, t.tag
        ));
    }
    Ok(t)
}

const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];

/// Sujets (noms distingués) de tous les certificats du SignedData.
fn certificate_subjects(der: &[u8]) -> Result<Vec<Vec<(String, String)>>, String> {
    let content_info = expect(der, 0x30)?;
    let oid = expect(content_info.body, 0x06)?;
    if oid.body != OID_SIGNED_DATA {
        return Err("ce n'est pas une structure PKCS #7 SignedData".into());
    }
    let explicit = expect(oid.rest, 0xA0)?;
    let signed = expect(explicit.body, 0x30)?;
    // version, digestAlgorithms, encapContentInfo
    let mut d = expect(signed.body, 0x02)?.rest;
    d = expect(d, 0x31)?.rest;
    d = expect(d, 0x30)?.rest;
    let certs = expect(d, 0xA0).map_err(|_| "aucun certificat dans la signature")?;
    let mut out = Vec::new();
    let mut c = certs.body;
    while !c.is_empty() {
        let cert = tlv(c)?;
        c = cert.rest;
        if cert.tag != 0x30 {
            continue;
        }
        let tbs = expect(cert.body, 0x30)?;
        let mut f = tbs.body;
        if f.first() == Some(&0xA0) {
            f = tlv(f)?.rest; // version
        }
        f = expect(f, 0x02)?.rest; // numéro de série
        f = expect(f, 0x30)?.rest; // algorithme
        f = expect(f, 0x30)?.rest; // émetteur
        f = expect(f, 0x30)?.rest; // validité
        out.push(parse_name(expect(f, 0x30)?.body)?);
    }
    if out.is_empty() {
        return Err("aucun certificat dans la signature".into());
    }
    Ok(out)
}

fn parse_name(mut d: &[u8]) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    while !d.is_empty() {
        let rdn = expect(d, 0x31)?;
        d = rdn.rest;
        let mut a = rdn.body;
        while !a.is_empty() {
            let atv = expect(a, 0x30)?;
            a = atv.rest;
            let oid = expect(atv.body, 0x06)?;
            let value = tlv(oid.rest)?;
            out.push((attribute_name(oid.body), der_string(value.tag, value.body)));
        }
    }
    Ok(out)
}

fn attribute_name(oid: &[u8]) -> String {
    match oid {
        [0x55, 0x04, 0x03] => "CN".into(),
        [0x55, 0x04, 0x05] => "SERIALNUMBER".into(),
        [0x55, 0x04, 0x06] => "C".into(),
        [0x55, 0x04, 0x07] => "L".into(),
        [0x55, 0x04, 0x08] => "S".into(),
        [0x55, 0x04, 0x09] => "STREET".into(),
        [0x55, 0x04, 0x0A] => "O".into(),
        [0x55, 0x04, 0x0B] => "OU".into(),
        [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x01] => "E".into(),
        _ => format!("OID.{}", dotted(oid)),
    }
}

fn dotted(oid: &[u8]) -> String {
    let mut parts = Vec::new();
    let mut v: u64 = 0;
    for &b in oid {
        v = (v << 7) | (b & 0x7F) as u64;
        if b & 0x80 == 0 {
            if parts.is_empty() {
                let (a, b) = if v < 80 {
                    (v / 40, v % 40)
                } else {
                    (2, v - 80)
                };
                parts.push(a.to_string());
                parts.push(b.to_string());
            } else {
                parts.push(v.to_string());
            }
            v = 0;
        }
    }
    parts.join(".")
}

fn der_string(tag: u8, body: &[u8]) -> String {
    match tag {
        0x1E => {
            let units: Vec<u16> = body
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            String::from_utf16_lossy(&units)
        }
        0x14 => body.iter().map(|&b| b as char).collect(),
        _ => String::from_utf8_lossy(body).into_owned(),
    }
}

// ============================================================
//  Noms distingués « CN=…, O=…, C=… »
// ============================================================

fn canonical_key(k: &str) -> String {
    match k.trim().to_ascii_uppercase().as_str() {
        "ST" => "S".into(),
        "EMAIL" | "EMAILADDRESS" => "E".into(),
        o => o.to_string(),
    }
}

/// Lit l'éditeur du manifeste (virgules, guillemets doublés comme Windows).
pub fn parse_dn(s: &str) -> Vec<(String, String)> {
    let mut parts = Vec::new();
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
            ',' | ';' if !quoted => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    parts
        .into_iter()
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((canonical_key(k), v.trim().to_string()))
        })
        .collect()
}

fn same_dn(a: &[(String, String)], b: &[(String, String)]) -> bool {
    let norm = |v: &[(String, String)]| {
        let mut v: Vec<(String, String)> = v
            .iter()
            .map(|(k, x)| (canonical_key(k), x.trim().to_string()))
            .collect();
        v.sort();
        v
    };
    !a.is_empty() && norm(a) == norm(b)
}

fn format_dn(v: &[(String, String)]) -> String {
    v.iter()
        .map(|(k, x)| format!("{k}={x}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn der(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut v = vec![tag];
        if body.len() < 0x80 {
            v.push(body.len() as u8);
        } else {
            v.extend([0x82, (body.len() >> 8) as u8, body.len() as u8]);
        }
        v.extend(body);
        v
    }

    fn name(attrs: &[(&[u8], &str)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (oid, val) in attrs {
            let atv = [der(0x06, oid), der(0x0C, val.as_bytes())].concat();
            body.extend(der(0x31, &der(0x30, &atv)));
        }
        der(0x30, &body)
    }

    /// Faux `AppxSignature.p7x` : un certificat au sujet donné.
    pub(crate) fn fake_p7x(subject: &[(&[u8], &str)]) -> Vec<u8> {
        let issuer = name(&[(&[0x55, 0x04, 0x03], "Autorité de test")]);
        let tbs = [
            der(0xA0, &der(0x02, &[2])),
            der(0x02, &[1]),
            der(
                0x30,
                &der(
                    0x06,
                    &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B],
                ),
            ),
            issuer,
            der(0x30, &[]),
            name(subject),
            der(0x30, &[]),
        ]
        .concat();
        let cert = der(
            0x30,
            &[der(0x30, &tbs), der(0x30, &[]), der(0x03, &[0])].concat(),
        );
        let signed = [
            der(0x02, &[1]),
            der(0x31, &[]),
            der(
                0x30,
                &der(
                    0x06,
                    &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x02, 0x01, 0x04],
                ),
            ),
            der(0xA0, &cert),
            der(0x31, &[]),
        ]
        .concat();
        let ci = der(
            0x30,
            &[der(0x06, OID_SIGNED_DATA), der(0xA0, &der(0x30, &signed))].concat(),
        );
        [b"PKCX".as_slice(), &ci].concat()
    }

    const CN: &[u8] = &[0x55, 0x04, 0x03];
    const O: &[u8] = &[0x55, 0x04, 0x0A];
    const C: &[u8] = &[0x55, 0x04, 0x06];

    #[test]
    fn sans_signature() {
        assert_eq!(examine(None, "CN=Test"), Signature::Absent);
        assert!(warning(&Signature::Absent, "Test").starts_with(UNVERIFIED));
    }

    #[test]
    fn certificat_au_nom_de_l_editeur() {
        let p7x = fake_p7x(&[(CN, "Exemple, SARL"), (O, "Exemple"), (C, "FR")]);
        let sig = examine(Some(&p7x), r#"CN="Exemple, SARL", O=Exemple, C=FR"#);
        assert_eq!(
            sig,
            Signature::Unverified {
                signer: "CN=Exemple, SARL, O=Exemple, C=FR".into()
            }
        );
        assert!(warning(&sig, "Exemple").starts_with(UNVERIFIED));
        // L'ordre des attributs ne compte pas.
        assert!(matches!(
            examine(Some(&p7x), r#"C=FR, O=Exemple, CN="Exemple, SARL""#),
            Signature::Unverified { .. }
        ));
    }

    #[test]
    fn autre_editeur_refuse() {
        let p7x = fake_p7x(&[(CN, "Quelqu'un d'autre")]);
        assert!(matches!(
            examine(Some(&p7x), "CN=Exemple"),
            Signature::Invalid(_)
        ));
    }

    #[test]
    fn signature_illisible_refusee() {
        assert!(matches!(
            examine(Some(b"PKCX\x30\x05\x06"), "CN=Exemple"),
            Signature::Invalid(_)
        ));
        assert!(matches!(
            examine(Some(b"n'importe quoi"), "CN=Exemple"),
            Signature::Invalid(_)
        ));
    }

    #[test]
    fn oid_inconnu_en_notation_pointee() {
        assert_eq!(
            attribute_name(&[0x2B, 0x06, 0x01, 0x04, 0x01]),
            "OID.1.3.6.1.4.1"
        );
    }
}
