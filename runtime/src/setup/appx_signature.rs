//! Signature d'un paquet MSIX ou AppX (`AppxSignature.p7x`).
//!
//! Le fichier commence par `PKCX`, suivi d'une structure PKCS #7 SignedData
//! en DER (Authenticode). Comme Windows, on vérifie :
//! - que le certificat du signataire porte exactement l'éditeur annoncé par
//!   le manifeste (`Identity Publisher`) ;
//! - que la signature est bien celle de la clé de ce certificat, sur des
//!   attributs dont le condensat `messageDigest` est celui du contenu
//!   (`SpcIndirectDataContent`) ;
//! - que ce contenu porte les condensats du paquet reçu : `AXBM` (block
//!   map, qui protège à son tour chaque fichier), `AXCT`
//!   (`[Content_Types].xml`), `AXCI` (`AppxMetadata/CodeIntegrity.cat`),
//!   `AXPC` (les fichiers de l'archive) et `AXCD` (son répertoire central) ;
//! - que la chaîne de certificats mène à une autorité reconnue par le
//!   système (magasin d'OpenSSL, `SSL_CERT_FILE` / `SSL_CERT_DIR`) et que le
//!   certificat est prévu pour signer du code.
//!
//! Une signature fausse, ou un paquet modifié après signature, est refusé
//! sans recours. Une signature correcte dont l'autorité n'est pas reconnue
//! (certificat de test, autorité d'entreprise non installée, certificat
//! expiré) reste « éditeur non vérifié » : l'utilisateur peut confirmer.
//! Seul un paquet entièrement vérifié s'installe sans question.
//!
//! Les calculs (condensats, vérification RSA ou ECDSA, chaîne) passent par
//! la bibliothèque OpenSSL du système (`dlls::hostssl`).

use crate::archive::zip::{self, ZipEntry};
use crate::dlls::hostssl;
use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Ce que l'on sait de la signature d'un paquet.
#[derive(Debug, PartialEq, Eq)]
pub enum Signature {
    /// Pas de `AppxSignature.p7x`.
    Absent,
    /// Signature illisible ou fausse, certificat qui ne correspond pas à
    /// l'éditeur du manifeste, paquet modifié après signature : le paquet
    /// est refusé, même avec confirmation.
    Invalid(String),
    /// Signature correcte au nom de l'éditeur, mais pas entièrement
    /// vérifiée (`reason` : pourquoi), à confirmer par l'utilisateur.
    Unverified { signer: String, reason: String },
    /// Signature, contenu et chaîne de certificats vérifiés.
    Verified { signer: String },
}

/// Début des messages d'erreur qui demandent une confirmation : le lanceur
/// du double-clic le reconnaît (code de sortie 3) et pose la question.
pub const UNVERIFIED: &str = "Éditeur non vérifié";

/// Examine la signature du paquet `package` (fichier .msix, .appx ou
/// bundle) dont le manifeste annonce l'éditeur `publisher`.
pub fn examine(package: &Path, publisher: &str) -> Signature {
    let mut f = match File::open(package) {
        Ok(f) => f,
        Err(e) => return Signature::Invalid(format!("{} : {e}", package.display())),
    };
    let entries = match zip::list(&mut f) {
        Ok(e) => e,
        Err(e) => return Signature::Invalid(format!("{} : {e}", package.display())),
    };
    let Some(sig_entry) = find(&entries, "AppxSignature.p7x").cloned() else {
        return Signature::Absent;
    };
    let p7x = match read_entry(&mut f, &sig_entry) {
        Ok(d) => d,
        Err(e) => return Signature::Invalid(e),
    };
    let parsed = match parse_p7x(&p7x) {
        Ok(p) => p,
        Err(e) => return Signature::Invalid(format!("AppxSignature.p7x illisible : {e}")),
    };
    check(
        &parsed,
        publisher,
        &mut Archive {
            f,
            entries,
            sig_offset: sig_entry.offset,
        },
    )
}

/// Texte expliqué à l'utilisateur avant la confirmation.
pub fn warning(sig: &Signature, display_publisher: &str) -> String {
    match sig {
        Signature::Absent => format!(
            "{UNVERIFIED} : ce paquet n'est pas signé. Rien ne garantit qu'il vient de « {display_publisher} » ni qu'il n'a pas été modifié. Ne l'installez que si vous faites confiance à sa provenance."
        ),
        Signature::Unverified { signer, reason } => format!(
            "{UNVERIFIED} : ce paquet est signé au nom de « {signer} », mais {reason}. Ne l'installez que si vous faites confiance à sa provenance."
        ),
        Signature::Invalid(e) => format!("signature refusée : {e}"),
        Signature::Verified { signer } => format!("paquet signé par « {signer} », signature vérifiée"),
    }
}

// ============================================================
//  Vérification
// ============================================================

/// Accès au paquet reçu, pour recalculer les condensats signés.
struct Archive {
    f: File,
    entries: Vec<ZipEntry>,
    /// Position de l'en-tête local de `AppxSignature.p7x`.
    sig_offset: u64,
}

fn check(p: &Parsed, publisher: &str, a: &mut Archive) -> Signature {
    let Some(signer) = p.certs.iter().find(|c| p.signer_id.matches(c)) else {
        return Signature::Invalid("le certificat du signataire est absent de la signature".into());
    };
    let wanted = parse_dn(publisher);
    if !same_dn(&signer.subject, &wanted) {
        return Signature::Invalid(format!(
            "la signature est au nom de « {} » et non de l'éditeur « {publisher} » : paquet modifié ou falsifié",
            format_dn(&signer.subject)
        ));
    }
    let name = format_dn(&signer.subject);
    let unverified = |reason: &str| Signature::Unverified {
        signer: name.clone(),
        reason: reason.into(),
    };
    if let Some(failure) = check_signer(p, signer, &name) {
        return failure;
    }

    // 2. Condensats du paquet reçu.
    let md = p.appx_md;
    let digest_of = |a: &mut Archive, name: &str| -> Result<Option<Vec<u8>>, String> {
        match find(&a.entries, name).cloned() {
            None => Ok(None),
            Some(e) => {
                let data = read_entry(&mut a.f, &e)?;
                Ok(hostssl::digest(md, &data))
            }
        }
    };
    for (tag, file, required) in [
        (*b"AXBM", "AppxBlockMap.xml", true),
        (*b"AXCT", "[Content_Types].xml", true),
        (*b"AXCI", "AppxMetadata/CodeIntegrity.cat", false),
    ] {
        let signed = p.appx.iter().find(|(t, _)| *t == tag).map(|(_, h)| h);
        let actual = match digest_of(a, file) {
            Ok(h) => h,
            Err(e) => return Signature::Invalid(e),
        };
        match (signed, actual) {
            (Some(s), Some(h)) if *s == h => {}
            (None, None) if !required => {}
            (None, _) => {
                return Signature::Invalid(format!("la signature ne couvre pas {file}"));
            }
            (Some(_), None) => {
                return Signature::Invalid(format!(
                    "{file} est signé mais absent du paquet : paquet altéré"
                ));
            }
            (Some(_), Some(_)) => {
                return Signature::Invalid(format!(
                    "{file} a été modifié après la signature : paquet altéré"
                ));
            }
        }
    }
    let structure = match archive_digests(a, md) {
        Ok(d) => d,
        Err(e) => return Signature::Invalid(e),
    };
    let tagged = |tag: &[u8; 4]| {
        p.appx
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, h)| h.clone())
    };
    // Le block map protège déjà chaque fichier (et le paquet refuse tout
    // fichier qu'il ne liste pas) : un écart ici n'est pas une preuve
    // d'altération, mais l'archive n'est alors pas confirmée bit à bit.
    if tagged(b"AXPC") != Some(structure.0) || tagged(b"AXCD") != Some(structure.1) {
        return unverified(
            "la structure de l'archive ne correspond pas exactement à celle qui a été signée",
        );
    }

    check_chain(p, signer, name)
}

