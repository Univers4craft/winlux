//! CryptMsgGetParam(CMSG_SIGNER_INFO_PARAM) : SignerInfo d'un message
//! PKCS #7 décodé, rendu en `CMSG_SIGNER_INFO` (#2689).
//!
//! Disposition de `CMSG_SIGNER_INFO`, `CRYPT_ATTRIBUTES`, `CRYPT_ATTRIBUTE`
//! et des blobs : windows-rs (Win32::Security::Cryptography). Tous les
//! champs y occupent une ou plusieurs cases de la taille d'un pointeur
//! (DWORD suivi de son remplissage) : 17 cases (136 octets en x64, 68 en
//! x86), 3 par attribut. Comme le crypt32 de Windows (documentation de
//! CryptMsgGetParam : « pvData … receives the data »), la structure et
//! tout ce qu'elle désigne tiennent dans le tampon de l'appelant.
//!
//! Champs (RFC 2315 § 9.2, SignerInfo) : version ; émetteur (Name DER
//! complet) et numéro de série (petit-boutiste, comme
//! `CERT_INFO.SerialNumber`) ; algorithmes de condensat et de chiffrement
//! (OID en notation pointée, paramètres DER) ; signature (EncryptedHash,
//! octets tels qu'encodés : inférence, l'ordre n'est pas documenté) ;
//! attributs authentifiés et non authentifiés (chaque valeur en DER).

use super::crypt32::{children, oid_str, tlv, Tlv};

/// SignerInfo `index` du SignedData `der` (ContentInfo complet).
fn signer_infos(der: &[u8]) -> Option<Vec<Tlv<'_>>> {
    let (ci, _) = tlv(der)?;
    let ci = children(ci.body);
    let explicit = ci.get(1).filter(|t| t.tag == 0xa0)?;
    let (signed, _) = tlv(explicit.body)?;
    let set = children(signed.body)
        .into_iter()
        .rev()
        .find(|t| t.tag == 0x31)?;
    Some(children(set.body))
}

struct Packer {
    p: usize,
    buf: Vec<u8>,
    /// (case du pointeur, décalage de la donnée désignée).
    fixups: Vec<(usize, usize)>,
}

