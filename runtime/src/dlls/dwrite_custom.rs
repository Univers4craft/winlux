//! Polices fournies par l'application (#1075) : IDWriteFactory::
//! CreateCustomFontCollection et CreateCustomFontFileReference.
//!
//! WPF (FontFamily par URI `pack://` ou `file://`), et toute appli qui livre
//! ses polices, donne à DirectWrite son propre chargeur de collection
//! (IDWriteFontCollectionLoader), son énumérateur de fichiers et son chargeur
//! de fichiers (IDWriteFontFileLoader → IDWriteFontFileStream). Le catalogue
//! de Winlux travaille sur des chemins : les octets lus par le flux de
//! l'application sont copiés une fois dans un dossier privé du processus,
//! puis la police se comporte comme un fichier local. Ces octets sont des
//! données non vérifiées : seul l'analyseur sfnt de Winlux les lit.
//!
//! Ordre des méthodes : `dwrite.h` (MinGW-w64, domaine public).

use super::{
    group, info, method, put, Com, FaceInfo, FactoryObj, FileObj, COLLECTION_VT, E_INVALIDARG,
    E_POINTER, FILE_VT, LOADER_VT,
};
use std::ffi::c_void;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// DWRITE_E_FILENOTFOUND.
const DWRITE_E_FILENOTFOUND: i32 = 0x8898_5003u32 as i32;
/// Au-delà, le fichier n'est pas une police raisonnable.
const MAX_FONT_FILE: u64 = 256 << 20;
/// Garde-fou contre un énumérateur qui ne s'arrête pas.
const MAX_FILES: usize = 4096;

type Release = unsafe extern "win64" fn(*mut c_void) -> u32;

unsafe fn com_release(p: *mut c_void) {
    if !p.is_null() {
        method::<Release>(p, 2)(p);
    }
}

/// Chemin Linux d'un IDWriteFontFile : directement pour un fichier Winlux,
/// sinon les octets que son chargeur sert, copiés dans le cache.
pub(super) unsafe fn file_path(file: *mut c_void) -> Option<PathBuf> {
    if file.is_null() {
        return None;
    }
    if *(file as *const *const ()) == FILE_VT.ptr() {
        return Some(Com::get(file as *mut FileObj).clone());
    }
    type Key = unsafe extern "win64" fn(*mut c_void, *mut *const c_void, *mut u32) -> i32;
    type GetLoader = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
    let mut key = std::ptr::null();
    let mut size = 0u32;
    if method::<Key>(file, 3)(file, &mut key, &mut size) < 0 {
        return None;
    }
    let mut loader = std::ptr::null_mut();
    if method::<GetLoader>(file, 4)(file, &mut loader) < 0 || loader.is_null() {
        return None;
    }
    let path = loader_path(loader, key, size);
    com_release(loader);
    path
}

/// Fichier désigné par (chargeur, clé) : la clé d'un chargeur Winlux est
/// déjà un chemin ; sinon on lit le flux de l'application.
unsafe fn loader_path(loader: *mut c_void, key: *const c_void, size: u32) -> Option<PathBuf> {
    if *(loader as *const *const ()) == LOADER_VT.ptr() {
        if key.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(key as *const u8, size as usize);
        let p = PathBuf::from(std::ffi::OsStr::from_encoded_bytes_unchecked(bytes));
        return p.exists().then_some(p);
    }
    type Create =
        unsafe extern "win64" fn(*mut c_void, *const c_void, u32, *mut *mut c_void) -> i32;
    let mut stream = std::ptr::null_mut();
    if method::<Create>(loader, 3)(loader, key, size, &mut stream) < 0 || stream.is_null() {
        crate::log_warn!("dwrite", "police de l'appli : CreateStreamFromKey refusé");
        return None;
    }
    let data = read_stream(stream);
    com_release(stream);
    cache(&data?)
}

/// Lit tout un IDWriteFontFileStream (GetFileSize puis ReadFileFragment).
unsafe fn read_stream(stream: *mut c_void) -> Option<Vec<u8>> {
    type Size = unsafe extern "win64" fn(*mut c_void, *mut u64) -> i32;
    type Read = unsafe extern "win64" fn(
        *mut c_void,
        *mut *const c_void,
        u64,
        u64,
        *mut *mut c_void,
    ) -> i32;
    type Free = unsafe extern "win64" fn(*mut c_void, *mut c_void);
    let mut len = 0u64;
    if method::<Size>(stream, 5)(stream, &mut len) < 0 || len == 0 || len > MAX_FONT_FILE {
        return None;
    }
    let mut start = std::ptr::null();
    let mut ctx = std::ptr::null_mut();
    if method::<Read>(stream, 3)(stream, &mut start, 0, len, &mut ctx) < 0 || start.is_null() {
        return None;
    }
    let data = std::slice::from_raw_parts(start as *const u8, len as usize).to_vec();
    method::<Free>(stream, 4)(stream, ctx);
    Some(data)
}