/// Signature des attributs signés par la clé du certificat du signataire
/// (commun aux paquets et aux exécutables) : Some(échec), None si elle est
/// juste.
fn check_signer(p: &Parsed, signer: &Cert, name: &str) -> Option<Signature> {
    let unverified = |reason: &str| {
        Some(Signature::Unverified {
            signer: name.to_string(),
            reason: reason.into(),
        })
    };
    if hostssl::ssl().is_none() {
        return unverified("la bibliothèque OpenSSL du système est absente : la signature n'a pas pu être vérifiée");
    }
    let Some(content_digest) = hostssl::digest(p.signer_md, &p.content) else {
        return unverified("son algorithme de condensat n'est pas pris en charge");
    };
    let signed: Vec<u8> = match &p.signed_attrs {
        Some(attrs) => {
            if attrs.message_digest != content_digest {
                return Some(Signature::Invalid(
                    "le condensat signé ne correspond pas au contenu de la signature : signature altérée".into(),
                ));
            }
            attrs.der.clone()
        }
        None => p.content.clone(),
    };
    match hostssl::verify_signature(&signer.der, p.signer_md, &p.signature, &signed) {
        Some(true) => {}
        Some(false) => return Some(Signature::Invalid(
            "la signature ne correspond pas à la clé du certificat : signature fausse ou altérée"
                .into(),
        )),
        None => return unverified("la clé de son certificat n'a pas pu être lue"),
    }
    None
}

/// Usage du certificat et chaîne de confiance.
fn check_chain(p: &Parsed, signer: &Cert, name: String) -> Signature {
    let unverified = |reason: &str| Signature::Unverified {
        signer: name.clone(),
        reason: reason.into(),
    };
    if signer
        .eku
        .as_ref()
        .is_some_and(|u| !u.iter().any(|o| o == OID_CODE_SIGNING))
    {
        return unverified("son certificat n'est pas prévu pour signer du code");
    }
    let others: Vec<Vec<u8>> = p
        .certs
        .iter()
        .filter(|c| c.der != signer.der)
        .map(|c| c.der.clone())
        .collect();
    match hostssl::verify(&signer.der, &others, None) {
        None => unverified("la chaîne de certificats n'a pas pu être vérifiée"),
        Some(v) if v.error == 0 => Signature::Verified { signer: name },
        // Certificat expiré (ou pas encore valide) : comme Windows, une
        // signature horodatée pendant sa validité reste valable (#1387).
        Some(v) if v.error == 9 || v.error == 10 => match timestamp(p) {
            Ok(Some(at)) => match hostssl::verify_at(&signer.der, &others, at) {
                Some(w) if w.error == 0 => Signature::Verified { signer: name },
                Some(w) => unverified(chain_reason(w.error)),
                None => unverified("la chaîne de certificats n'a pas pu être vérifiée"),
            },
            Ok(None) => unverified(chain_reason(v.error)),
            Err(e) => unverified(&format!(
                "{}, et son horodatage est refusé ({e})",
                chain_reason(v.error)
            )),
        },
        Some(v) => unverified(chain_reason(v.error)),
    }
}

/// Date (secondes Unix) de l'horodatage de la signature `p`, vérifié :
/// jeton RFC 3161 ou contre-signature PKCS #9 dont le condensat est celui
/// de la signature, signés par une autorité d'horodatage reconnue à cette
/// date. Ok(None) : pas d'horodatage.
fn timestamp(p: &Parsed) -> Result<Option<i64>, String> {
    if let Some(token) = &p.rfc3161 {
        return rfc3161_time(p, token).map(Some);
    }
    if let Some(body) = &p.countersignature {
        return countersignature_time(p, body).map(Some);
    }
    Ok(None)
}

fn rfc3161_time(p: &Parsed, token: &[u8]) -> Result<i64, String> {
    let ts = parse_signed_any(token)?;
    if ts.content_type != OID_TST_INFO {
        return Err("jeton d'horodatage sans TSTInfo".into());
    }
    let tsa = ts
        .certs
        .iter()
        .chain(p.certs.iter())
        .find(|c| ts.signer_id.matches(c))
        .ok_or("certificat de l'autorité d'horodatage absent")?;
    if let Some(failure) = check_signer(&ts, tsa, "horodatage") {
        return Err(match failure {
            Signature::Invalid(e) => e,
            Signature::Unverified { reason, .. } => reason,
            _ => "signature d'horodatage refusée".into(),
        });
    }
    // TSTInfo : version, politique, messageImprint, numéro, genTime…
    let mut t = expect(&ts.econtent, 0x02)?.rest;
    t = expect(t, 0x06)?.rest;
    let imprint = expect(t, 0x30)?;
    t = imprint.rest;
    let alg = expect(imprint.body, 0x30)?;
    let (md, _) = digest_name(alg.body)?;
    let hashed = expect(alg.rest, 0x04)?.body;
    t = expect(t, 0x02)?.rest;
    let gen = expect(t, 0x18)?;
    let at = der_time(0x18, gen.body).ok_or("date d'horodatage illisible")?;
    if hostssl::digest(md, &p.signature).as_deref() != Some(hashed) {
        return Err("l'horodatage porte sur une autre signature".into());
    }
    let extra: Vec<Vec<u8>> = ts
        .certs
        .iter()
        .chain(p.certs.iter())
        .map(|c| c.der.clone())
        .collect();
    check_tsa(tsa, &extra, at)?;
    Ok(at)
}

fn countersignature_time(p: &Parsed, body: &[u8]) -> Result<i64, String> {
    let si = parse_signer_info(body)?;
    let tsa = p
        .certs
        .iter()
        .find(|c| si.signer_id.matches(c))
        .ok_or("certificat de l'autorité d'horodatage absent")?;
    let attrs = si
        .signed_attrs
        .as_ref()
        .ok_or("contre-signature sans attributs signés")?;
    if hostssl::digest(si.signer_md, &p.signature).as_deref() != Some(&attrs.message_digest[..]) {
        return Err("l'horodatage porte sur une autre signature".into());
    }
    if hostssl::verify_signature(&tsa.der, si.signer_md, &si.signature, &attrs.der) != Some(true) {
        return Err("signature d'horodatage fausse".into());
    }
    let at = attrs.signing_time.ok_or("contre-signature sans date")?;
    let extra: Vec<Vec<u8>> = p.certs.iter().map(|c| c.der.clone()).collect();
    check_tsa(tsa, &extra, at)?;
    Ok(at)
}

