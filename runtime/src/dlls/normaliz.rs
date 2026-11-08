//! normaliz.dll : noms de domaine internationaux (IDNA, RFC 3490, avec le
//! Punycode de la RFC 3492).
//!
//! Avant chaque négociation TLS, `SslStream` de .NET normalise le nom d'hôte
//! par `IdnMapping.GetAscii`, qui appelle `Normaliz.dll!IdnToAscii` même pour
//! un nom déjà en ASCII. Sans cette DLL, tout HTTPS d'une application .NET
//! échouait (« The SSL connection could not be established »
//! ← DllNotFoundException, #1183). kernel32 exporte les mêmes fonctions.
//!
//! L'encodage (`IdnToAscii`) vit dans ftp_apps.rs depuis FileZilla ; le
//! décodage (`IdnToUnicode`) suit la section 6.2 de la RFC 3492.

use super::ftp_apps::{idn_adapt, idn_to_ascii_w, IDN_BASE, IDN_TMAX, IDN_TMIN};

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_NAME: u32 = 123;

/// Décodage Punycode d'une étiquette sans son préfixe « xn-- »
/// (RFC 3492, section 6.2). `None` si l'entrée est mal formée.
pub fn punycode_decode(input: &str) -> Option<String> {
    if !input.is_ascii() {
        return None;
    }
    let (basic, rest) = match input.rfind('-') {
        Some(at) => (&input[..at], &input[at + 1..]),
        None => ("", input),
    };
    let mut output: Vec<u32> = basic.chars().map(|c| c as u32).collect();
    let (mut n, mut i, mut bias) = (128u32, 0u32, 72u32);
    let mut digits = rest.bytes().peekable();
    while digits.peek().is_some() {
        let old_i = i;
        let mut w = 1u32;
        let mut k = IDN_BASE;
        loop {
            let digit = match digits.next()? {
                c @ b'a'..=b'z' => (c - b'a') as u32,
                c @ b'A'..=b'Z' => (c - b'A') as u32,
                c @ b'0'..=b'9' => (c - b'0') as u32 + 26,
                _ => return None,
            };
            i = i.checked_add(digit.checked_mul(w)?)?;
            let t = if k <= bias {
                IDN_TMIN
            } else if k >= bias + IDN_TMAX {
                IDN_TMAX
            } else {
                k - bias
            };
            if digit < t {
                break;
            }
            w = w.checked_mul(IDN_BASE - t)?;
            k += IDN_BASE;
        }
        let points = output.len() as u32 + 1;
        bias = idn_adapt(i - old_i, points, old_i == 0);
        n = n.checked_add(i / points)?;
        i %= points;
        char::from_u32(n)?;
        output.insert(i as usize, n);
        i += 1;
    }
    output.into_iter().map(char::from_u32).collect()
}

/// Nom de domaine en Unicode : chaque étiquette « xn--… » est décodée, les
/// autres restent telles quelles (RFC 3490, ToUnicode).
pub fn idn_to_unicode(name: &str) -> Option<String> {
    let labels: Option<Vec<String>> = name
        .split('.')
        .map(|label| match label.get(..4) {
            Some(prefix) if prefix.eq_ignore_ascii_case("xn--") => {
                let decoded = punycode_decode(&label[4..])?;
                // Le décodage doit redonner l'étiquette (RFC 3490, 4.2, étape 6).
                let back = super::ftp_apps::punycode(&decoded.to_lowercase())?;
                back.eq_ignore_ascii_case(&label[4..]).then_some(decoded)
            }
            _ => Some(label.to_string()),
        })
        .collect();
    labels.map(|l| l.join("."))
}

