//! Classe Windows Runtime `Windows.System.UserProfile.GlobalizationPreferences`
//! (#2172) : propriétés statiques Calendars, Clocks, Currencies, Languages,
//! HomeGeographicRegion et WeekStartsOn, tirées de la locale de la session
//! (la même que GetUserDefaultLocaleName). Teams lève
//! `winrt::hresult_class_not_registered` sans elle et s'arrête avant sa
//! fenêtre ; Spotify la lit à l'installation.
//!
//! Sources : documentation publique de Microsoft (GlobalizationPreferences,
//! CalendarIdentifiers.Gregorian = « GregorianCalendar »,
//! ClockIdentifiers « 12HourClock » / « 24HourClock », DayOfWeek
//! Sunday = 0 … Saturday = 6, IVectorView, IIterable, IIterator) ;
//! identifiants d'interface et ordre des méthodes tels que les publie
//! windows-rs (licence MIT). Les IID des interfaces paramétrées
//! (IVectorView<String>, IIterable<String>, IIterator<String>) suivent
//! l'algorithme public des pinterface (UUID v5) et valent les valeurs
//! publiées.

use std::sync::atomic::{AtomicU32, Ordering};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_BOUNDS: i32 = 0x8000_000Bu32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
const IID_STATICS: Guid = g(0x01BF_4326, 0xED37, 0x4E96, 0xB0E9_C134_0D1E_A158);
const IID_VECTOR_VIEW_STRING: Guid = g(0x2f13_c006, 0xa03a, 0x5f69, 0xb090_75a4_3e33_423e);
const IID_ITERABLE_STRING: Guid = g(0xe2fc_c7c1, 0x3bfc, 0x5a0b, 0xb2b0_72e7_69d1_cb7e);
const IID_ITERATOR_STRING: Guid = g(0x8c30_4ebb, 0x6615, 0x50a4, 0x8829_879e_cd44_3236);

const CLASS: &str = "Windows.System.UserProfile.GlobalizationPreferences";
const VIEW_CLASS: &str = "Windows.Foundation.Collections.IVectorView`1<String>";
const ITER_CLASS: &str = "Windows.Foundation.Collections.IIterator`1<String>";

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

// ============================================================
//  Valeurs
// ============================================================

/// Préférences de la session : (langues, région, devise, horloge, premier jour).
struct Prefs {
    language: String,
    region: String,
    currency: String,
    clock: &'static str,
    /// Windows.Globalization.DayOfWeek : dimanche = 0 … samedi = 6.
    week_starts_on: i32,
}

fn prefs() -> Prefs {
    let (name, region, currency, clock24, first_day) = super::nls::user_preferences();
    Prefs {
        language: name.to_string(),
        region: region.to_string(),
        currency: currency.to_string(),
        clock: if clock24 {
            "24HourClock"
        } else {
            "12HourClock"
        },
        // Table de la locale : 0 = lundi … 6 = dimanche.
        week_starts_on: ((first_day + 1) % 7) as i32,
    }
}

// ============================================================
//  IVectorView<String> et IIterable<String>
// ============================================================

/// Vue de chaînes en lecture seule : IVectorView à l'adresse de l'objet,
/// IIterable huit octets plus loin.
#[repr(C)]
struct View {
    vt_view: &'static VTable<10>,
    vt_iterable: &'static VTable<7>,
    refs: AtomicU32,
    items: Vec<String>,
}

unsafe fn view_of(base: usize) -> &'static View {
    &*(base as *const View)
}

fn new_view(items: Vec<String>) -> usize {
    Box::into_raw(Box::new(View {
        vt_view: &VIEW_VTBL,
        vt_iterable: &ITERABLE_VTBL,
        refs: AtomicU32::new(1),
        items,
    })) as usize
}

unsafe fn view_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if iid == IID_ITERABLE_STRING {
        base + 8
    } else if [
        IID_VECTOR_VIEW_STRING,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else {
        0
    };
    if *out == 0 {
        return E_NOINTERFACE;
    }
    view_of(base).refs.fetch_add(1, Ordering::SeqCst);
    S_OK
}