/// Autorité d'horodatage : prévue pour horodater et reconnue à la date `at`.
fn check_tsa(tsa: &Cert, extra: &[Vec<u8>], at: i64) -> Result<(), String> {
    if tsa
        .eku
        .as_ref()
        .is_some_and(|u| !u.iter().any(|o| o == OID_TIME_STAMPING))
    {
        return Err("certificat non prévu pour horodater".into());
    }
    match hostssl::verify_at(&tsa.der, extra, at) {
        Some(v) if v.error == 0 => Ok(()),
        Some(v) => Err(format!("autorité d'horodatage : {}", chain_reason(v.error))),
        None => Err("chaîne de l'autorité d'horodatage non vérifiable".into()),
    }
}

fn chain_reason(error: i32) -> &'static str {
    match error {
        // X509_V_ERR_CERT_NOT_YET_VALID, X509_V_ERR_CERT_HAS_EXPIRED
        9 => "son certificat n'est pas encore valide",
        10 => "son certificat a expiré",
        // DEPTH_ZERO_SELF_SIGNED, SELF_SIGNED_IN_CHAIN, UNABLE_TO_GET_ISSUER_LOCALLY…
        2 | 18 | 19 | 20 | 21 => {
            "son certificat ne vient pas d'une autorité reconnue par ce système"
        }
        23 => "son certificat a été révoqué",
        _ => "sa chaîne de certificats n'est pas valide",
    }
}

/// Condensats `AXPC` (tout ce qui précède l'en-tête local de la signature)
/// et `AXCD` (répertoire central et fin d'archive, tels qu'ils seraient
/// sans l'entrée de la signature).
fn archive_digests(a: &mut Archive, md: &CStr) -> Result<(Vec<u8>, Vec<u8>), String> {
    let io = |e: std::io::Error| format!("lecture du paquet : {e}");
    let len = a.f.seek(SeekFrom::End(0)).map_err(io)?;
    let tail_len = len.min(65_557 + 76);
    let mut tail = vec![0u8; tail_len as usize];
    a.f.seek(SeekFrom::Start(len - tail_len)).map_err(io)?;
    a.f.read_exact(&mut tail).map_err(io)?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4B, 0x05, 0x06])
        .ok_or("fin d'archive introuvable")?;
    let le16 = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as u64;
    let le32 = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as u64;
    let le64 = |b: &[u8], o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let mut end = tail[eocd..].to_vec();
    let mut cd_size = le32(&end, 12);
    let mut cd_off = le32(&end, 16);
    // Enregistrements Zip64 (fin d'archive + localisateur), s'il y en a.
    let mut z64: Option<(Vec<u8>, Vec<u8>)> = None;
    if eocd >= 20 && tail[eocd - 20..eocd - 16] == [0x50, 0x4B, 0x06, 0x07] {
        let loc = tail[eocd - 20..eocd].to_vec();
        let rec_off = le64(&loc, 8);
        let mut rec = vec![0u8; 56];
        a.f.seek(SeekFrom::Start(rec_off)).map_err(io)?;
        a.f.read_exact(&mut rec).map_err(io)?;
        if rec[..4] != [0x50, 0x4B, 0x06, 0x06] {
            return Err("fin d'archive Zip64 invalide".into());
        }
        let rec_len = 12 + le64(&rec, 4);
        let mut full = vec![0u8; rec_len.min(1 << 16) as usize];
        a.f.seek(SeekFrom::Start(rec_off)).map_err(io)?;
        a.f.read_exact(&mut full).map_err(io)?;
        cd_size = le64(&full, 40);
        cd_off = le64(&full, 48);
        z64 = Some((full, loc));
    }
    if cd_off + cd_size > len || cd_size > 1 << 30 {
        return Err("répertoire central invalide".into());
    }
    let mut cd = vec![0u8; cd_size as usize];
    a.f.seek(SeekFrom::Start(cd_off)).map_err(io)?;
    a.f.read_exact(&mut cd).map_err(io)?;
    // Retire l'enregistrement de AppxSignature.p7x.
    let mut out = Vec::with_capacity(cd.len());
    let mut i = 0;
    let mut removed = 0u64;
    while i + 46 <= cd.len() && cd[i..i + 4] == [0x50, 0x4B, 0x01, 0x02] {
        let n = le16(&cd, i + 28) as usize;
        let size = 46 + n + le16(&cd, i + 30) as usize + le16(&cd, i + 32) as usize;
        let rec = cd.get(i..i + size).ok_or("répertoire central tronqué")?;
        if rec[46..46 + n].eq_ignore_ascii_case(b"AppxSignature.p7x") {
            removed += size as u64;
        } else {
            out.extend_from_slice(rec);
        }
        i += size;
    }
    if removed == 0 {
        return Err("AppxSignature.p7x absent du répertoire central".into());
    }
    let new_size = cd_size - removed;
    let new_off = a.sig_offset;
    let set16 = |b: &mut [u8], o: usize, v: u64| {
        if b[o..o + 2] != [0xFF, 0xFF] {
            b[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes())
        }
    };
    let set32 = |b: &mut [u8], o: usize, v: u64| {
        if b[o..o + 4] != [0xFF; 4] {
            b[o..o + 4].copy_from_slice(&(v as u32).to_le_bytes())
        }
    };
    if let Some((mut rec, mut loc)) = z64 {
        let n = le64(&rec, 24) - 1;
        let total = le64(&rec, 32) - 1;
        rec[24..32].copy_from_slice(&n.to_le_bytes());
        rec[32..40].copy_from_slice(&total.to_le_bytes());
        rec[40..48].copy_from_slice(&new_size.to_le_bytes());
        rec[48..56].copy_from_slice(&new_off.to_le_bytes());
        loc[8..16].copy_from_slice(&(new_off + new_size).to_le_bytes());
        out.extend(rec);
        out.extend(loc);
    }
    let (n, total) = (
        le16(&end, 8).wrapping_sub(1),
        le16(&end, 10).wrapping_sub(1),
    );
    set16(&mut end, 8, n);
    set16(&mut end, 10, total);
    set32(&mut end, 12, new_size);
    set32(&mut end, 16, new_off);
    out.extend(end);

    let mut h = hostssl::Hasher::new(md).ok_or("algorithme de condensat indisponible")?;
    a.f.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut left = a.sig_offset;
    let mut buf = vec![0u8; 1 << 20];
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        a.f.read_exact(&mut buf[..n]).map_err(io)?;
        h.update(&buf[..n]);
        left -= n as u64;
    }
    let axpc = h.finish();
    let axcd = hostssl::digest(md, &out).ok_or("algorithme de condensat indisponible")?;
    Ok((axpc, axcd))
}

fn find<'a>(entries: &'a [ZipEntry], name: &str) -> Option<&'a ZipEntry> {
    entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
}

fn read_entry(f: &mut File, e: &ZipEntry) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(e.usize.min(1 << 26) as usize);
    zip::open_entry(f, e, None)
        .map_err(|err| format!("{} : {err}", e.name))?
        .read_to_end(&mut data)
        .map_err(|err| format!("{} : {err}", e.name))?;
    Ok(data)
}