impl Packer {
    fn new(p: usize, slots: usize) -> Self {
        Packer {
            p,
            buf: vec![0; slots * p],
            fixups: Vec::new(),
        }
    }
    fn reserve(&mut self, slots: usize) -> usize {
        let at = self.buf.len().div_ceil(self.p) * self.p;
        self.buf.resize(at + slots * self.p, 0);
        at
    }
    fn data(&mut self, bytes: &[u8]) -> usize {
        let at = self.buf.len().div_ceil(self.p) * self.p;
        self.buf.resize(at, 0);
        self.buf.extend_from_slice(bytes);
        at
    }
    fn u32_at(&mut self, at: usize, v: u32) {
        self.buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn ptr_at(&mut self, at: usize, target: usize) {
        self.fixups.push((at, target));
    }
    /// Blob { cb, pb } dans les cases `at` et `at + p`.
    fn blob(&mut self, at: usize, bytes: &[u8]) {
        self.u32_at(at, bytes.len() as u32);
        if !bytes.is_empty() {
            let d = self.data(bytes);
            self.ptr_at(at + self.p, d);
        }
    }
    fn cstr(&mut self, at: usize, s: &str) {
        let mut z = s.as_bytes().to_vec();
        z.push(0);
        let d = self.data(&z);
        self.ptr_at(at, d);
    }
    /// CRYPT_ALGORITHM_IDENTIFIER { pszObjId, Parameters }.
    fn alg(&mut self, at: usize, seq: &Tlv) {
        let c = children(seq.body);
        let oid = c.first().map(|o| oid_str(o.body)).unwrap_or_default();
        self.cstr(at, &oid);
        self.blob(at + self.p, c.get(1).map(|t| t.all).unwrap_or(&[]));
    }
    /// CRYPT_ATTRIBUTES { cAttr, rgAttr } d'un [0] ou [1] IMPLICIT SET.
    fn attrs(&mut self, at: usize, set: Option<&Tlv>) {
        let Some(set) = set else {
            return;
        };
        let list = children(set.body);
        self.u32_at(at, list.len() as u32);
        if list.is_empty() {
            return;
        }
        let rg = self.reserve(3 * list.len());
        self.ptr_at(at + self.p, rg);
        for (i, a) in list.iter().enumerate() {
            let slot = rg + 3 * self.p * i;
            let c = children(a.body);
            let oid = c.first().map(|o| oid_str(o.body)).unwrap_or_default();
            self.cstr(slot, &oid);
            let values = c.get(1).map(|v| children(v.body)).unwrap_or_default();
            self.u32_at(slot + self.p, values.len() as u32);
            if values.is_empty() {
                continue;
            }
            let rv = self.reserve(2 * values.len());
            self.ptr_at(slot + 2 * self.p, rv);
            for (j, v) in values.iter().enumerate() {
                self.blob(rv + 2 * self.p * j, v.all);
            }
        }
    }
}

/// CMSG_SIGNER_INFO du signataire `index`, empaqueté pour des pointeurs
/// de `p` octets : octets et liste des cases à reloger (décalage du
/// pointeur, décalage visé).
pub(crate) fn signer_info(
    der: &[u8],
    index: usize,
    p: usize,
) -> Option<(Vec<u8>, Vec<(usize, usize)>)> {
    let infos = signer_infos(der)?;
    let si = infos.get(index)?;
    let mut f = children(si.body).into_iter().peekable();
    let version = f.next().filter(|t| t.tag == 0x02)?;
    let sid = f.next()?;
    let (issuer, serial) = match sid.tag {
        0x30 => {
            let c = children(sid.body);
            let issuer = c.first().filter(|t| t.tag == 0x30)?.all;
            let serial = c.get(1).filter(|t| t.tag == 0x02)?.body;
            (issuer, serial)
        }
        // subjectKeyIdentifier (CMS) : Windows le rend sous un nom
        // szOID_KEYID_RDN, non pris en charge ici.
        _ => return None,
    };
    let digest = f.next().filter(|t| t.tag == 0x30)?;
    let auth = f.next_if(|t| t.tag == 0xa0);
    let enc = f.next().filter(|t| t.tag == 0x30)?;
    let sig = f.next().filter(|t| t.tag == 0x04)?;
    let unauth = f.next_if(|t| t.tag == 0xa1);

    let mut k = Packer::new(p, 17);
    let v = version.body.iter().fold(0u32, |a, &b| (a << 8) | b as u32);
    k.u32_at(0, v);
    k.blob(p, issuer);
    let serial_le: Vec<u8> = serial.iter().rev().copied().collect();
    k.blob(3 * p, &serial_le);
    k.alg(5 * p, &digest);
    k.alg(8 * p, &enc);
    k.blob(11 * p, sig.body);
    k.attrs(13 * p, auth.as_ref());
    k.attrs(15 * p, unauth.as_ref());
    Some((k.buf, k.fixups))
}

/// Recopie dans le tampon de l'appelant, pointeurs relogés.
///
/// # Safety
/// `data` désigne au moins `bytes.len()` octets inscriptibles.
pub(crate) unsafe fn write_packed(
    bytes: &[u8],
    fixups: &[(usize, usize)],
    p: usize,
    data: *mut u8,
) {
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
    for &(at, target) in fixups {
        let addr = data as usize + target;
        if p == 4 {
            data.add(at).cast::<u32>().write_unaligned(addr as u32);
        } else {
            data.add(at).cast::<u64>().write_unaligned(addr as u64);
        }
    }
}

#[cfg(test)]
#[path = "crypt32_signer/tests.rs"]
mod tests;
