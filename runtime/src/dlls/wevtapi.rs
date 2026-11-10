//! wevtapi : journal des événements de Windows Vista et suivants (#1393).
//!
//! win32evtlog (pywin32) l'importe en différé. Winlux ne tient pas de
//! journal d'événements au format de Windows : son journal est un fichier
//! texte (`journal.rs`, lu par l'Observateur d'événements). Cette DLL
//! répond donc comme un Windows sans aucun canal ni fournisseur :
//! - les énumérations (canaux, fournisseurs, métadonnées, résultats) sont
//!   vides et finissent par ERROR_NO_MORE_ITEMS ;
//! - ouvrir, interroger ou s'abonner à un canal rend
//!   ERROR_EVT_CHANNEL_NOT_FOUND, un fournisseur
//!   ERROR_EVT_PUBLISHER_METADATA_NOT_FOUND ;
//! - les sessions distantes et les fichiers .evtx ne sont pas pris en
//!   charge (ERROR_NOT_SUPPORTED) ;
//! - aucune fonction ne prétend avoir effacé, exporté ou archivé un journal.
//!
//! Écrit d'après la documentation publique de l'API Windows Event Log.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_EVT_PUBLISHER_METADATA_NOT_FOUND: u32 = 15002;
const ERROR_EVT_CHANNEL_NOT_FOUND: u32 = 15007;

/// EvtQueryFilePath, EvtOpenFilePath : le chemin désigne un fichier.
const EVT_FILE_PATH: u32 = 2;

/// Nature d'une poignée EVT_HANDLE rendue par cette DLL.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    ChannelEnum,
    PublisherEnum,
    RenderContext,
    Bookmark,
}

fn handles() -> &'static Mutex<HashMap<usize, Kind>> {
    static H: OnceLock<Mutex<HashMap<usize, Kind>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<usize, Kind>> {
    handles().lock().unwrap_or_else(|e| e.into_inner())
}

/// Nouvelle poignée, multiple de 4 et sous 4 Gio (programmes x86, #1360).
fn new_handle(kind: Kind) -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let h = 0x5EE0_0000 + 4 * (n % 0x0400_0000);
    lock().insert(h, kind);
    h
}

fn is(h: usize, kind: Kind) -> bool {
    lock().get(&h) == Some(&kind)
}

fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}

fn none(code: u32) -> usize {
    crate::process::set_last_error(code);
    0
}

/// Session locale (0) seulement : pas de journal à distance.
fn local(session: usize) -> bool {
    session == 0
}

pub extern "win64" fn evt_open_session(
    _class: u32,
    _login: usize,
    _timeout: u32,
    _flags: u32,
) -> usize {
    none(ERROR_NOT_SUPPORTED)
}

pub extern "win64" fn evt_close(h: usize) -> i32 {
    if lock().remove(&h).is_some() {
        crate::process::set_last_error(0);
        1
    } else {
        fail(ERROR_INVALID_HANDLE)
    }
}

/// Texte de la dernière erreur : vide, aucune n'en porte ici.
pub unsafe extern "win64" fn evt_get_extended_status(
    size: u32,
    buffer: *mut u16,
    used: *mut u32,
) -> u32 {
    if !used.is_null() {
        std::ptr::write_unaligned(used, 1);
    }
    if size >= 1 && !buffer.is_null() {
        std::ptr::write_unaligned(buffer, 0);
    }
    0
}

/// Canal (ou fichier avec EVT_FILE_PATH) introuvable.
fn no_source(session: usize, flags: u32) -> usize {
    if !local(session) || flags & EVT_FILE_PATH != 0 {
        none(ERROR_NOT_SUPPORTED)
    } else {
        none(ERROR_EVT_CHANNEL_NOT_FOUND)
    }
}

pub extern "win64" fn evt_query(
    session: usize,
    _path: *const u16,
    _query: *const u16,
    flags: u32,
) -> usize {
    no_source(session, flags)
}