// ============================================================
//  Lecture du SignedData
// ============================================================

const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];
const OID_SPC_INDIRECT_DATA: &[u8] = &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x02, 0x01, 0x04];
const OID_MESSAGE_DIGEST: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04];
const OID_SIGNING_TIME: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05];
const OID_COUNTERSIGNATURE: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x06];
/// Jeton d'horodatage RFC 3161 d'Authenticode (1.3.6.1.4.1.311.3.3.1).
const OID_RFC3161_TIMESTAMP: &[u8] = &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x03, 0x03, 0x01];
/// id-ct-TSTInfo (1.2.840.113549.1.9.16.1.4).
const OID_TST_INFO: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x01, 0x04,
];
/// Usage étendu « horodatage » (1.3.6.1.5.5.7.3.8).
const OID_TIME_STAMPING: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x08];
const OID_CODE_SIGNING: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x03];

/// Certificat de la signature.
struct Cert {
    der: Vec<u8>,
    issuer: Vec<u8>,
    serial: Vec<u8>,
    subject: Vec<(String, String)>,
    ski: Option<Vec<u8>>,
    /// Usages étendus (None : extension absente, tout usage permis).
    eku: Option<Vec<Vec<u8>>>,
}

enum SignerId {
    IssuerSerial(Vec<u8>, Vec<u8>),
    KeyId(Vec<u8>),
}

impl SignerId {
    fn matches(&self, c: &Cert) -> bool {
        match self {
            SignerId::IssuerSerial(i, s) => *i == c.issuer && *s == c.serial,
            SignerId::KeyId(k) => c.ski.as_ref() == Some(k),
        }
    }
}

struct SignedAttrs {
    /// Attributs ré-étiquetés en SET (0x31), tels qu'ils sont signés.
    der: Vec<u8>,
    message_digest: Vec<u8>,
    /// Attribut signingTime (secondes Unix), s'il est présent.
    signing_time: Option<i64>,
}

struct Parsed {
    certs: Vec<Cert>,
    /// Octets dont `messageDigest` est le condensat.
    content: Vec<u8>,
    /// Type du contenu signé (OID).
    content_type: Vec<u8>,
    /// Corps de la SEQUENCE du contenu signé.
    econtent: Vec<u8>,
    /// Algorithme du condensat signé (`APPX` d'un paquet, image d'un .exe).
    appx_md: &'static CStr,
    /// Condensats `APPX` : (étiquette, valeur).
    appx: Vec<([u8; 4], Vec<u8>)>,
    /// Condensat signé tel quel (`SpcIndirectDataContent.messageDigest`).
    indirect: Vec<u8>,
    signer_id: SignerId,
    signer_md: &'static CStr,
    signed_attrs: Option<SignedAttrs>,
    signature: Vec<u8>,
    /// Jeton d'horodatage RFC 3161 (ContentInfo SignedData), non signé.
    rfc3161: Option<Vec<u8>>,
    /// Contre-signature PKCS #9 (corps du SignerInfo), non signée.
    countersignature: Option<Vec<u8>>,
}

fn raw<'a>(d: &'a [u8], t: &Tlv<'a>) -> &'a [u8] {
    &d[..d.len() - t.rest.len()]
}

fn digest_name(alg_id: &[u8]) -> Result<(&'static CStr, usize), String> {
    let oid = expect(alg_id, 0x06)?.body;
    match oid {
        [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01] => Ok((c"SHA256", 32)),
        // SHA-1 : encore courant sur les exécutables signés avant 2016.
        [0x2B, 0x0E, 0x03, 0x02, 0x1A] => Ok((c"SHA1", 20)),
        [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02] => Ok((c"SHA384", 48)),
        [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x03] => Ok((c"SHA512", 64)),
        _ => Err(format!(
            "algorithme de condensat {} non pris en charge",
            dotted(oid)
        )),
    }
}

fn parse_p7x(data: &[u8]) -> Result<Parsed, String> {
    let der = data
        .strip_prefix(b"PKCX")
        .ok_or("ne commence pas par PKCX")?;
    let mut p = parse_signed_data(der)?;
    if p.appx_md.to_bytes() == b"SHA1" {
        return Err("condensats APPX en SHA-1, algorithme refusé".into());
    }
    let body = p
        .indirect
        .strip_prefix(b"APPX")
        .ok_or("condensats APPX absents")?;
    // Les condensats APPX ont la taille de l'algorithme annoncé.
    let hlen = digest_len(p.appx_md);
    if body.is_empty() || body.len() % (4 + hlen) != 0 {
        return Err("condensats APPX de taille invalide".into());
    }
    p.appx = body
        .chunks(4 + hlen)
        .map(|c| ([c[0], c[1], c[2], c[3]], c[4..].to_vec()))
        .collect();
    Ok(p)
}

fn digest_len(md: &CStr) -> usize {
    match md.to_bytes() {
        b"SHA1" => 20,
        b"SHA384" => 48,
        b"SHA512" => 64,
        _ => 32,
    }
}

/// PKCS #7 SignedData Authenticode (DER) : certificats, signataire et
/// condensat signé du `SpcIndirectDataContent`.
fn parse_signed_data(der: &[u8]) -> Result<Parsed, String> {
    let mut p = parse_signed_any(der)?;
    if p.content_type != OID_SPC_INDIRECT_DATA {
        return Err("le contenu signé n'est pas un SpcIndirectDataContent".into());
    }
    let spc = p.econtent.clone();
    let attr = expect(&spc, 0x30)?; // SpcAttributeTypeAndOptionalValue
    let digest_info = expect(attr.rest, 0x30)?;
    let alg = expect(digest_info.body, 0x30)?;
    let (appx_md, _) = digest_name(alg.body)?;
    p.appx_md = appx_md;
    p.indirect = expect(alg.rest, 0x04)?.body.to_vec();
    Ok(p)
}

