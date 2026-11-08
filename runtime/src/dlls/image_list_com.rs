//! Listes d'images système du Shell : SHGetImageList, l'objet IImageList
//! qu'elle rend, et SHGetStockIconInfo.
//!
//! Sous Windows, HIMAGELIST et IImageList* sont interchangeables
//! (documentation Microsoft Learn de SHGetImageList et d'IImageList) : un
//! programme Delphi (WinSCP) range le pointeur rendu comme un HIMAGELIST
//! et appelle ImageList_*, Qt (VLC) appelle les méthodes de l'interface.
//! Le handle d'une liste système est donc l'adresse d'un objet dont la
//! table de méthodes renvoie vers les fonctions ImageList_* d'icons.rs.
//! Ordre des méthodes : commoncontrols.h de MinGW-w64 (domaine public).
//!
//! Écart connu : un programme x86 qui appelle les méthodes elles-mêmes
//! (et pas ImageList_*) tomberait sur des fonctions win64 ; aucun programme
//! relevé ne le fait (WinSCP x86 n'utilise que le handle).

#![allow(non_snake_case)]

use super::icons;
use std::ffi::c_void;

const S_OK: i32 = 0;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;

/// IID_IImageList {46EB5926-582E-4017-9FDF-E8998DAA0950}.
const IID_IIMAGELIST: [u8; 16] = [
    0x26, 0x59, 0xeb, 0x46, 0x2e, 0x58, 0x17, 0x40, 0x9f, 0xdf, 0xe8, 0x99, 0x8d, 0xaa, 0x09, 0x50,
];
/// IID_IUnknown {00000000-0000-0000-C000-000000000046}.
const IID_IUNKNOWN: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46];

#[repr(C)]
struct Obj {
    vtbl: *const [usize; 32],
}

unsafe impl Sync for Obj {}

/// Nouvel objet IImageList (jamais libéré : une liste système vit autant
/// que le processus). Son adresse sert de handle de liste.
pub fn new_object() -> usize {
    Box::into_raw(Box::new(Obj { vtbl: vtbl() })) as usize
}

/// Table des méthodes (adresses connues seulement à l'exécution).
fn vtbl() -> &'static [usize; 32] {
    static V: std::sync::OnceLock<[usize; 32]> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        [
            QueryInterface as *const () as usize,
            AddRef as *const () as usize,
            Release as *const () as usize,
            Add as *const () as usize,
            ReplaceIcon as *const () as usize,
            SetOverlayImage as *const () as usize,
            Replace as *const () as usize,
            AddMasked as *const () as usize,
            Draw as *const () as usize,
            Remove as *const () as usize,
            GetIcon as *const () as usize,
            GetImageInfo as *const () as usize,
            NotImpl4 as *const () as usize, // Copy
            NotImpl7 as *const () as usize, // Merge
            NotImpl2 as *const () as usize, // Clone
            GetImageRect as *const () as usize,
            GetIconSize as *const () as usize,
            SetIconSize as *const () as usize,
            GetImageCount as *const () as usize,
            SetImageCount as *const () as usize,
            SetBkColor as *const () as usize,
            GetBkColor as *const () as usize,
            NotImpl4 as *const () as usize, // BeginDrag
            Ok0 as *const () as usize,      // EndDrag
            NotImpl3 as *const () as usize, // DragEnter
            NotImpl1 as *const () as usize, // DragLeave
            NotImpl2 as *const () as usize, // DragMove
            NotImpl4 as *const () as usize, // SetDragCursorImage
            NotImpl1 as *const () as usize, // DragShowNolock
            NotImpl4 as *const () as usize, // GetDragImage
            GetItemFlags as *const () as usize,
            GetOverlayImage as *const () as usize,
        ]
    })
}

unsafe fn put<T>(p: *mut T, v: T) -> i32 {
    if p.is_null() {
        return E_POINTER;
    }
    p.write_unaligned(v);
    S_OK
}

