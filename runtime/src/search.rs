//! Recherche d'une suite d'octets dans une image projetée.
//!
//! Le lancement de chaque .exe cherche des signatures dans son fichier
//! (installateur Inno, moteur SDL ou GLFW lié statiquement). Ces recherches
//! comparaient octet par octet : 45 % du temps processeur de l'ouverture de
//! l'Explorateur (10 Mo) y passaient (#2878). Ici, `memchr` de la glibc
//! (vectorisé) saute d'un premier octet possible au suivant, et la suite
//! n'est comparée qu'à ces endroits. `memmem` de la glibc, essayé d'abord,
//! rendait l'ouverture de l'Explorateur six fois plus lente (mesuré).

/// Position de la première occurrence de `needle` dans `hay`.
pub fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > hay.len() {
        return None;
    }
    let first = needle[0];
    let last_start = hay.len() - needle.len();
    let mut at = 0;
    while at <= last_start {
        let window = &hay[at..=last_start];
        // SAFETY : pointeur et longueur d'une tranche valide ; memchr ne lit
        // qu'à l'intérieur et ne garde aucun pointeur.
        let found = unsafe { libc::memchr(window.as_ptr().cast(), first as i32, window.len()) };
        if found.is_null() {
            return None;
        }
        let i = at + (found as usize - window.as_ptr() as usize);
        if hay[i + 1..i + needle.len()] == needle[1..] {
            return Some(i);
        }
        at = i + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::find;

    fn naive(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    #[test]
    fn meme_resultat_que_la_comparaison_octet_par_octet() {
        let mut hay = vec![0u8; 70_000];
        for (i, b) in hay.iter_mut().enumerate() {
            *b = (i * 7 % 251) as u8;
        }
        hay[65_000..65_006].copy_from_slice(b"GLFW30");
        for needle in [
            &b"GLFW30"[..],
            b"\x00\x07\x0e",
            b"absent!",
            b"G",
            &hay[69_990..],
        ] {
            assert_eq!(find(&hay, needle), naive(&hay, needle), "{needle:?}");
        }
        assert_eq!(find(b"abc", b""), Some(0));
        assert_eq!(find(b"ab", b"abc"), None);
        assert_eq!(find(b"", b"a"), None);
    }

    /// Mesure : 32 Mio sans la signature, comme le .rdata d'un gros
    /// programme. La comparaison octet par octet y mettait plusieurs
    /// secondes en build de test ; la recherche par memchr bien moins de
    /// 0,5 s, même sans optimisation (#2878).
    #[test]
    fn recherche_de_32_mio_rapide() {
        let mut hay = vec![0u8; 32 << 20];
        for b in hay.iter_mut().step_by(97) {
            *b = b'G';
        }
        let start = std::time::Instant::now();
        assert_eq!(find(&hay, b"GLFW30"), None);
        assert_eq!(find(&hay, b"rDlPtS\xcd\xe6\xd7\x7b\x0b\x2a"), None);
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 500, "{elapsed:?}");
    }
}