/// `IdnToUnicode(dwFlags, lpASCIICharStr, cchASCIIChar, lpUnicodeCharStr,
/// cchUnicodeChar)` : nombre de caractères écrits (ou nécessaires).
pub(crate) unsafe extern "win64" fn idn_to_unicode_w(
    _flags: u32,
    input: *const u16,
    count: i32,
    out: *mut u16,
    out_len: i32,
) -> i32 {
    if input.is_null() || count == 0 || out_len < 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (units, terminated) = if count < 0 {
        (crate::printf::wide_len(input), true)
    } else {
        (count as usize, false)
    };
    let Ok(text) = String::from_utf16(std::slice::from_raw_parts(input, units)) else {
        crate::process::set_last_error(ERROR_INVALID_NAME);
        return 0;
    };
    let Some(unicode) = idn_to_unicode(&text) else {
        crate::process::set_last_error(ERROR_INVALID_NAME);
        return 0;
    };
    let mut wide: Vec<u16> = unicode.encode_utf16().collect();
    if terminated {
        wide.push(0);
    }
    if out_len == 0 {
        return wide.len() as i32;
    }
    if out.is_null() || (out_len as usize) < wide.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), out, wide.len());
    wide.len() as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "IdnToAscii" => fn idn_to_ascii_w,
        "IdnToUnicode" => fn idn_to_unicode_w,
    }
}

/// Les mêmes fonctions, exportées aussi par kernel32.
pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "IdnToUnicode" => fn idn_to_unicode_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call(
        f: unsafe extern "win64" fn(u32, *const u16, i32, *mut u16, i32) -> i32,
        text: &str,
    ) -> Option<String> {
        let input: Vec<u16> = text.encode_utf16().collect();
        // Comme .NET : longueur sans zéro final, taille demandée d'abord.
        let need = f(
            0,
            input.as_ptr(),
            input.len() as i32,
            std::ptr::null_mut(),
            0,
        );
        if need == 0 {
            return None;
        }
        let mut out = vec![0u16; need as usize];
        let written = f(
            0,
            input.as_ptr(),
            input.len() as i32,
            out.as_mut_ptr(),
            need,
        );
        assert_eq!(written, need);
        Some(String::from_utf16_lossy(&out))
    }

    /// Ce que fait SslStream avant TLS (#1183) : un nom ASCII passe, un nom
    /// accentué devient « xn--… », et IdnToUnicode fait le chemin inverse.
    #[test]
    fn noms_de_domaine_pour_le_tls_de_dotnet() {
        unsafe {
            assert_eq!(
                call(idn_to_ascii_w, "api.github.com").as_deref(),
                Some("api.github.com")
            );
            assert_eq!(
                call(idn_to_ascii_w, "bücher.example").as_deref(),
                Some("xn--bcher-kva.example")
            );
            assert_eq!(
                call(idn_to_unicode_w, "xn--bcher-kva.example").as_deref(),
                Some("bücher.example")
            );
            assert_eq!(
                call(idn_to_unicode_w, "raw.githubusercontent.com").as_deref(),
                Some("raw.githubusercontent.com")
            );
            // Étiquette Punycode mal formée : ERROR_INVALID_NAME.
            assert_eq!(call(idn_to_unicode_w, "xn--a-é.example"), None);
            assert_eq!(call(idn_to_unicode_w, "xn--99999999999999.example"), None);
            // Tampon trop petit.
            let input: Vec<u16> = "xn--bcher-kva".encode_utf16().collect();
            let mut small = [0u16; 2];
            assert_eq!(
                idn_to_unicode_w(0, input.as_ptr(), input.len() as i32, small.as_mut_ptr(), 2),
                0
            );
        }
    }

    #[test]
    fn decodage_punycode_exemples_de_la_rfc_3492() {
        // RFC 3492, 7.1 (L) et un aller-retour avec l'encodeur existant.
        assert_eq!(
            punycode_decode("3B-ww4c5e180e575a65lsy2b").as_deref(),
            Some("3年B組金八先生")
        );
        assert_eq!(punycode_decode("bcher-kva").as_deref(), Some("bücher"));
        for word in ["münchen", "日本語", "ελληνικά", "abc"] {
            let encoded = super::super::ftp_apps::punycode(word).unwrap();
            assert_eq!(punycode_decode(&encoded).as_deref(), Some(word));
        }
    }
}