fn ok(b: bool) -> i32 {
    if b {
        S_OK
    } else {
        E_FAIL
    }
}

unsafe extern "win64" fn QueryInterface(this: usize, iid: *const [u8; 16], out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if !iid.is_null() && (*iid == IID_IIMAGELIST || *iid == IID_IUNKNOWN) {
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}

extern "win64" fn AddRef(_this: usize) -> u32 {
    2
}

extern "win64" fn Release(_this: usize) -> u32 {
    1
}

unsafe extern "win64" fn Add(this: usize, bmp: usize, mask: usize, out: *mut i32) -> i32 {
    let i = icons::image_list_add(this, bmp, mask);
    if !out.is_null() {
        *out = i;
    }
    ok(i >= 0)
}

unsafe extern "win64" fn ReplaceIcon(this: usize, i: i32, icon: usize, out: *mut i32) -> i32 {
    let r = icons::image_list_replace_icon(this, i, icon);
    if !out.is_null() {
        *out = r;
    }
    ok(r >= 0)
}

extern "win64" fn SetOverlayImage(_this: usize, _image: i32, _overlay: i32) -> i32 {
    S_OK
}

extern "win64" fn Replace(this: usize, i: i32, bmp: usize, mask: usize) -> i32 {
    ok(icons::image_list_replace(this, i, bmp, mask) != 0)
}

unsafe extern "win64" fn AddMasked(this: usize, bmp: usize, key: u32, out: *mut i32) -> i32 {
    let i = icons::image_list_add_masked(this, bmp, key);
    if !out.is_null() {
        *out = i;
    }
    ok(i >= 0)
}

/// Draw(IMAGELISTDRAWPARAMS*) : la structure porte sa propre liste.
unsafe extern "win64" fn Draw(_this: usize, params: *const u8) -> i32 {
    ok(icons::image_list_draw_indirect(params) != 0)
}

extern "win64" fn Remove(this: usize, i: i32) -> i32 {
    ok(icons::image_list_remove(this, i) != 0)
}

unsafe extern "win64" fn GetIcon(this: usize, i: i32, flags: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let h = icons::image_list_get_icon(this, i, flags);
    *out = h;
    ok(h != 0)
}

unsafe extern "win64" fn GetImageInfo(this: usize, i: i32, info: *mut u8) -> i32 {
    ok(icons::image_list_get_image_info(this, i, info) != 0)
}

unsafe extern "win64" fn GetImageRect(this: usize, i: i32, rect: *mut [i32; 4]) -> i32 {
    let count = icons::image_list_get_image_count(this);
    match icons::list_size(this) {
        Some((cx, cy)) if i >= 0 && i < count => put(rect, [0, 0, cx, cy]),
        _ => E_INVALIDARG,
    }
}

unsafe extern "win64" fn GetIconSize(this: usize, cx: *mut i32, cy: *mut i32) -> i32 {
    let Some((w, h)) = icons::list_size(this) else {
        return E_FAIL;
    };
    if cx.is_null() || cy.is_null() {
        return E_POINTER;
    }
    *cx = w;
    *cy = h;
    S_OK
}

extern "win64" fn SetIconSize(this: usize, cx: i32, cy: i32) -> i32 {
    ok(icons::image_list_set_icon_size(this, cx, cy) != 0)
}

unsafe extern "win64" fn GetImageCount(this: usize, out: *mut i32) -> i32 {
    put(out, icons::image_list_get_image_count(this))
}

extern "win64" fn SetImageCount(this: usize, count: u32) -> i32 {
    ok(icons::image_list_set_image_count(this, count) != 0)
}

unsafe extern "win64" fn SetBkColor(this: usize, color: u32, old: *mut u32) -> i32 {
    let prev = icons::image_list_set_bk_color(this, color);
    if !old.is_null() {
        *old = prev;
    }
    S_OK
}

unsafe extern "win64" fn GetBkColor(this: usize, out: *mut u32) -> i32 {
    put(out, icons::image_list_get_bk_color(this))
}

unsafe extern "win64" fn GetItemFlags(_this: usize, _i: i32, out: *mut u32) -> i32 {
    put(out, 0)
}

/// Pas d'images de superposition dans les listes de Winlux.
unsafe extern "win64" fn GetOverlayImage(_this: usize, _overlay: i32, out: *mut i32) -> i32 {
    put(out, -1);
    E_FAIL
}

extern "win64" fn Ok0(_this: usize) -> i32 {
    S_OK
}

extern "win64" fn NotImpl1(_this: usize) -> i32 {
    E_NOTIMPL
}

extern "win64" fn NotImpl2(_this: usize, _a: usize, _b: usize) -> i32 {
    E_NOTIMPL
}

extern "win64" fn NotImpl3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    E_NOTIMPL
}

