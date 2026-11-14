//! Classe Windows Runtime `Windows.UI.ViewManagement.UISettings` (#2206) :
//! `new UISettings()` passe par IActivationFactory::ActivateInstance et rend
//! un objet IUISettings (… IUISettings6) : couleurs du système et de
//! l'accent, échelle du texte, effets, durées. Teams l'active au démarrage
//! et s'arrête sur son REGDB_E_CLASSNOTREG.
//!
//! Sources : documentation publique de Microsoft (UISettings, UIColorType
//! Background = 0 … Complement = 9, UIElementType ActiveCaption = 0 …
//! WindowText = 12 et AccentColor = 1000 … OverlayOutsidePopup = 1012,
//! Windows.UI.Color de quatre octets A, R, G, B, HandPreference) ;
//! identifiants d'interface et ordre des méthodes tels que les publie
//! windows-rs (licence MIT).
//!
//! Valeurs : thème clair et accent Prisme #6A3DF0, comme
//! ShouldAppsUseDarkMode (uxtheme) et DwmGetColorizationColor. Les nuances
//! AccentDark1..3 et AccentLight1..3 mélangent l'accent avec le noir ou le
//! blanc à 20, 40 et 60 % : Windows ne documente pas son calcul, c'est notre
//! choix. Les couleurs UIElementType 0..12 sont celles de GetSysColor. Les
//! événements …Changed rendent un jeton sans jamais appeler le gestionnaire :
//! ces réglages ne changent pas pendant la vie du processus.

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};
use super::weak_ref::{
    get_weak_reference as get_weak_reference_of, WeakControl, IID_IWEAK_REFERENCE_SOURCE,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
const IID_IACTIVATION_FACTORY: Guid = g(0x0000_0035, 0x0000, 0x0000, 0xC000_0000_0000_0046);
const IID_UI_SETTINGS: [Guid; 6] = [
    g(0x8536_1600, 0x1c63, 0x4627, 0xbcb1_3a89_e0bc_9c55),
    g(0xbad8_2401, 0x2721, 0x44f9, 0xbb91_2bb2_28be_442f),
    g(0x0302_1be4, 0x5254, 0x4781, 0x8194_5168_f7d0_6d7b),
    g(0x52bb_3002, 0x919b, 0x4d6b, 0x9b78_8dd6_6ff4_b93b),
    g(0x5349_d588, 0x0cb5, 0x5f05, 0xbd34_706b_3231_f0bd),
    g(0xaef1_9bd7, 0xfe31, 0x5a04, 0xada4_469a_aec6_dfa9),
];

const CLASS: &str = "Windows.UI.ViewManagement.UISettings";

/// Accent Prisme, en 0xRRGGBB.
const ACCENT: u32 = 0x6A_3D_F0;

unsafe fn hstring(text: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}
unsafe extern "win64" fn class_name(_this: usize, out: *mut usize) -> i32 {
    hstring(CLASS, out)
}

// ============================================================
//  Couleurs
// ============================================================

/// Windows.UI.Color : A, R, G, B.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    a: u8,
    r: u8,
    g: u8,
    b: u8,
}

fn rgb(c: u32) -> Color {
    Color {
        a: 255,
        r: (c >> 16) as u8,
        g: (c >> 8) as u8,
        b: c as u8,
    }
}

/// Mélange l'accent avec `toward` (0 noir, 255 blanc) à `percent` %.
fn shade(toward: u8, percent: u32) -> Color {
    let mix = |v: u32| ((v * (100 - percent) + toward as u32 * percent + 50) / 100) as u8;
    let a = rgb(ACCENT);
    Color {
        a: 255,
        r: mix(a.r as u32),
        g: mix(a.g as u32),
        b: mix(a.b as u32),
    }
}