/// SignedData quel que soit son contenu (Authenticode, jeton d'horodatage
/// RFC 3161) : certificats, contenu signé et premier signataire.
fn parse_signed_any(der: &[u8]) -> Result<Parsed, String> {
    let content_info = expect(der, 0x30)?;
    let oid = expect(content_info.body, 0x06)?;
    if oid.body != OID_SIGNED_DATA {
        return Err("ce n'est pas une structure PKCS #7 SignedData".into());
    }
    let explicit = expect(oid.rest, 0xA0)?;
    let signed = expect(explicit.body, 0x30)?;
    let mut d = expect(signed.body, 0x02)?.rest; // version
    d = expect(d, 0x31)?.rest; // digestAlgorithms

    let encap = expect(d, 0x30)?;
    d = encap.rest;
    let ctype = expect(encap.body, 0x06)?;
    let wrapped = expect(ctype.rest, 0xA0).map_err(|_| "contenu signé absent")?;
    let inner = tlv(wrapped.body)?;
    // PKCS #7 (Authenticode) : la SEQUENCE elle-même, condensat de son
    // corps ; CMS : une OCTET STRING, condensat de sa valeur.
    let (content, econtent) = match inner.tag {
        0x30 => (inner.body.to_vec(), inner.body.to_vec()),
        0x04 => (
            inner.body.to_vec(),
            expect(inner.body, 0x30)
                .map(|t| t.body.to_vec())
                .unwrap_or_default(),
        ),
        t => return Err(format!("contenu signé d'étiquette {t:#04x}")),
    };

    // Certificats.
    let certs_tlv = expect(d, 0xA0).map_err(|_| "aucun certificat dans la signature")?;
    d = certs_tlv.rest;
    let mut certs = Vec::new();
    let mut c = certs_tlv.body;
    while !c.is_empty() {
        let t = tlv(c)?;
        let whole = raw(c, &t);
        c = t.rest;
        if t.tag == 0x30 {
            certs.push(parse_cert(whole, t.body)?);
        }
    }
    if certs.is_empty() {
        return Err("aucun certificat dans la signature".into());
    }
    if d.first() == Some(&0xA1) {
        d = tlv(d)?.rest; // CRL
    }

    // Premier SignerInfo.
    let infos = expect(d, 0x31)?;
    let info = expect(infos.body, 0x30).map_err(|_| "aucun signataire")?;
    let si = parse_signer_info(info.body)?;
    Ok(Parsed {
        certs,
        content,
        content_type: ctype.body.to_vec(),
        econtent,
        appx_md: si.signer_md,
        appx: Vec::new(),
        indirect: Vec::new(),
        signer_id: si.signer_id,
        signer_md: si.signer_md,
        signed_attrs: si.signed_attrs,
        signature: si.signature,
        rfc3161: si.rfc3161,
        countersignature: si.countersignature,
    })
}

/// Un SignerInfo (corps de la SEQUENCE).
struct SignerInfo {
    signer_id: SignerId,
    signer_md: &'static CStr,
    signed_attrs: Option<SignedAttrs>,
    signature: Vec<u8>,
    rfc3161: Option<Vec<u8>>,
    countersignature: Option<Vec<u8>>,
}

fn parse_signer_info(body: &[u8]) -> Result<SignerInfo, String> {
    let mut s = expect(body, 0x02)?.rest; // version
    let sid = tlv(s)?;
    s = sid.rest;
    let signer_id = match sid.tag {
        0x30 => {
            let issuer = expect(sid.body, 0x30)?;
            let serial = expect(issuer.rest, 0x02)?;
            SignerId::IssuerSerial(raw(sid.body, &issuer).to_vec(), serial.body.to_vec())
        }
        0x80 => SignerId::KeyId(sid.body.to_vec()),
        t => return Err(format!("identifiant de signataire d'étiquette {t:#04x}")),
    };
    let md = expect(s, 0x30)?;
    s = md.rest;
    let (signer_md, _) = digest_name(md.body)?;
    let mut signed_attrs = None;
    if s.first() == Some(&0xA0) {
        let t = tlv(s)?;
        let mut der = raw(s, &t).to_vec();
        der[0] = 0x31;
        let mut message_digest = None;
        let mut signing_time = None;
        let mut a = t.body;
        while !a.is_empty() {
            let at = expect(a, 0x30)?;
            a = at.rest;
            let oid = expect(at.body, 0x06)?;
            let set = expect(oid.rest, 0x31)?;
            if oid.body == OID_MESSAGE_DIGEST {
                message_digest = Some(expect(set.body, 0x04)?.body.to_vec());
            } else if oid.body == OID_SIGNING_TIME {
                signing_time = tlv(set.body).ok().and_then(|v| der_time(v.tag, v.body));
            }
        }
        signed_attrs = Some(SignedAttrs {
            der,
            message_digest: message_digest.ok_or("attribut messageDigest absent")?,
            signing_time,
        });
        s = t.rest;
    }
    s = expect(s, 0x30)?.rest; // signatureAlgorithm
    let sig = expect(s, 0x04)?;
    s = sig.rest;
    // Attributs non signés : contre-signature d'horodatage (#1387).
    let mut rfc3161 = None;
    let mut countersignature = None;
    if s.first() == Some(&0xA1) {
        let mut a = tlv(s)?.body;
        while !a.is_empty() {
            let at = expect(a, 0x30)?;
            a = at.rest;
            let oid = expect(at.body, 0x06)?;
            let set = expect(oid.rest, 0x31)?;
            let first = tlv(set.body)?;
            let whole = raw(set.body, &first).to_vec();
            if oid.body == OID_RFC3161_TIMESTAMP {
                rfc3161 = Some(whole);
            } else if oid.body == OID_COUNTERSIGNATURE && first.tag == 0x30 {
                countersignature = Some(first.body.to_vec());
            }
        }
    }
    Ok(SignerInfo {
        signer_id,
        signer_md,
        signed_attrs,
        signature: sig.body.to_vec(),
        rfc3161,
        countersignature,
    })
}

/// Date ASN.1 (UTCTime 0x17 ou GeneralizedTime 0x18) en secondes Unix.
fn der_time(tag: u8, body: &[u8]) -> Option<i64> {
    let t = std::str::from_utf8(body).ok()?;
    let t = t.strip_suffix('Z')?;
    let (year, rest) = match tag {
        0x17 => {
            let yy: i64 = t.get(..2)?.parse().ok()?;
            (if yy < 50 { 2000 + yy } else { 1900 + yy }, t.get(2..)?)
        }
        0x18 => (t.get(..4)?.parse().ok()?, t.get(4..)?),
        _ => return None,
    };
    let rest = rest.split('.').next()?;
    if rest.len() != 10 || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |i: usize| rest[i..i + 2].parse::<i64>().ok();
    let (month, day, h, m, sec) = (n(0)?, n(2)?, n(4)?, n(6)?, n(8)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Jours depuis 1970-01-01 (calendrier grégorien proleptique).
    let (y, mo) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * mo + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + m * 60 + sec)
}

fn parse_cert(whole: &[u8], body: &[u8]) -> Result<Cert, String> {
    let tbs = expect(body, 0x30)?;
    let mut f = tbs.body;
    if f.first() == Some(&0xA0) {
        f = tlv(f)?.rest; // version
    }
    let serial = expect(f, 0x02)?;
    f = serial.rest;
    f = expect(f, 0x30)?.rest; // algorithme
    let issuer = expect(f, 0x30)?;
    let issuer_raw = raw(f, &issuer).to_vec();
    f = issuer.rest;
    f = expect(f, 0x30)?.rest; // validité
    let subject = expect(f, 0x30)?;
    f = subject.rest;
    f = expect(f, 0x30)?.rest; // clé publique
    let mut ski = None;
    let mut eku = None;
    while !f.is_empty() {
        let t = tlv(f)?;
        f = t.rest;
        if t.tag != 0xA3 {
            continue;
        }
        let mut e = expect(t.body, 0x30)?.body;
        while !e.is_empty() {
            let ext = expect(e, 0x30)?;
            e = ext.rest;
            let oid = expect(ext.body, 0x06)?;
            let mut v = oid.rest;
            if v.first() == Some(&0x01) {
                v = tlv(v)?.rest; // critique
            }
            let value = expect(v, 0x04)?.body;
            match oid.body {
                [0x55, 0x1D, 0x0E] => ski = Some(expect(value, 0x04)?.body.to_vec()),
                [0x55, 0x1D, 0x25] => {
                    let mut list = Vec::new();
                    let mut u = expect(value, 0x30)?.body;
                    while !u.is_empty() {
                        let o = expect(u, 0x06)?;
                        u = o.rest;
                        list.push(o.body.to_vec());
                    }
                    eku = Some(list);
                }
                _ => {}
            }
        }
    }
    Ok(Cert {
        der: whole.to_vec(),
        issuer: issuer_raw,
        serial: serial.body.to_vec(),
        subject: parse_name(subject.body)?,
        ski,
        eku,
    })
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

// ============================================================
//  Exécutables signés (Authenticode d'un PE)
// ============================================================

/// Taille au-delà de laquelle un exécutable n'est pas lu en entier pour
/// vérifier sa signature (il est alors « éditeur inconnu »).
const PE_MAX: u64 = 512 << 20;

/// Signature Authenticode d'un exécutable ou d'une DLL : table de sécurité
/// du PE (WIN_CERTIFICATE, PKCS #7 SignedData), selon la spécification
/// publique « Windows Authenticode Portable Executable Signature Format ».
/// Vérifie le condensat de l'image, la signature du signataire et sa chaîne
/// (#1382). `Verified.signer` / `Unverified.signer` : nom à afficher (CN du
/// certificat, sinon O).
pub fn examine_pe(path: &Path) -> Signature {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size == 0 || size > PE_MAX {
        return Signature::Absent;
    }
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return Signature::Invalid(format!("{} : {e}", path.display())),
    };
    examine_pe_bytes(&data)
}