extern "win64" fn NotImpl4(_this: usize, _a: usize, _b: usize, _c: usize, _d: usize) -> i32 {
    E_NOTIMPL
}

#[allow(clippy::too_many_arguments)]
extern "win64" fn NotImpl7(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
) -> i32 {
    E_NOTIMPL
}

// ── Fonctions de shell32 ────────────────────────────────────────────

const SHIL_LARGE: i32 = 0;
const SHIL_SMALL: i32 = 1;
const SHIL_EXTRALARGE: i32 = 2;
const SHIL_SYSSMALL: i32 = 3;
const SHIL_JUMBO: i32 = 4;

/// SHGetImageList : liste système de la taille demandée (petites 16,
/// grandes 32, très grandes 48, géantes 256 pixels).
unsafe extern "win64" fn SHGetImageList(
    kind: i32,
    iid: *const [u8; 16],
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let px = match kind {
        SHIL_LARGE => 32,
        SHIL_SMALL | SHIL_SYSSMALL => 16,
        SHIL_EXTRALARGE => 48,
        SHIL_JUMBO => 256,
        _ => return E_INVALIDARG,
    };
    let list = icons::system_list_px(px);
    let mut obj = 0usize;
    let hr = QueryInterface(list, iid, &mut obj);
    *out = obj as *mut c_void;
    hr
}

const SHGSI_ICON: u32 = 0x100;
const SHGSI_SYSICONINDEX: u32 = 0x4000;
const SHGSI_SMALLICON: u32 = 0x1;
const SHGSI_SHELLICONSIZE: u32 = 0x4;

/// Icône de la liste système pour une icône standard (SHSTOCKICONID).
fn stock_index(id: i32) -> i32 {
    match id {
        3 | 4 => 0,  // SIID_FOLDER, SIID_FOLDEROPEN
        2 => 4,      // SIID_APPLICATION
        6..=11 => 2, // lecteurs : disquette, amovible, fixe, réseau…
        105 => 3,    // SIID_ZIPFILE
        72 => 5,     // SIID_IMAGEFILES
        1 => 6,      // SIID_DOCASSOC
        _ => 1,      // SIID_DOCNOASSOC et le reste : fichier
    }
}

/// SHGetStockIconInfo : SHSTOCKICONINFO de 536 octets en x86 (cbSize,
/// hIcon, iSysImageIndex, iIcon, szPath[260]) et de 544 en 64 bits.
unsafe extern "win64" fn SHGetStockIconInfo(id: i32, flags: u32, info: *mut u8) -> i32 {
    if info.is_null() {
        return E_INVALIDARG;
    }
    let ptr = crate::wow32::pointer_size();
    let size = (info as *const u32).read_unaligned() as usize;
    let (icon_at, index_at, path_at) = if ptr == 4 { (4, 8, 16) } else { (8, 16, 24) };
    if size < path_at + 520 || !(0..=200).contains(&id) {
        return E_INVALIDARG;
    }
    let index = stock_index(id);
    crate::wow32::write_ptr(info as usize + icon_at, 0);
    (info.add(index_at) as *mut i32).write_unaligned(-1);
    (info.add(index_at + 4) as *mut i32).write_unaligned(0);
    std::ptr::write_bytes(info.add(path_at), 0, 520);
    if flags & SHGSI_SYSICONINDEX != 0 {
        (info.add(index_at) as *mut i32).write_unaligned(index);
    }
    if flags & SHGSI_ICON != 0 {
        let small = flags & SHGSI_SMALLICON != 0 && flags & SHGSI_SHELLICONSIZE == 0;
        let icon = icons::list_image(icons::system_list(small), index).map_or(0, icons::new_icon);
        crate::wow32::write_ptr(info as usize + icon_at, icon);
    }
    S_OK
}