/// UIColorType : None pour une valeur hors de l'énumération.
fn color_value(kind: i32) -> Option<Color> {
    Some(match kind {
        0 => rgb(0xFF_FF_FF),           // Background (thème clair)
        1 => rgb(0x00_00_00),           // Foreground
        2 => shade(0, 60),              // AccentDark3
        3 => shade(0, 40),              // AccentDark2
        4 => shade(0, 20),              // AccentDark1
        5 => rgb(ACCENT),               // Accent
        6 => shade(255, 20),            // AccentLight1
        7 => shade(255, 40),            // AccentLight2
        8 => shade(255, 60),            // AccentLight3
        9 => rgb(!ACCENT & 0xFF_FF_FF), // Complement (« non pris en charge » selon la doc)
        _ => return None,
    })
}

/// GetSysColor rend 0x00BBGGRR.
fn sys_color(index: i32) -> Color {
    let c = super::user32::get_sys_color(index);
    rgb(((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF))
}

/// UIElementType : None pour une valeur hors de l'énumération.
fn element_color(kind: i32) -> Option<Color> {
    // UIElementType 0..12 → index COLOR_* de GetSysColor.
    const SYS: [i32; 13] = [2, 1, 15, 18, 9, 17, 13, 14, 26, 3, 19, 5, 8];
    if let Some(&i) = SYS.get(kind as usize).filter(|_| kind >= 0) {
        return Some(sys_color(i));
    }
    Some(match kind {
        1000 => rgb(ACCENT),     // AccentColor
        1001 => rgb(0x00_00_00), // TextHigh
        1002 => rgb(0x5C_5C_5C), // TextMedium
        1003 => rgb(0x8A_8A_8A), // TextLow
        1004 => rgb(0xFF_FF_FF), // TextContrastWithHigh
        1005 => rgb(0x00_00_00), // NonTextHigh
        1006 => rgb(0x33_33_33), // NonTextMediumHigh
        1007 => rgb(0x8A_8A_8A), // NonTextMedium
        1008 => rgb(0xC2_C2_C2), // NonTextMediumLow
        1009 => rgb(0xE5_E5_E5), // NonTextLow
        1010 => rgb(0xF3_F3_F3), // PageBackground
        1011 => rgb(0xFF_FF_FF), // PopupBackground
        1012 => Color {
            a: 0x66,
            r: 0,
            g: 0,
            b: 0,
        }, // OverlayOutsidePopup
        _ => return None,
    })
}

// ============================================================
//  Objet UISettings : six tables, une par interface
// ============================================================

#[repr(C)]
struct Settings {
    /// IUISettings…IUISettings6 puis IWeakReferenceSource (#2207).
    vt: [&'static VTable<19>; 7],
    refs: AtomicU32,
    /// Cible des références faibles, créée au premier GetWeakReference.
    weak: std::sync::OnceLock<std::sync::Arc<WeakControl>>,
}

/// Jetons d'abonnement aux événements (jamais déclenchés).
static NEXT_TOKEN: AtomicI64 = AtomicI64::new(1);

fn new_settings() -> usize {
    Box::into_raw(Box::new(Settings {
        vt: [&VT1, &VT2, &VT3, &VT4, &VT5, &VT6, &VT_WEAK],
        refs: AtomicU32::new(1),
        weak: std::sync::OnceLock::new(),
    })) as usize
}

/// Adresse de l'objet depuis l'une de ses sept interfaces.
unsafe fn base_of(this: usize) -> usize {
    let vt = *(this as *const *const VTable<19>);
    let n = [&VT1, &VT2, &VT3, &VT4, &VT5, &VT6, &VT_WEAK]
        .iter()
        .position(|t| std::ptr::eq(*t, vt))
        .unwrap_or(0);
    this - 8 * n
}
unsafe fn settings_of(this: usize) -> &'static Settings {
    &*(base_of(this) as *const Settings)
}

unsafe extern "win64" fn s_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let base = base_of(this);
    *out = if let Some(n) = IID_UI_SETTINGS.iter().position(|g| *g == iid) {
        base + 8 * n
    } else if iid == IID_IWEAK_REFERENCE_SOURCE {
        base + 8 * 6
    } else if [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT].contains(&iid) {
        base
    } else {
        0
    };
    if *out == 0 {
        return E_NOINTERFACE;
    }
    settings_of(this).refs.fetch_add(1, Ordering::SeqCst);
    S_OK
}
unsafe extern "win64" fn s_add_ref(this: usize) -> u32 {
    settings_of(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn s_release(this: usize) -> u32 {
    let base = base_of(this);
    let n = settings_of(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let s = &*(base as *const Settings);
        if s.weak.get().is_some_and(|w| !w.release_last(&s.refs)) {
            // Repris par IWeakReference::Resolve entre-temps.
            return 0;
        }
        drop(Box::from_raw(base as *mut Settings));
    }
    n
}

/// IWeakReferenceSource::GetWeakReference (#2207).
unsafe extern "win64" fn get_weak_reference(this: usize, out: *mut usize) -> i32 {
    let base = base_of(this);
    let s = &*(base as *const Settings);
    let control = s
        .weak
        .get_or_init(|| WeakControl::new(base, s_qi, &s.refs as *const AtomicU32));
    get_weak_reference_of(control, out)
}

unsafe fn put<T>(out: *mut T, v: T) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    out.write(v);
    S_OK
}

// IUISettings
unsafe extern "win64" fn hand_preference(_this: usize, out: *mut i32) -> i32 {
    put(out, 1) // HandPreference.RightHanded
}
/// Windows.Foundation.Size : deux f32.
unsafe extern "win64" fn cursor_size(_this: usize, out: *mut [f32; 2]) -> i32 {
    put(out, [32.0, 32.0])
}
unsafe extern "win64" fn scroll_bar_size(_this: usize, out: *mut [f32; 2]) -> i32 {
    put(out, [17.0, 17.0])
}
unsafe extern "win64" fn message_duration(_this: usize, out: *mut u32) -> i32 {
    put(out, 5)
}
unsafe extern "win64" fn yes(_this: usize, out: *mut u8) -> i32 {
    put(out, 1)
}
unsafe extern "win64" fn no(_this: usize, out: *mut u8) -> i32 {
    put(out, 0)
}
unsafe extern "win64" fn caret_blink_rate(_this: usize, out: *mut u32) -> i32 {
    put(out, super::user32::get_caret_blink_time())
}
unsafe extern "win64" fn caret_width(_this: usize, out: *mut u32) -> i32 {
    put(out, 1)
}
unsafe extern "win64" fn double_click_time(_this: usize, out: *mut u32) -> i32 {
    put(out, super::user32::get_double_click_time())
}
unsafe extern "win64" fn mouse_hover_time(_this: usize, out: *mut u32) -> i32 {
    put(out, 400)
}
unsafe extern "win64" fn ui_element_color(_this: usize, kind: i32, out: *mut Color) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match element_color(kind) {
        Some(c) => put(out, c),
        None => E_INVALIDARG,
    }
}
// IUISettings2
unsafe extern "win64" fn text_scale_factor(_this: usize, out: *mut f64) -> i32 {
    put(out, 1.0)
}
// IUISettings3
unsafe extern "win64" fn get_color_value(_this: usize, kind: i32, out: *mut Color) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match color_value(kind) {
        Some(c) => put(out, c),
        None => E_INVALIDARG,
    }
}
// Événements communs à IUISettings2..6.
unsafe extern "win64" fn add_event(_this: usize, handler: usize, token: *mut i64) -> i32 {
    if handler == 0 {
        return E_INVALIDARG;
    }
    put(token, NEXT_TOKEN.fetch_add(1, Ordering::SeqCst))
}
extern "win64" fn remove_event(_this: usize, _token: i64) -> i32 {
    S_OK
}