/// Zones de l'image : (somme de contrôle, entrée « sécurité » du répertoire
/// des données, table des certificats (début, taille)).
fn pe_layout(d: &[u8]) -> Option<(usize, usize, usize, usize)> {
    let u16_at = |o: usize| Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?));
    let u32_at = |o: usize| Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?));
    if d.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(0x3C)? as usize;
    if d.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let opt = pe + 24;
    let dirs = match u16_at(opt)? {
        0x10B => opt + 96,
        0x20B => opt + 112,
        _ => return None,
    };
    let count = u32_at(dirs - 4)? as usize;
    if count < 5 {
        return Some((opt + 64, 0, 0, 0));
    }
    let entry = dirs + 4 * 8;
    let start = u32_at(entry)? as usize;
    let len = u32_at(entry + 4)? as usize;
    Some((opt + 64, entry, start, len))
}

fn examine_pe_bytes(d: &[u8]) -> Signature {
    let Some((checksum, entry, start, len)) = pe_layout(d) else {
        return Signature::Absent;
    };
    if len == 0 || entry == 0 {
        return Signature::Absent;
    }
    let Some(table) = start.checked_add(len).and_then(|end| d.get(start..end)) else {
        return Signature::Invalid("table des certificats hors du fichier".into());
    };
    // WIN_CERTIFICATE : dwLength, wRevision, wCertificateType (2 = PKCS #7).
    if table.len() < 8 {
        return Signature::Invalid("table des certificats tronquée".into());
    }
    let cert_len = u32::from_le_bytes([table[0], table[1], table[2], table[3]]) as usize;
    let kind = u16::from_le_bytes([table[6], table[7]]);
    if kind != 2 || cert_len < 8 || cert_len > table.len() {
        return Signature::Invalid("certificat Authenticode illisible".into());
    }
    let parsed = match parse_signed_data(&table[8..cert_len]) {
        Ok(p) => p,
        Err(e) => return Signature::Invalid(format!("signature illisible : {e}")),
    };
    let Some(signer) = parsed.certs.iter().find(|c| parsed.signer_id.matches(c)) else {
        return Signature::Invalid("le certificat du signataire est absent de la signature".into());
    };
    let name = display_name(&signer.subject);
    if let Some(failure) = check_signer(&parsed, signer, &name) {
        return failure;
    }
    // Condensat de l'image : tout le fichier sauf la somme de contrôle,
    // l'entrée « sécurité » du répertoire et la table des certificats.
    if !(checksum + 4 <= entry && entry + 8 <= start && start + len <= d.len()) {
        return Signature::Invalid("disposition de l'image inattendue".into());
    }
    let mut image = Vec::with_capacity(d.len());
    image.extend_from_slice(&d[..checksum]);
    image.extend_from_slice(&d[checksum + 4..entry]);
    image.extend_from_slice(&d[entry + 8..start]);
    image.extend_from_slice(&d[start + len..]);
    match hostssl::digest(parsed.appx_md, &image) {
        Some(h) if h == parsed.indirect => {}
        Some(_) => return Signature::Invalid("le fichier a été modifié après sa signature".into()),
        None => {
            return Signature::Unverified {
                signer: name,
                reason: "son algorithme de condensat n'est pas pris en charge".into(),
            }
        }
    }
    check_chain(&parsed, signer, name)
}