unsafe fn view_release(base: usize) -> u32 {
    let n = view_of(base).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut View));
    }
    n
}

unsafe extern "win64" fn v_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    view_qi(this, iid, out)
}
unsafe extern "win64" fn v_add_ref(this: usize) -> u32 {
    view_of(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn v_release(this: usize) -> u32 {
    view_release(this)
}
unsafe extern "win64" fn it_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    view_qi(this - 8, iid, out)
}
unsafe extern "win64" fn it_add_ref(this: usize) -> u32 {
    view_of(this - 8).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn it_release(this: usize) -> u32 {
    view_release(this - 8)
}
unsafe extern "win64" fn view_class_name(_this: usize, out: *mut usize) -> i32 {
    hstring(VIEW_CLASS, out)
}

unsafe extern "win64" fn v_get_at(this: usize, index: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match view_of(this).items.get(index as usize) {
        Some(s) => hstring(s, out),
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn v_size(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = view_of(this).items.len() as u32;
    S_OK
}
unsafe extern "win64" fn v_index_of(
    this: usize,
    value: usize,
    index: *mut u32,
    found: *mut u8,
) -> i32 {
    if index.is_null() || found.is_null() {
        return E_POINTER;
    }
    let wanted = String::from_utf16_lossy(super::electron_win32::hstring_slice(value));
    match view_of(this).items.iter().position(|s| *s == wanted) {
        Some(i) => {
            *index = i as u32;
            *found = 1;
        }
        None => {
            *index = 0;
            *found = 0;
        }
    }
    S_OK
}
unsafe fn copy_many(
    items: &[String],
    start: usize,
    cap: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (cap > 0 && out.is_null()) {
        return E_POINTER;
    }
    let mut n = 0u32;
    for s in items.iter().skip(start).take(cap as usize) {
        let hr = hstring(s, out.add(n as usize));
        if hr != S_OK {
            return hr;
        }
        n += 1;
    }
    *actual = n;
    S_OK
}
unsafe extern "win64" fn v_get_many(
    this: usize,
    start: u32,
    cap: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    let items = &view_of(this).items;
    if start as usize > items.len() {
        if !actual.is_null() {
            *actual = 0;
        }
        return E_BOUNDS;
    }
    copy_many(items, start as usize, cap, out, actual)
}
unsafe extern "win64" fn it_first(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let base = this - 8;
    view_of(base).refs.fetch_add(1, Ordering::SeqCst);
    *out = Box::into_raw(Box::new(Iter {
        vt: &ITERATOR_VTBL,
        refs: AtomicU32::new(1),
        view: base,
        pos: AtomicU32::new(0),
    })) as usize;
    S_OK
}

static VIEW_VTBL: VTable<10> = VTable([
    v_qi as *const (),
    v_add_ref as *const (),
    v_release as *const (),
    get_iids as *const (),
    view_class_name as *const (),
    trust_level as *const (),
    v_get_at as *const (),
    v_size as *const (),
    v_index_of as *const (),
    v_get_many as *const (),
]);

static ITERABLE_VTBL: VTable<7> = VTable([
    it_qi as *const (),
    it_add_ref as *const (),
    it_release as *const (),
    get_iids as *const (),
    view_class_name as *const (),
    trust_level as *const (),
    it_first as *const (),
]);

// ============================================================
//  IIterator<String>
// ============================================================

#[repr(C)]
struct Iter {
    vt: &'static VTable<10>,
    refs: AtomicU32,
    /// Vue parcourue, tenue par une référence.
    view: usize,
    pos: AtomicU32,
}

unsafe fn iter_of(this: usize) -> &'static Iter {
    &*(this as *const Iter)
}
unsafe fn iter_items(this: usize) -> &'static [String] {
    &view_of(iter_of(this).view).items
}

unsafe extern "win64" fn r_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_ITERATOR_STRING,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        iter_of(this).refs.fetch_add(1, Ordering::SeqCst);
        *out = this;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn r_add_ref(this: usize) -> u32 {
    iter_of(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn r_release(this: usize) -> u32 {
    let n = iter_of(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let it = Box::from_raw(this as *mut Iter);
        view_release(it.view);
    }
    n
}
unsafe extern "win64" fn iter_class_name(_this: usize, out: *mut usize) -> i32 {
    hstring(ITER_CLASS, out)
}
unsafe extern "win64" fn r_current(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let pos = iter_of(this).pos.load(Ordering::SeqCst) as usize;
    match iter_items(this).get(pos) {
        Some(s) => hstring(s, out),
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn r_has_current(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let pos = iter_of(this).pos.load(Ordering::SeqCst) as usize;
    *out = u8::from(pos < iter_items(this).len());
    S_OK
}
unsafe extern "win64" fn r_move_next(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let len = iter_items(this).len() as u32;
    let it = iter_of(this);
    let pos = it.pos.load(Ordering::SeqCst);
    let next = if pos < len { pos + 1 } else { len };
    it.pos.store(next, Ordering::SeqCst);
    *out = u8::from(next < len);
    S_OK
}
unsafe extern "win64" fn r_get_many(
    this: usize,
    cap: u32,
    out: *mut usize,
    actual: *mut u32,
) -> i32 {
    let it = iter_of(this);
    let pos = it.pos.load(Ordering::SeqCst);
    let hr = copy_many(iter_items(this), pos as usize, cap, out, actual);
    if hr == S_OK {
        it.pos.store(pos + *actual, Ordering::SeqCst);
    }
    hr
}

static ITERATOR_VTBL: VTable<10> = VTable([
    r_qi as *const (),
    r_add_ref as *const (),
    r_release as *const (),
    get_iids as *const (),
    iter_class_name as *const (),
    trust_level as *const (),
    r_current as *const (),
    r_has_current as *const (),
    r_move_next as *const (),
    r_get_many as *const (),
]);

// ============================================================
//  IGlobalizationPreferencesStatics
// ============================================================

#[repr(C)]
struct Statics {
    vtbl: &'static VTable<12>,
}

static STATICS: Statics = Statics {
    vtbl: &STATICS_VTBL,
};

unsafe extern "win64" fn st_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if [
        IID_STATICS,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        *out = std::ptr::addr_of!(STATICS) as usize;
        S_OK
    } else {
        *out = 0;
        E_NOINTERFACE
    }
}
/// Fabrique statique : jamais libérée.
extern "win64" fn st_add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn st_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn st_class_name(_this: usize, out: *mut usize) -> i32 {
    hstring(CLASS, out)
}

unsafe fn view_out(items: Vec<String>, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = new_view(items);
    S_OK
}
unsafe extern "win64" fn st_calendars(_this: usize, out: *mut usize) -> i32 {
    view_out(vec!["GregorianCalendar".into()], out)
}
unsafe extern "win64" fn st_clocks(_this: usize, out: *mut usize) -> i32 {
    view_out(vec![prefs().clock.into()], out)
}
unsafe extern "win64" fn st_currencies(_this: usize, out: *mut usize) -> i32 {
    view_out(vec![prefs().currency], out)
}
unsafe extern "win64" fn st_languages(_this: usize, out: *mut usize) -> i32 {
    view_out(vec![prefs().language], out)
}
unsafe extern "win64" fn st_home_region(_this: usize, out: *mut usize) -> i32 {
    hstring(&prefs().region, out)
}
unsafe extern "win64" fn st_week_starts_on(_this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = prefs().week_starts_on;
    S_OK
}

static STATICS_VTBL: VTable<12> = VTable([
    st_qi as *const (),
    st_add_ref as *const (),
    st_release as *const (),
    get_iids as *const (),
    st_class_name as *const (),
    trust_level as *const (),
    st_calendars as *const (),
    st_clocks as *const (),
    st_currencies as *const (),
    st_languages as *const (),
    st_home_region as *const (),
    st_week_starts_on as *const (),
]);

/// `RoGetActivationFactory` pour GlobalizationPreferences, `None` pour une
/// autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(st_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn slot(obj: usize, n: usize) -> *const () {
        *(*(obj as *const *const *const ())).add(n)
    }
    unsafe fn text(h: usize) -> String {
        String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h))
    }

    /// Teams (#2172) : la fabrique répond et ses vues se parcourent.
    #[test]
    fn preferences_de_la_session_2172() {
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_STATICS.as_ptr(), &mut f),
                Some(S_OK)
            );
            let mut other = 0usize;
            assert!(
                activation_factory("Windows.Other", IID_STATICS.as_ptr(), &mut other).is_none()
            );
            let call: unsafe extern "win64" fn(usize, *mut usize) -> i32 =
                std::mem::transmute(slot(f, 6));
            let mut cal = 0usize;
            assert_eq!(call(f, &mut cal), S_OK);
            let size: unsafe extern "win64" fn(usize, *mut u32) -> i32 =
                std::mem::transmute(slot(cal, 7));
            let mut n = 0u32;
            assert_eq!(size(cal, &mut n), S_OK);
            assert_eq!(n, 1);
            let get_at: unsafe extern "win64" fn(usize, u32, *mut usize) -> i32 =
                std::mem::transmute(slot(cal, 6));
            let mut h = 0usize;
            assert_eq!(get_at(cal, 0, &mut h), S_OK);
            assert_eq!(text(h), "GregorianCalendar");
            assert_eq!(get_at(cal, 1, &mut h), E_BOUNDS);

            // IIterable puis IIterator.
            let mut iterable = 0usize;
            assert_eq!(v_qi(cal, IID_ITERABLE_STRING.as_ptr(), &mut iterable), S_OK);
            assert_eq!(iterable, cal + 8);
            let mut it = 0usize;
            assert_eq!(it_first(iterable, &mut it), S_OK);
            let mut has = 0u8;
            assert_eq!(r_has_current(it, &mut has), S_OK);
            assert_eq!(has, 1);
            assert_eq!(r_current(it, &mut h), S_OK);
            assert_eq!(text(h), "GregorianCalendar");
            assert_eq!(r_move_next(it, &mut has), S_OK);
            assert_eq!(has, 0);
            assert_eq!(r_current(it, &mut h), E_BOUNDS);
            assert_eq!(r_release(it), 0);
            assert_eq!(it_release(iterable), 1);
            assert_eq!(v_release(cal), 0);

            // Langue, région, horloge et premier jour cohérents avec la locale.
            let (name, region, currency, clock24, first_day) =
                super::super::nls::user_preferences();
            let mut v = 0usize;
            assert_eq!(st_languages(f, &mut v), S_OK);
            assert_eq!(v_get_at(v, 0, &mut h), S_OK);
            assert_eq!(text(h), name);
            v_release(v);
            assert_eq!(st_currencies(f, &mut v), S_OK);
            assert_eq!(v_get_at(v, 0, &mut h), S_OK);
            assert_eq!(text(h), currency);
            v_release(v);
            assert_eq!(st_clocks(f, &mut v), S_OK);
            assert_eq!(v_get_at(v, 0, &mut h), S_OK);
            assert_eq!(
                text(h),
                if clock24 {
                    "24HourClock"
                } else {
                    "12HourClock"
                }
            );
            v_release(v);
            assert_eq!(st_home_region(f, &mut h), S_OK);
            assert_eq!(text(h), region);
            let mut d = -1i32;
            assert_eq!(st_week_starts_on(f, &mut d), S_OK);
            assert_eq!(d, ((first_day + 1) % 7) as i32);
            assert_eq!(st_home_region(f, std::ptr::null_mut()), E_POINTER);
        }
    }
}