const HEAD: [*const (); 6] = [
    s_qi as *const (),
    s_add_ref as *const (),
    s_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
];

const fn table(methods: [*const (); 13]) -> VTable<19> {
    let mut t = [std::ptr::null(); 19];
    let mut i = 0;
    while i < 6 {
        t[i] = HEAD[i];
        i += 1;
    }
    while i < 19 {
        t[i] = methods[i - 6];
        i += 1;
    }
    VTable(t)
}

const NUL: *const () = std::ptr::null();
const ADD: *const () = add_event as *const ();
const REM: *const () = remove_event as *const ();

static VT1: VTable<19> = table([
    hand_preference as *const (),
    cursor_size as *const (),
    scroll_bar_size as *const (), // ScrollBarSize
    scroll_bar_size as *const (), // ScrollBarArrowSize
    scroll_bar_size as *const (), // ScrollBarThumbBoxSize
    message_duration as *const (),
    yes as *const (), // AnimationsEnabled
    no as *const (),  // CaretBrowsingEnabled
    caret_blink_rate as *const (),
    caret_width as *const (),
    double_click_time as *const (),
    mouse_hover_time as *const (),
    ui_element_color as *const (),
]);
static VT2: VTable<19> = table([
    text_scale_factor as *const (),
    ADD,
    REM,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
]);
static VT3: VTable<19> = table([
    get_color_value as *const (),
    ADD,
    REM,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
]);
static VT4: VTable<19> = table([
    yes as *const (), // AdvancedEffectsEnabled (EnableTransparency = 1)
    ADD,
    REM,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
]);
static VT5: VTable<19> = table([
    yes as *const (), // AutoHideScrollBars
    ADD,
    REM,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
]);
static VT6: VTable<19> = table([
    ADD, // AnimationsEnabledChanged
    REM, ADD, // MessageDurationChanged
    REM, NUL, NUL, NUL, NUL, NUL, NUL, NUL, NUL, NUL,
]);