/// Nom d'éditeur affiché : CN du certificat, sinon O, sinon le nom complet.
fn display_name(subject: &[(String, String)]) -> String {
    ["CN", "O"]
        .iter()
        .find_map(|k| subject.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()))
        .unwrap_or_else(|| format_dn(subject))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exécutable minimal (PE32+, sans section), long d'un multiple de 8.
    fn pe_minimal() -> Vec<u8> {
        let mut d = vec![0u8; 0x200];
        d[..2].copy_from_slice(b"MZ");
        d[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        d[0x40..0x44].copy_from_slice(b"PE\0\0");
        d[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
        d[0x54..0x56].copy_from_slice(&240u16.to_le_bytes());
        let opt = 0x58;
        d[opt..opt + 2].copy_from_slice(&0x20Bu16.to_le_bytes());
        d[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes());
        d[0x180..0x190].copy_from_slice(b"contenu du test!");
        d
    }

    fn der(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        match body.len() {
            n if n < 0x80 => out.push(n as u8),
            n if n < 0x100 => out.extend([0x81, n as u8]),
            n => out.extend([0x82, (n >> 8) as u8, n as u8]),
        }
        out.extend_from_slice(body);
        out
    }

    /// Condensat de l'image d'un PE et SpcIndirectDataContent qui le porte.
    fn spc_for(pe: &[u8]) -> Vec<u8> {
        let (checksum, entry, _, _) = pe_layout(pe).unwrap();
        let mut image = pe[..checksum].to_vec();
        image.extend_from_slice(&pe[checksum + 4..entry]);
        image.extend_from_slice(&pe[entry + 8..]);
        let digest = hostssl::digest(c"SHA256", &image).unwrap();
        let oid_pe = der(
            0x06,
            &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x02, 0x01, 0x0F],
        );
        let attr = der(0x30, &[oid_pe, der(0x30, &der(0x03, &[0]))].concat());
        let digest_info = der(0x30, &[sha256_alg(), der(0x04, &digest)].concat());
        der(0x30, &[attr, digest_info].concat())
    }

    fn sha256_alg() -> Vec<u8> {
        let sha256 = der(
            0x06,
            &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01],
        );
        der(0x30, &[sha256, vec![0x05, 0x00]].concat())
    }

    /// Ajoute la table des certificats (signature `sig`) à la fin du PE.
    fn attach(pe: &[u8], sig: &[u8]) -> Vec<u8> {
        let (_, entry, _, _) = pe_layout(pe).unwrap();
        let padded = (8 + sig.len()).div_ceil(8) * 8;
        let mut table = (padded as u32).to_le_bytes().to_vec();
        table.extend_from_slice(&0x0200u16.to_le_bytes());
        table.extend_from_slice(&2u16.to_le_bytes());
        table.extend_from_slice(sig);
        table.resize(padded, 0);
        let mut signed = pe.to_vec();
        let start = signed.len();
        signed.extend_from_slice(&table);
        signed[entry..entry + 4].copy_from_slice(&(start as u32).to_le_bytes());
        signed[entry + 4..entry + 8].copy_from_slice(&(padded as u32).to_le_bytes());
        signed
    }

    /// Ajoute l'attribut non signé `attr` (SEQUENCE complète) au premier
    /// SignerInfo d'un ContentInfo SignedData.
    fn with_unsigned_attr(sig: &[u8], attr: &[u8]) -> Vec<u8> {
        let ci = expect(sig, 0x30).unwrap();
        let oid = expect(ci.body, 0x06).unwrap();
        let explicit = expect(oid.rest, 0xA0).unwrap();
        let sd = expect(explicit.body, 0x30).unwrap();
        // Tout ce qui précède le SET des SignerInfo reste tel quel.
        let mut d = sd.body;
        let mut head = Vec::new();
        loop {
            let t = tlv(d).unwrap();
            if t.tag == 0x31 && t.rest.is_empty() {
                break;
            }
            head.extend_from_slice(raw(d, &t));
            d = t.rest;
        }
        let infos = expect(d, 0x31).unwrap();
        let info = expect(infos.body, 0x30).unwrap();
        let body = [info.body, &der(0xA1, attr)].concat();
        let infos = der(0x31, &der(0x30, &body));
        let sd = der(0x30, &[head, infos].concat());
        der(
            0x30,
            &[raw(ci.body, &oid).to_vec(), der(0xA0, &sd)].concat(),
        )
    }

    /// #1387 : certificat expiré mais signature horodatée (jeton RFC 3161)
    /// pendant sa validité : éditeur vérifié, comme sous Windows (Steam).
    /// Autorité de test datée de 2020 avec `openssl ca`, désignée par
    /// SSL_CERT_FILE le temps du test.
    #[test]
    fn signature_horodatee_au_certificat_expire() {
        if hostssl::ssl().is_none()
            || std::process::Command::new("openssl")
                .arg("version")
                .output()
                .is_err()
        {
            eprintln!("OpenSSL absent : horodatage non testé");
            return;
        }
        let dir = std::env::temp_dir().join(format!("winlux-horodatage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.txt"), "").unwrap();
        std::fs::write(dir.join("serial"), "1000\n").unwrap();
        std::fs::write(
            dir.join("ca.cnf"),
            "[ca]\ndefault_ca=d\n[d]\ndir=.\ndatabase=index.txt\nnew_certs_dir=.\nserial=serial\n\
             default_md=sha256\npolicy=p\nunique_subject=no\n[p]\ncommonName=supplied\n\
             [racine]\nbasicConstraints=critical,CA:true\nkeyUsage=keyCertSign,cRLSign\n\
             [code]\nbasicConstraints=CA:false\nextendedKeyUsage=codeSigning\n\
             [horodatage]\nbasicConstraints=CA:false\nextendedKeyUsage=critical,timeStamping\n",
        )
        .unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("openssl")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "openssl {args:?} : {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        let cert = |name: &str, cn: &str, end: &str, ext: &str, selfsign: bool| {
            let key = format!("{name}.key");
            let csr = format!("{name}.csr");
            let pem = format!("{name}.pem");
            let subj = format!("/CN={cn}");
            run(&[
                "req", "-new", "-newkey", "rsa:2048", "-nodes", "-keyout", &key, "-out", &csr,
                "-subj", &subj,
            ]);
            let mut args = vec![
                "ca",
                "-batch",
                "-config",
                "ca.cnf",
                "-notext",
                "-in",
                &csr,
                "-out",
                &pem,
                "-startdate",
                "20200101000000Z",
                "-enddate",
                end,
                "-extensions",
                ext,
            ];
            if selfsign {
                args.extend(["-selfsign", "-keyfile", &key]);
                run(&args);
            } else {
                args.extend(["-cert", "racine.pem", "-keyfile", "racine.key"]);
                run(&args);
            }
        };
        cert("racine", "Racine Winlux", "20400101000000Z", "racine", true);
        cert("code", "Testeur Winlux", "20220101000000Z", "code", false);
        cert(
            "tsa",
            "Horodatage Winlux",
            "20400101000000Z",
            "horodatage",
            false,
        );

        let pe = pe_minimal();
        std::fs::write(dir.join("spc.der"), spc_for(&pe)).unwrap();
        run(&[
            "cms",
            "-sign",
            "-binary",
            "-nodetach",
            "-outform",
            "DER",
            "-md",
            "sha256",
            "-econtent_type",
            "1.3.6.1.4.1.311.2.1.4",
            "-signer",
            "code.pem",
            "-inkey",
            "code.key",
            "-in",
            "spc.der",
            "-out",
            "sig.der",
        ]);
        let sig = std::fs::read(dir.join("sig.der")).unwrap();
        let outer = parse_signed_data(&sig).unwrap();
        // TSTInfo daté de 2021 (certificat encore valide), sur la signature.
        let tst = |imprint: &[u8]| {
            der(
                0x30,
                &[
                    der(0x02, &[1]),
                    der(0x06, &[0x2A, 0x03, 0x04]),
                    der(0x30, &[sha256_alg(), der(0x04, imprint)].concat()),
                    der(0x02, &[7]),
                    der(0x18, b"20210601120000Z"),
                ]
                .concat(),
            )
        };
        let token = |imprint: &[u8]| {
            std::fs::write(dir.join("tst.der"), tst(imprint)).unwrap();
            run(&[
                "cms",
                "-sign",
                "-binary",
                "-nodetach",
                "-outform",
                "DER",
                "-md",
                "sha256",
                "-econtent_type",
                "1.2.840.113549.1.9.16.1.4",
                "-signer",
                "tsa.pem",
                "-inkey",
                "tsa.key",
                "-in",
                "tst.der",
                "-out",
                "ts.der",
            ]);
            let ts = std::fs::read(dir.join("ts.der")).unwrap();
            let attr = der(
                0x30,
                &[der(0x06, OID_RFC3161_TIMESTAMP), der(0x31, &ts)].concat(),
            );
            attach(&pe, &with_unsigned_attr(&sig, &attr))
        };
        let good = token(&hostssl::digest(c"SHA256", &outer.signature).unwrap());
        let wrong = token(&[0u8; 32]);

        let previous = std::env::var_os("SSL_CERT_FILE");
        std::env::set_var("SSL_CERT_FILE", dir.join("racine.pem"));
        let plain = examine_pe_bytes(&attach(&pe, &sig));
        let stamped = examine_pe_bytes(&good);
        let mismatched = examine_pe_bytes(&wrong);
        match previous {
            Some(v) => std::env::set_var("SSL_CERT_FILE", v),
            None => std::env::remove_var("SSL_CERT_FILE"),
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            matches!(&plain, Signature::Unverified { reason, .. } if reason.contains("expiré")),
            "{plain:?}"
        );
        assert_eq!(
            stamped,
            Signature::Verified {
                signer: "Testeur Winlux".into()
            }
        );
        assert!(
            matches!(&mismatched, Signature::Unverified { reason, .. } if reason.contains("autre signature")),
            "{mismatched:?}"
        );
    }

    #[test]
    fn dates_asn1() {
        assert_eq!(der_time(0x18, b"19700101000000Z"), Some(0));
        assert_eq!(der_time(0x18, b"20210601120000.123Z"), Some(1_622_548_800));
        assert_eq!(der_time(0x17, b"210601120000Z"), Some(1_622_548_800));
        assert_eq!(der_time(0x17, b"991231235959Z"), Some(946_684_799));
        assert_eq!(der_time(0x18, b"2021060112Z"), None);
    }

    /// #1382 : l'éditeur d'un .exe signé est lu dans sa table de sécurité.
    /// Signature d'essai faite par la commande openssl (certificat
    /// autosigné : signature juste mais autorité inconnue).
    #[test]
    fn executable_signe_lu_et_verifie() {
        let pe = pe_minimal();
        assert_eq!(examine_pe_bytes(&pe), Signature::Absent);
        if hostssl::ssl().is_none() {
            eprintln!("OpenSSL du système absent : vérification non testée");
            return;
        }
        let (checksum, entry, _, _) = pe_layout(&pe).unwrap();
        let mut image = pe[..checksum].to_vec();
        image.extend_from_slice(&pe[checksum + 4..entry]);
        image.extend_from_slice(&pe[entry + 8..]);
        let digest = hostssl::digest(c"SHA256", &image).unwrap();
        // SpcIndirectDataContent : { { SPC_PE_IMAGE_DATAOBJ, SpcPeImageData },
        // { { sha256, NULL }, condensat } }.
        let oid_pe = der(
            0x06,
            &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x02, 0x01, 0x0F],
        );
        let attr = der(0x30, &[oid_pe, der(0x30, &der(0x03, &[0]))].concat());
        let sha256 = der(
            0x06,
            &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01],
        );
        let alg = der(0x30, &[sha256, vec![0x05, 0x00]].concat());
        let digest_info = der(0x30, &[alg, der(0x04, &digest)].concat());
        let spc = der(0x30, &[attr, digest_info].concat());

        let dir = std::env::temp_dir().join(format!("winlux-authenticode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let ok = std::process::Command::new("openssl")
                .args(args)
                .current_dir(&dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            assert!(ok, "openssl {args:?}");
        };
        if std::process::Command::new("openssl")
            .arg("version")
            .output()
            .is_err()
        {
            eprintln!("commande openssl absente : vérification non testée");
            return;
        }
        std::fs::write(dir.join("spc.der"), &spc).unwrap();
        run(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            "cle.pem",
            "-out",
            "cert.pem",
            "-days",
            "2",
            "-subj",
            "/CN=Testeur Winlux/O=Essai",
            "-addext",
            "extendedKeyUsage=codeSigning",
        ]);
        run(&[
            "cms",
            "-sign",
            "-binary",
            "-nodetach",
            "-outform",
            "DER",
            "-md",
            "sha256",
            "-econtent_type",
            "1.3.6.1.4.1.311.2.1.4",
            "-signer",
            "cert.pem",
            "-inkey",
            "cle.pem",
            "-in",
            "spc.der",
            "-out",
            "sig.der",
        ]);
        let sig = std::fs::read(dir.join("sig.der")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let mut table = Vec::new();
        let padded = (8 + sig.len()).div_ceil(8) * 8;
        table.extend_from_slice(&(padded as u32).to_le_bytes());
        table.extend_from_slice(&0x0200u16.to_le_bytes());
        table.extend_from_slice(&2u16.to_le_bytes());
        table.extend_from_slice(&sig);
        table.resize(padded, 0);
        let mut signed = pe.clone();
        let start = signed.len();
        signed.extend_from_slice(&table);
        signed[entry..entry + 4].copy_from_slice(&(start as u32).to_le_bytes());
        signed[entry + 4..entry + 8].copy_from_slice(&(padded as u32).to_le_bytes());

        match examine_pe_bytes(&signed) {
            Signature::Unverified { signer, reason } => {
                assert_eq!(signer, "Testeur Winlux");
                assert!(reason.contains("autorité"), "{reason}");
            }
            other => panic!("attendu : signature juste, autorité inconnue ; obtenu {other:?}"),
        }
        // Un octet changé après la signature : refusé.
        let mut altered = signed.clone();
        altered[0x185] ^= 1;
        assert!(matches!(examine_pe_bytes(&altered), Signature::Invalid(_)));
        // La somme de contrôle n'est pas couverte, comme sous Windows.
        let mut patched = signed;
        patched[checksum] ^= 0xFF;
        assert!(matches!(
            examine_pe_bytes(&patched),
            Signature::Unverified { .. }
        ));
    }

    #[test]
    fn noms_distingues() {
        let a = parse_dn(r#"CN="Exemple, SARL", O=Exemple, C=FR"#);
        assert_eq!(format_dn(&a), "CN=Exemple, SARL, O=Exemple, C=FR");
        // L'ordre des attributs et ST/S ne comptent pas.
        assert!(same_dn(
            &a,
            &parse_dn(r#"C=FR, O=Exemple, CN="Exemple, SARL""#)
        ));
        assert!(same_dn(
            &parse_dn("CN=x, ST=Paris"),
            &parse_dn("S=Paris, CN=x")
        ));
        assert!(!same_dn(&a, &parse_dn("CN=Exemple")));
    }

    #[test]
    fn avertissements() {
        assert!(warning(&Signature::Absent, "Test").starts_with(UNVERIFIED));
        let w = warning(
            &Signature::Unverified {
                signer: "CN=Test".into(),
                reason: "son certificat a expiré".into(),
            },
            "Test",
        );
        assert!(w.starts_with(UNVERIFIED) && w.contains("signé au nom de « CN=Test »"));
        assert!(w.contains("expiré"));
    }

    #[test]
    fn signature_illisible_refusee() {
        for bad in [&b"PKCX\x30\x05\x06"[..], b"n'importe quoi", b"PKCX"] {
            assert!(parse_p7x(bad).is_err());
        }
    }

    #[test]
    fn oid_inconnu_en_notation_pointee() {
        assert_eq!(
            attribute_name(&[0x2B, 0x06, 0x01, 0x04, 0x01]),
            "OID.1.3.6.1.4.1"
        );
        // MD5 : refusé ; SHA-1 n'est lu que pour les exécutables.
        assert!(
            digest_name(&[0x06, 0x08, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x02, 0x05]).is_err()
        );
        assert!(digest_name(&[0x06, 0x05, 0x2B, 0x0E, 0x03, 0x02, 0x1A]).is_ok());
        assert_eq!(
            digest_name(&[0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01])
                .unwrap()
                .1,
            32
        );
    }

    #[test]
    fn raisons_de_la_chaine() {
        assert!(chain_reason(20).contains("autorité"));
        assert!(chain_reason(10).contains("expiré"));
    }
}
