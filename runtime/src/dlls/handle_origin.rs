//! Diagnostic `WINLUX_DIAG_HANDLES=1` (#3004) : chaque numéro de handle neuf
//! est noté avec l'endroit du code de Winlux qui l'a créé (fichier:ligne), et
//! un DuplicateHandle refusé nomme cet endroit.
//!
//! Le bac à sable d'Acrobat (#236) meurt sur un DuplicateHandle d'un handle
//! que notre trace ne savait pas nommer (« autre »). Ce diagnostic ne déduit
//! rien : il rapporte l'appel créateur tel que le compilateur le situe
//! (`#[track_caller]`). Sans la variable, il ne coûte qu'une lecture.

use std::collections::HashMap;
use std::panic::Location;
use std::sync::{Mutex, OnceLock};

/// Variable qui active le diagnostic (valeur « 1 »).
pub const ENV: &str = "WINLUX_DIAG_HANDLES";

static ORIGINS: Mutex<Option<HashMap<usize, &'static Location<'static>>>> = Mutex::new(None);

fn parse(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| v == "1")
}

/// Le diagnostic est-il actif dans ce processus ?
pub fn active() -> bool {
    static ACTIVE: OnceLock<bool> = OnceLock::new();
    *ACTIVE.get_or_init(|| parse(std::env::var_os(ENV).as_deref()))
}

fn line(text: &str) {
    if crate::thunks::api_trace_active() {
        crate::thunks::api_note(text);
    } else {
        crate::log::write_diag(format!("{text}\n").as_bytes());
    }
}

fn remember(h: usize, at: &'static Location<'static>) {
    super::lock(&ORIGINS)
        .get_or_insert_with(HashMap::new)
        .insert(h, at);
}

fn tid() -> i64 {
    unsafe { libc::syscall(libc::SYS_gettid) }
}

/// Note la création du handle `h` à l'endroit qui a appelé la fonction
/// d'allocation (chaîne de `#[track_caller]`).
#[track_caller]
pub fn record(h: usize) {
    if !active() {
        return;
    }
    let at = Location::caller();
    remember(h, at);
    line(&format!(
        "winlux[diag-handles] {} handle {h:#x} créé par {}:{}",
        tid(),
        at.file(),
        at.line()
    ));
}

/// Endroit qui a créé `h`, s'il a été noté.
pub fn origin(h: usize) -> Option<String> {
    super::lock(&ORIGINS)
        .as_ref()?
        .get(&h)
        .map(|at| format!("{}:{}", at.file(), at.line()))
}

/// Une fonction refuse le handle `h` : nomme son créateur.
pub fn refused(h: usize, call: &str) {
    if !active() {
        return;
    }
    let from = origin(h).unwrap_or_else(|| "aucune création notée pour ce numéro".into());
    line(&format!(
        "winlux[diag-handles] {} {call} refuse le handle {h:#x}, créé par {from}",
        tid()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn allocate(h: usize) {
        remember(h, Location::caller());
    }

    /// #3004 : l'origine notée est l'appelant de la fonction d'allocation,
    /// pas la fonction elle-même ; seule la valeur « 1 » active le diagnostic.
    #[test]
    fn origine_d_un_handle_nomme_l_appelant_3004() {
        let h = 0x7ff0_0004;
        allocate(h); // ligne attendue
        let expected = format!("{}:{}", file!(), line!() - 1);
        assert_eq!(origin(h), Some(expected));
        assert_eq!(origin(0x7ff0_0008), None);
        assert!(parse(Some(std::ffi::OsStr::new("1"))));
        assert!(!parse(Some(std::ffi::OsStr::new("0"))));
        assert!(!parse(None));
    }
}