/// IWeakReferenceSource : IUnknown puis GetWeakReference.
static VT_WEAK: VTable<19> = VTable([
    s_qi as *const (),
    s_add_ref as *const (),
    s_release as *const (),
    get_weak_reference as *const (),
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
    NUL,
]);

// ============================================================
//  Fabrique : IActivationFactory
// ============================================================

#[repr(C)]
struct Factory {
    vtbl: &'static VTable<7>,
}

static FACTORY: Factory = Factory {
    vtbl: &FACTORY_VTBL,
};

unsafe extern "win64" fn f_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_IACTIVATION_FACTORY,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = std::ptr::addr_of!(FACTORY) as usize;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
/// Fabrique statique : jamais libérée.
extern "win64" fn f_add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn f_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn activate_instance(_this: usize, out: *mut usize) -> i32 {
    put(out, new_settings())
}

static FACTORY_VTBL: VTable<7> = VTable([
    f_qi as *const (),
    f_add_ref as *const (),
    f_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    activate_instance as *const (),
]);

/// `RoGetActivationFactory` pour UISettings, `None` pour une autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(f_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn slot(obj: usize, n: usize) -> *const () {
        (*(obj as *const &VTable<19>)).0[n]
    }

    /// Teams (#2206) : new UISettings(), puis les interfaces qu'il interroge.
    #[test]
    fn activation_et_couleurs_2206() {
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_IACTIVATION_FACTORY.as_ptr(), &mut f),
                Some(S_OK)
            );
            let mut other = 0usize;
            assert!(
                activation_factory("Windows.Other", IID_IUNKNOWN.as_ptr(), &mut other).is_none()
            );
            let mut obj = 0usize;
            assert_eq!(activate_instance(f, &mut obj), S_OK);

            // Chaque interface IUISettingsN est à 8·N octets de l'objet.
            for (n, iid) in IID_UI_SETTINGS.iter().enumerate() {
                let mut p = 0usize;
                assert_eq!(s_qi(obj, iid.as_ptr(), &mut p), S_OK);
                assert_eq!(p, obj + 8 * n);
                assert_eq!(base_of(p), obj);
                assert_eq!(s_release(p), 1);
            }

            // IUISettings3::GetColorValue.
            let mut s3 = 0usize;
            assert_eq!(s_qi(obj, IID_UI_SETTINGS[2].as_ptr(), &mut s3), S_OK);
            let gcv: unsafe extern "win64" fn(usize, i32, *mut Color) -> i32 =
                std::mem::transmute(slot(s3, 6));
            let mut c = Color {
                a: 0,
                r: 0,
                g: 0,
                b: 0,
            };
            assert_eq!(gcv(s3, 5, &mut c), S_OK);
            assert_eq!(
                c,
                Color {
                    a: 255,
                    r: 0x6A,
                    g: 0x3D,
                    b: 0xF0
                }
            );
            assert_eq!(gcv(s3, 0, &mut c), S_OK);
            assert_eq!(
                c,
                Color {
                    a: 255,
                    r: 255,
                    g: 255,
                    b: 255
                }
            );
            assert_eq!(gcv(s3, 2, &mut c), S_OK);
            assert!(c.r < 0x6A && c.b < 0xF0);
            assert_eq!(gcv(s3, 8, &mut c), S_OK);
            assert!(c.r > 0x6A && c.g > 0x3D);
            assert_eq!(gcv(s3, 10, &mut c), E_INVALIDARG);
            assert_eq!(gcv(s3, 5, std::ptr::null_mut()), E_POINTER);

            // Événement : un jeton, puis retrait.
            let add: unsafe extern "win64" fn(usize, usize, *mut i64) -> i32 =
                std::mem::transmute(slot(s3, 7));
            let mut token = 0i64;
            assert_eq!(add(s3, 0x1000, &mut token), S_OK);
            assert_ne!(token, 0);
            assert_eq!(add(s3, 0, &mut token), E_INVALIDARG);
            let rem: extern "win64" fn(usize, i64) -> i32 = std::mem::transmute(slot(s3, 8));
            assert_eq!(rem(s3, token), S_OK);
            assert_eq!(s_release(s3), 1);

            // IUISettings::UIElementColor : couleurs système et accent.
            let mut e = Color {
                a: 0,
                r: 0,
                g: 0,
                b: 0,
            };
            assert_eq!(ui_element_color(obj, 1000, &mut e), S_OK);
            assert_eq!(e, rgb(ACCENT));
            assert_eq!(ui_element_color(obj, 11, &mut e), S_OK);
            assert_eq!(e, sys_color(5));
            assert_eq!(ui_element_color(obj, 13, &mut e), E_INVALIDARG);

            // Référence faible (#2207) : QI, GetWeakReference, Resolve.
            let mut ws = 0usize;
            assert_eq!(
                s_qi(obj, IID_IWEAK_REFERENCE_SOURCE.as_ptr(), &mut ws),
                S_OK
            );
            assert_eq!(ws, obj + 48);
            let gwr: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
                std::mem::transmute(slot(ws, 3));
            let mut weak = 0usize;
            assert_eq!(gwr(ws, &mut weak), S_OK);
            assert_eq!(s_release(ws), 1);
            let weak_slot = |n: usize| (*(weak as *const &VTable<4>)).0[n];
            let resolve: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
                std::mem::transmute(weak_slot(3));
            let mut strong = 0usize;
            assert_eq!(
                resolve(weak, IID_UI_SETTINGS[2].as_ptr(), &mut strong),
                S_OK
            );
            assert_eq!(strong, obj + 16);
            assert_eq!(s_release(strong), 1);

            let mut scale = 0f64;
            assert_eq!(text_scale_factor(obj + 8, &mut scale), S_OK);
            assert_eq!(scale, 1.0);
            assert_eq!(s_release(obj), 0);
            // Objet détruit : Resolve rend S_OK et un pointeur nul.
            strong = 1;
            assert_eq!(
                resolve(weak, IID_UI_SETTINGS[0].as_ptr(), &mut strong),
                S_OK
            );
            assert_eq!(strong, 0);
            let release: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(weak_slot(2));
            assert_eq!(release(weak), 0);
        }
    }
}