pub fn shell32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SHGetImageList" => fn SHGetImageList,
        "#727" => fn SHGetImageList,
        "SHGetStockIconInfo" => fn SHGetStockIconInfo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_systeme_et_iimagelist() {
        unsafe {
            let mut p: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                SHGetImageList(SHIL_EXTRALARGE, &IID_IIMAGELIST, &mut p),
                S_OK
            );
            let h = p as usize;
            assert_ne!(h, 0);
            // Même objet à chaque appel, utilisable comme HIMAGELIST.
            let mut again: *mut c_void = std::ptr::null_mut();
            SHGetImageList(SHIL_EXTRALARGE, &IID_IIMAGELIST, &mut again);
            assert_eq!(again, p);
            assert_eq!(icons::list_size(h), Some((48, 48)));
            assert!(icons::image_list_get_image_count(h) >= 7);
            // Appels par la table de méthodes, comme Qt.
            let vt = &*(*(h as *const *const [usize; 32]));
            let get_size: extern "win64" fn(usize, *mut i32, *mut i32) -> i32 =
                std::mem::transmute(vt[16]);
            let (mut cx, mut cy) = (0, 0);
            assert_eq!(get_size(h, &mut cx, &mut cy), S_OK);
            assert_eq!((cx, cy), (48, 48));
            let get_count: extern "win64" fn(usize, *mut i32) -> i32 = std::mem::transmute(vt[18]);
            let mut n = 0;
            assert_eq!(get_count(h, &mut n), S_OK);
            assert!(n >= 7);
            let get_icon: extern "win64" fn(usize, i32, u32, *mut usize) -> i32 =
                std::mem::transmute(vt[10]);
            let mut icon = 0usize;
            assert_eq!(get_icon(h, 0, 0, &mut icon), S_OK);
            assert_ne!(icon, 0);
            // Interface inconnue refusée, taille inconnue refusée.
            let other = [1u8; 16];
            assert_eq!(SHGetImageList(SHIL_SMALL, &other, &mut p), E_NOINTERFACE);
            assert!(p.is_null());
            assert_eq!(SHGetImageList(9, &IID_IIMAGELIST, &mut p), E_INVALIDARG);
            assert_eq!(SHGetImageList(SHIL_SMALL, &IID_IUNKNOWN, &mut p), S_OK);
            assert_eq!(icons::list_size(p as usize), Some((16, 16)));
        }
    }

    #[test]
    fn icone_standard() {
        unsafe {
            let mut info = [0u8; 544];
            info[..4].copy_from_slice(&544u32.to_le_bytes());
            assert_eq!(
                SHGetStockIconInfo(3, SHGSI_SYSICONINDEX | SHGSI_ICON, info.as_mut_ptr()),
                S_OK
            );
            assert_eq!(i32::from_le_bytes(info[16..20].try_into().unwrap()), 0);
            assert_ne!(usize::from_le_bytes(info[8..16].try_into().unwrap()), 0);
            info[..4].copy_from_slice(&10u32.to_le_bytes());
            assert_eq!(SHGetStockIconInfo(3, 0, info.as_mut_ptr()), E_INVALIDARG);
        }
    }
}