/// Dossier privé (0700) des polices servies par l'application.
fn cache_dir() -> Option<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let dir = std::env::temp_dir().join(format!("winlux-polices-appli-{}", std::process::id()));
    if !dir.is_dir() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .ok()?;
    }
    Some(dir)
}

/// Copie les octets d'une police dans le cache (une fois par contenu).
fn cache(data: &[u8]) -> Option<PathBuf> {
    let ext = match data.get(0..4) {
        Some(b"ttcf") => "ttc",
        Some(b"OTTO") => "otf",
        _ => "ttf",
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    let path = cache_dir()?.join(format!("{:016x}-{}.{ext}", h.finish(), data.len()));
    if std::fs::metadata(&path).map(|m| m.len()).ok() == Some(data.len() as u64) {
        return Some(path);
    }
    // Nom provisoire propre à chaque écriture : deux threads peuvent copier
    // la même police en même temps.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("part{n}"));
    std::fs::write(&tmp, data).ok()?;
    std::fs::rename(&tmp, &path).ok()?;
    Some(path)
}

/// Faces d'un fichier, comme le catalogue système (sans CFF).
fn faces_of(path: &std::path::Path) -> Vec<FaceInfo> {
    super::super::gdi32_fonts::parse_font_file(path)
        .iter()
        .filter(|f| !f.cff)
        .map(info)
        .collect()
}

/// IDWriteFactory::CreateCustomFontCollection(loader, key, keySize, out).
pub(super) unsafe extern "win64" fn custom_collection(
    this: *mut FactoryObj,
    loader: *mut c_void,
    key: *const c_void,
    size: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if loader.is_null() {
        return E_INVALIDARG;
    }
    type Enumerate = unsafe extern "win64" fn(
        *mut c_void,
        *mut c_void,
        *const c_void,
        u32,
        *mut *mut c_void,
    ) -> i32;
    type MoveNext = unsafe extern "win64" fn(*mut c_void, *mut i32) -> i32;
    type Current = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;
    let mut en = std::ptr::null_mut();
    let hr = method::<Enumerate>(loader, 3)(loader, this.cast(), key, size, &mut en);
    if hr < 0 {
        return hr;
    }
    if en.is_null() {
        return E_INVALIDARG;
    }
    let mut faces = Vec::new();
    for _ in 0..MAX_FILES {
        let mut more = 0i32;
        if method::<MoveNext>(en, 3)(en, &mut more) < 0 || more == 0 {
            break;
        }
        let mut file = std::ptr::null_mut();
        if method::<Current>(en, 4)(en, &mut file) < 0 || file.is_null() {
            continue;
        }
        match file_path(file) {
            Some(p) => faces.extend(faces_of(&p)),
            None => crate::log_warn!("dwrite", "police de l'appli illisible, ignorée"),
        }
        com_release(file);
    }
    com_release(en);
    crate::log_info!(
        "dwrite",
        "collection de l'appli : {} face(s) ({})",
        faces.len(),
        faces
            .iter()
            .map(|f| f.family.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    // Une police de l'application se trouve aussi par son nom (formats de
    // texte qui ne passent pas la collection).
    {
        let mut extra = crate::dlls::lock(super::extra());
        for f in &faces {
            if !extra.contains(f) {
                extra.push(f.clone());
            }
        }
    }
    put(out, Com::create(COLLECTION_VT.ptr(), group(faces)))
}

/// IDWriteFactory::CreateCustomFontFileReference(key, keySize, loader, out).
pub(super) unsafe extern "win64" fn custom_file_reference(
    _this: *mut FactoryObj,
    key: *const c_void,
    size: u32,
    loader: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if loader.is_null() {
        return E_INVALIDARG;
    }
    match loader_path(loader, key, size) {
        Some(p) => put(out, Com::create(FILE_VT.ptr(), p)),
        None => DWRITE_E_FILENOTFOUND,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlls::audio_com::{VTable, S_OK};
    use std::sync::atomic::{AtomicU32, Ordering};

    // Chargeur de fichiers de l'« application » : un flux qui sert la police
    // de test, et compte ses libérations.
    static RELEASED: AtomicU32 = AtomicU32::new(0);

    unsafe extern "win64" fn qi(_: *mut c_void, _: *const c_void, _: *mut *mut c_void) -> i32 {
        0x8000_4002u32 as i32
    }
    unsafe extern "win64" fn add_ref(_: *mut c_void) -> u32 {
        1
    }
    unsafe extern "win64" fn release(_: *mut c_void) -> u32 {
        RELEASED.fetch_add(1, Ordering::SeqCst);
        1
    }

    fn font() -> &'static [u8] {
        static F: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        F.get_or_init(|| {
            crate::dlls::gdi32_fonts::tests::sample_font(
                "Winlux Icones",
                "Regular",
                400,
                false,
                false,
            )
        })
    }

    unsafe extern "win64" fn stream_read(
        _: *mut c_void,
        start: *mut *const c_void,
        off: u64,
        len: u64,
        ctx: *mut *mut c_void,
    ) -> i32 {
        assert_eq!((off, len), (0, font().len() as u64));
        *start = font().as_ptr().cast();
        *ctx = std::ptr::null_mut();
        S_OK
    }
    unsafe extern "win64" fn stream_free(_: *mut c_void, _: *mut c_void) {}
    unsafe extern "win64" fn stream_size(_: *mut c_void, out: *mut u64) -> i32 {
        *out = font().len() as u64;
        S_OK
    }
    static STREAM_VT: VTable<7> = VTable([
        qi as *const (),
        add_ref as *const (),
        release as *const (),
        stream_read as *const (),
        stream_free as *const (),
        stream_size as *const (),
        stream_size as *const (),
    ]);
    static mut STREAM: *const () = std::ptr::null();

    unsafe extern "win64" fn loader_stream(
        _: *mut c_void,
        key: *const c_void,
        size: u32,
        out: *mut *mut c_void,
    ) -> i32 {
        let k = std::slice::from_raw_parts(key as *const u8, size as usize);
        assert_eq!(k, b"pack://application:,,,/Icons.ttf");
        STREAM = STREAM_VT.ptr();
        *out = std::ptr::addr_of_mut!(STREAM).cast();
        S_OK
    }
    static FILE_LOADER_VT: VTable<4> = VTable([
        qi as *const (),
        add_ref as *const (),
        release as *const (),
        loader_stream as *const (),
    ]);
    static mut FILE_LOADER: *const () = std::ptr::null();

    // Énumérateur : un seul fichier, créé par CreateCustomFontFileReference.
    static mut STEP: u32 = 0;
    unsafe extern "win64" fn enum_next(_: *mut c_void, more: *mut i32) -> i32 {
        STEP += 1;
        *more = (STEP == 1) as i32;
        S_OK
    }
    unsafe extern "win64" fn enum_current(_: *mut c_void, out: *mut *mut c_void) -> i32 {
        FILE_LOADER = FILE_LOADER_VT.ptr();
        let key = b"pack://application:,,,/Icons.ttf";
        custom_file_reference(
            std::ptr::null_mut(),
            key.as_ptr().cast(),
            key.len() as u32,
            std::ptr::addr_of_mut!(FILE_LOADER).cast(),
            out,
        )
    }
    static ENUM_VT: VTable<5> = VTable([
        qi as *const (),
        add_ref as *const (),
        release as *const (),
        enum_next as *const (),
        enum_current as *const (),
    ]);
    static mut ENUM: *const () = std::ptr::null();

    unsafe extern "win64" fn collection_enum(
        _: *mut c_void,
        _factory: *mut c_void,
        _key: *const c_void,
        _size: u32,
        out: *mut *mut c_void,
    ) -> i32 {
        ENUM = ENUM_VT.ptr();
        *out = std::ptr::addr_of_mut!(ENUM).cast();
        S_OK
    }
    static COLLECTION_LOADER_VT: VTable<4> = VTable([
        qi as *const (),
        add_ref as *const (),
        release as *const (),
        collection_enum as *const (),
    ]);

    #[test]
    fn app_font_collection_reads_the_app_stream() {
        unsafe {
            let mut loader = COLLECTION_LOADER_VT.ptr();
            let mut out = std::ptr::null_mut();
            let key = b"Icons";
            assert_eq!(
                custom_collection(
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(loader).cast(),
                    key.as_ptr().cast(),
                    key.len() as u32,
                    &mut out
                ),
                S_OK
            );
            assert!(!out.is_null());
            let fams = Com::get(out as *mut super::super::CollectionObj);
            assert_eq!(fams.len(), 1);
            assert_eq!(fams[0].name, "Winlux Icones");
            let path = &fams[0].faces[0].path;
            assert!(path.starts_with(cache_dir().unwrap()));
            assert_eq!(std::fs::read(path).unwrap(), font());
            // L'énumérateur et le flux de l'appli sont rendus.
            assert!(RELEASED.load(Ordering::SeqCst) >= 2);
            Com::release(out as *mut super::super::CollectionObj);
        }
    }

    #[test]
    fn same_bytes_share_one_cache_file() {
        let a = cache(font()).unwrap();
        let b = cache(font()).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.extension().unwrap(), "ttf");
    }
}