pub extern "win64" fn evt_open_log(session: usize, _path: *const u16, flags: u32) -> usize {
    no_source(session, flags)
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn evt_subscribe(
    session: usize,
    _signal: usize,
    _channel: *const u16,
    _query: *const u16,
    _bookmark: usize,
    _context: usize,
    _callback: usize,
    _flags: u32,
) -> usize {
    no_source(session, 0)
}

pub extern "win64" fn evt_open_channel_config(
    session: usize,
    _channel: *const u16,
    _flags: u32,
) -> usize {
    no_source(session, 0)
}

pub extern "win64" fn evt_open_publisher_metadata(
    session: usize,
    _publisher: *const u16,
    _log_file: *const u16,
    _locale: u32,
    _flags: u32,
) -> usize {
    if !local(session) {
        return none(ERROR_NOT_SUPPORTED);
    }
    none(ERROR_EVT_PUBLISHER_METADATA_NOT_FOUND)
}

/// Aucun résultat, événement ni métadonnée n'existe : toute poignée de ce
/// genre est invalide.
pub extern "win64" fn evt_next(
    _results: usize,
    _size: u32,
    _events: usize,
    _timeout: u32,
    _flags: u32,
    returned: *mut u32,
) -> i32 {
    if !returned.is_null() {
        // SAFETY : pointeur de sortie fourni par l'appelant.
        unsafe { std::ptr::write_unaligned(returned, 0) };
    }
    fail(ERROR_INVALID_HANDLE)
}

pub extern "win64" fn evt_seek(
    _results: usize,
    _position: i64,
    _bookmark: usize,
    _timeout: u32,
    _flags: u32,
) -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

pub extern "win64" fn evt_create_render_context(_count: u32, _paths: usize, _flags: u32) -> usize {
    new_handle(Kind::RenderContext)
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn evt_render(
    _context: usize,
    _fragment: usize,
    _flags: u32,
    _size: u32,
    _buffer: usize,
    _used: *mut u32,
    _count: *mut u32,
) -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn evt_format_message(
    _metadata: usize,
    _event: usize,
    _message_id: u32,
    _value_count: u32,
    _values: usize,
    _flags: u32,
    _size: u32,
    _buffer: usize,
    _used: *mut u32,
) -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

pub extern "win64" fn evt_open_channel_enum(session: usize, _flags: u32) -> usize {
    if !local(session) {
        return none(ERROR_NOT_SUPPORTED);
    }
    new_handle(Kind::ChannelEnum)
}

pub extern "win64" fn evt_open_publisher_enum(session: usize, _flags: u32) -> usize {
    if !local(session) {
        return none(ERROR_NOT_SUPPORTED);
    }
    new_handle(Kind::PublisherEnum)
}

/// Fin d'une énumération vide, ou poignée d'un autre genre.
fn next_of(h: usize, kind: Kind, used: *mut u32) -> i32 {
    if !used.is_null() {
        // SAFETY : pointeur de sortie fourni par l'appelant.
        unsafe { std::ptr::write_unaligned(used, 0) };
    }
    if is(h, kind) {
        fail(ERROR_NO_MORE_ITEMS)
    } else {
        fail(ERROR_INVALID_HANDLE)
    }
}

pub extern "win64" fn evt_next_channel_path(
    h: usize,
    _size: u32,
    _buffer: usize,
    used: *mut u32,
) -> i32 {
    next_of(h, Kind::ChannelEnum, used)
}

pub extern "win64" fn evt_next_publisher_id(
    h: usize,
    _size: u32,
    _buffer: usize,
    used: *mut u32,
) -> i32 {
    next_of(h, Kind::PublisherEnum, used)
}

/// Propriété d'un objet que cette DLL ne rend jamais (canal, fournisseur,
/// événement, tableau de métadonnées) : poignée invalide.
pub extern "win64" fn evt_get_property(
    _h: usize,
    _id: u32,
    _flags: u32,
    _size: u32,
    _buffer: usize,
    _used: *mut u32,
) -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

pub extern "win64" fn evt_open_event_metadata_enum(_metadata: usize, _flags: u32) -> usize {
    none(ERROR_INVALID_HANDLE)
}

pub extern "win64" fn evt_next_event_metadata(_h: usize, _flags: u32) -> usize {
    none(ERROR_INVALID_HANDLE)
}

pub extern "win64" fn evt_get_object_array_size(_array: usize, size: *mut u32) -> i32 {
    if !size.is_null() {
        // SAFETY : pointeur de sortie fourni par l'appelant.
        unsafe { std::ptr::write_unaligned(size, 0) };
    }
    fail(ERROR_INVALID_HANDLE)
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn evt_get_object_array_property(
    _array: usize,
    _id: u32,
    _index: u32,
    _flags: u32,
    _size: u32,
    _buffer: usize,
    _used: *mut u32,
) -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

/// EvtGetLogInfo et EvtGetEventInfo : (poignée, propriété, taille, tampon,
/// taille utilisée).
pub extern "win64" fn evt_get_info(
    _h: usize,
    _id: u32,
    _size: u32,
    _buffer: usize,
    _used: *mut u32,
) -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

pub extern "win64" fn evt_clear_log(
    session: usize,
    _channel: *const u16,
    _target: *const u16,
    _flags: u32,
) -> i32 {
    if !local(session) {
        return fail(ERROR_NOT_SUPPORTED);
    }
    fail(ERROR_EVT_CHANNEL_NOT_FOUND)
}

pub extern "win64" fn evt_export_log(
    session: usize,
    _path: *const u16,
    _query: *const u16,
    _target: *const u16,
    flags: u32,
) -> i32 {
    if !local(session) || flags & EVT_FILE_PATH != 0 {
        return fail(ERROR_NOT_SUPPORTED);
    }
    fail(ERROR_EVT_CHANNEL_NOT_FOUND)
}

pub extern "win64" fn evt_archive_exported_log(
    session: usize,
    _path: *const u16,
    _locale: u32,
    _flags: u32,
) -> i32 {
    if !local(session) {
        return fail(ERROR_NOT_SUPPORTED);
    }
    fail(ERROR_NOT_SUPPORTED)
}

/// Signet : vide au départ ; sans événement à y ranger, il le reste.
pub extern "win64" fn evt_create_bookmark(_xml: *const u16) -> usize {
    new_handle(Kind::Bookmark)
}

pub extern "win64" fn evt_update_bookmark(bookmark: usize, _event: usize) -> i32 {
    if !is(bookmark, Kind::Bookmark) {
        return fail(ERROR_INVALID_HANDLE);
    }
    // Aucun événement ne peut exister ici.
    fail(ERROR_INVALID_HANDLE)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "EvtOpenSession" => fn evt_open_session,
        "EvtClose" => fn evt_close,
        "EvtGetExtendedStatus" => fn evt_get_extended_status,
        "EvtQuery" => fn evt_query,
        "EvtOpenLog" => fn evt_open_log,
        "EvtSubscribe" => fn evt_subscribe,
        "EvtOpenChannelConfig" => fn evt_open_channel_config,
        "EvtOpenPublisherMetadata" => fn evt_open_publisher_metadata,
        "EvtNext" => fn evt_next,
        "EvtSeek" => fn evt_seek,
        "EvtCreateRenderContext" => fn evt_create_render_context,
        "EvtRender" => fn evt_render,
        "EvtFormatMessage" => fn evt_format_message,
        "EvtOpenChannelEnum" => fn evt_open_channel_enum,
        "EvtNextChannelPath" => fn evt_next_channel_path,
        "EvtOpenPublisherEnum" => fn evt_open_publisher_enum,
        "EvtNextPublisherId" => fn evt_next_publisher_id,
        "EvtGetChannelConfigProperty" => fn evt_get_property,
        "EvtGetPublisherMetadataProperty" => fn evt_get_property,
        "EvtGetEventMetadataProperty" => fn evt_get_property,
        "EvtOpenEventMetadataEnum" => fn evt_open_event_metadata_enum,
        "EvtNextEventMetadata" => fn evt_next_event_metadata,
        "EvtGetObjectArraySize" => fn evt_get_object_array_size,
        "EvtGetObjectArrayProperty" => fn evt_get_object_array_property,
        "EvtGetLogInfo" => fn evt_get_info,
        "EvtGetEventInfo" => fn evt_get_info,
        "EvtClearLog" => fn evt_clear_log,
        "EvtExportLog" => fn evt_export_log,
        "EvtArchiveExportedLog" => fn evt_archive_exported_log,
        "EvtCreateBookmark" => fn evt_create_bookmark,
        "EvtUpdateBookmark" => fn evt_update_bookmark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_sans_canal_ni_fournisseur() {
        let _teb = crate::process::test_teb();
        // Fonctions importées en différé par win32evtlog (pywin32 312).
        let e = exports();
        for n in [
            "EvtSubscribe",
            "EvtQuery",
            "EvtExportLog",
            "EvtGetPublisherMetadataProperty",
            "EvtGetLogInfo",
            "EvtCreateRenderContext",
            "EvtClose",
            "EvtGetEventMetadataProperty",
            "EvtGetObjectArraySize",
            "EvtFormatMessage",
            "EvtClearLog",
            "EvtOpenSession",
            "EvtOpenPublisherEnum",
            "EvtArchiveExportedLog",
            "EvtOpenLog",
            "EvtOpenChannelConfig",
            "EvtGetEventInfo",
            "EvtGetExtendedStatus",
            "EvtOpenEventMetadataEnum",
            "EvtRender",
            "EvtNextChannelPath",
            "EvtGetObjectArrayProperty",
            "EvtUpdateBookmark",
            "EvtGetChannelConfigProperty",
            "EvtNextPublisherId",
            "EvtSeek",
            "EvtNextEventMetadata",
            "EvtOpenChannelEnum",
            "EvtCreateBookmark",
            "EvtNext",
            "EvtOpenPublisherMetadata",
        ] {
            assert!(e.contains_key(n), "{n}");
        }
        assert_eq!(crate::dlls::normalize("wevtapi.dll"), "wevtapi");
        let ch = evt_open_channel_enum(0, 0);
        assert!(ch != 0 && ch % 4 == 0 && ch < 1 << 32);
        let mut used = 7u32;
        assert_eq!(evt_next_channel_path(ch, 0, 0, &mut used), 0);
        assert_eq!(
            (crate::process::last_error(), used),
            (ERROR_NO_MORE_ITEMS, 0)
        );
        assert_eq!(evt_next_publisher_id(ch, 0, 0, &mut used), 0);
        assert_eq!(crate::process::last_error(), ERROR_INVALID_HANDLE);
        assert_eq!(evt_close(ch), 1);
        assert_eq!(evt_close(ch), 0);
        let app: Vec<u16> = "Application\0".encode_utf16().collect();
        assert_eq!(evt_query(0, app.as_ptr(), std::ptr::null(), 1), 0);
        assert_eq!(crate::process::last_error(), ERROR_EVT_CHANNEL_NOT_FOUND);
        assert_eq!(
            evt_query(0, app.as_ptr(), std::ptr::null(), EVT_FILE_PATH),
            0
        );
        assert_eq!(crate::process::last_error(), ERROR_NOT_SUPPORTED);
        assert_eq!(evt_clear_log(0, app.as_ptr(), std::ptr::null(), 0), 0);
        assert_eq!(crate::process::last_error(), ERROR_EVT_CHANNEL_NOT_FOUND);
        let b = evt_create_bookmark(std::ptr::null());
        assert_eq!(evt_update_bookmark(b, 0), 0);
        assert_eq!(evt_close(b), 1);
    }
}
