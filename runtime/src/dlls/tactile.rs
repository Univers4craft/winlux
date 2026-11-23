//! Fonctions tactiles de user32 absentes jusqu'ici (#1932) : un .exe qui en
//! importait une s'arrêtait en 0xC0000139 avant sa première fenêtre.
//!
//! Elles répondent comme Windows sur un PC dont aucun message n'est de type
//! PT_TOUCH (#1528 : le toucher arrive encore aux programmes en souris) :
//! - GetPointerTouchInfo(History) : ERROR_DATATYPE_MISMATCH pour un pointeur
//!   souris ou stylet, ERROR_INVALID_PARAMETER pour un identifiant inconnu ;
//! - GetPointerInputTransform : ERROR_NO_DATA, aucune transformation n'est
//!   jamais appliquée à une fenêtre ;
//! - GetCurrentInputMessageSource : clavier, souris ou stylet pour un message
//!   venu du serveur X, IMDT_UNAVAILABLE pour un message posté ou envoyé ;
//! - Register/UnregisterPointerInputTarget : ERROR_ACCESS_DENIED, aucun
//!   programme n'a le privilège UI Access ;
//! - EvaluateProximityToRect/Polygon : calculés (inclusion, intersection,
//!   point ajusté sur l'élément le plus proche) ;
//! - Set/GetGestureConfig : réglages gardés par fenêtre.
//!
//! L'injection (InjectTouchInput, InjectSyntheticPointerInput) tape le
//! contact principal en souris par `saisie` : la même autorisation
//! « input » que SendInput, demandée à la personne, jamais implicite.
//!
//! Sources : documentation publique Learn de chaque fonction et structure
//! (sdk-api, winuser.h), constantes de winuser.h de MinGW-w64 (domaine
//! public) : MAX_TOUCH_COUNT, TOUCH_FEEDBACK_*, TOUCH_HIT_TESTING_*,
//! TOUCH_HIT_TESTING_PROXIMITY_CLOSEST/FARTHEST, GESTURECONFIG.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::dlls::{gui, pointer, saisie};
use crate::process::set_last_error;

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NO_DATA: u32 = 232;
const ERROR_INVALID_WINDOW_HANDLE: u32 = 1400;
const ERROR_DATATYPE_MISMATCH: u32 = 1629;

const PT_TOUCH: u32 = 2;
const PT_PEN: u32 = 3;
const MAX_TOUCH_COUNT: u32 = 256;

// POINTER_FLAGS
const PF_INRANGE: u32 = 0x2;
const PF_INCONTACT: u32 = 0x4;
const PF_DOWN: u32 = 0x1_0000;
const PF_UPDATE: u32 = 0x2_0000;
const PF_UP: u32 = 0x4_0000;

const TOUCH_HIT_TESTING_NONE: u32 = 0x2;
const PROXIMITY_CLOSEST: u16 = 0x0;
const PROXIMITY_FARTHEST: u16 = 0xFFF;

/// sizeof(GESTURECONFIG) : dwID, dwWant, dwBlock.
const GESTURECONFIG_SIZE: u32 = 12;
const GCF_INCLUDE_ANCESTORS: u32 = 0x1;

fn fail(code: u32) -> i32 {
    set_last_error(code);
    0
}

fn ptr_size() -> usize {
    crate::wow32::pointer_size()
}

unsafe fn get32(base: *const u8, at: usize) -> u32 {
    std::ptr::read_unaligned(base.add(at) as *const u32)
}

unsafe fn put32(base: *mut u8, at: usize, v: u32) {
    std::ptr::write_unaligned(base.add(at) as *mut u32, v);
}

// ── Injection ───────────────────────────────────────────────────────────

/// POINTER_TOUCH_INFO : POINTER_INFO puis touchFlags, touchMask,
/// rcContact, rcContactRaw, orientation, pressure (44 octets), alignée sur
/// la taille d'un pointeur : 144 octets en 64 bits, 132 en 32 bits.
pub fn touch_info_size(ps: usize) -> usize {
    (pointer::pointer_info_size(ps) + 44 + ps - 1) & !(ps - 1)
}

/// POINTER_TYPE_INFO : type, puis l'union POINTER_TOUCH_INFO /
/// POINTER_PEN_INFO alignée sur un pointeur (152 octets en 64 bits, 136 en
/// 32 bits ; l'union a la taille de sa plus grande branche, le toucher).
pub fn type_info_size(ps: usize) -> usize {
    ps + touch_info_size(ps)
}

/// Un contact lu dans POINTER_INFO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Contact {
    kind: u32,
    flags: u32,
    x: i32,
    y: i32,
}

/// POINTER_INFO : pointerType 0, pointerFlags 12, ptPixelLocation après
/// sourceDevice et hwndTarget.
unsafe fn read_contact(info: *const u8, ps: usize) -> Contact {
    let at = 16 + 2 * ps;
    Contact {
        kind: get32(info, 0),
        flags: get32(info, 12),
        x: get32(info, at) as i32,
        y: get32(info, at + 4) as i32,
    }
}

/// Combinaisons d'états admises par InjectTouchInput (Learn, remarques) :
/// survol qui commence ou bouge, appui, contact qui bouge, levée vers le
/// survol, fin de survol, fin.
fn valid_flags(flags: u32) -> bool {
    let state = flags & (PF_INRANGE | PF_INCONTACT | PF_DOWN | PF_UPDATE | PF_UP);
    [
        PF_INRANGE | PF_UPDATE,
        PF_INRANGE | PF_INCONTACT | PF_DOWN,
        PF_INRANGE | PF_INCONTACT | PF_UPDATE,
        PF_INRANGE | PF_UP,
        PF_UPDATE,
        PF_UP,
    ]
    .contains(&state)
}

/// Le contact principal (le premier du tableau) devient la souris, comme
/// la promotion du toucher en souris sous Windows : appui = bouton gauche
/// enfoncé, contact qui bouge = déplacement, levée = bouton relâché. Le
/// survol ne déplace pas le pointeur. `pressed` suit le bouton tenu.
fn mouse_events(c: &Contact, screen: (i32, i32), pressed: bool) -> (Vec<saisie::Event>, bool) {
    let (w, h) = screen;
    let motion = saisie::Event::Motion {
        absolute: true,
        x: (c.x as i64 * 65535 / (w - 1).max(1) as i64) as i32,
        y: (c.y as i64 * 65535 / (h - 1).max(1) as i64) as i32,
    };
    let button = |down| saisie::Event::Button { button: 1, down };
    if c.flags & PF_DOWN != 0 {
        (vec![motion, button(true)], true)
    } else if c.flags & PF_UP != 0 {
        (if pressed { vec![button(false)] } else { vec![] }, false)
    } else if c.flags & PF_INCONTACT != 0 {
        (vec![motion], pressed)
    } else {
        (vec![], pressed)
    }
}

/// Vérifie le tableau de contacts puis tape le contact principal.
/// `stride` : taille d'un élément ; `info_at` : place de POINTER_INFO.
unsafe fn inject(
    contacts: *const u8,
    count: u32,
    stride: usize,
    info_at: usize,
    kind: u32,
    pressed: &mut bool,
) -> i32 {
    let ps = ptr_size();
    let (w, h) = gui::screen_size();
    let mut first = None;
    for i in 0..count as usize {
        let c = read_contact(contacts.add(i * stride + info_at), ps);
        // Coordonnées dans les limites du bureau (Learn, paramètre contacts).
        if c.kind != kind || !valid_flags(c.flags) || c.x < 0 || c.y < 0 || c.x >= w || c.y >= h {
            return fail(ERROR_INVALID_PARAMETER);
        }
        first.get_or_insert(c);
    }
    let Some(c) = first else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    let (events, now) = mouse_events(&c, (w, h), *pressed);
    match saisie::inject(&events) {
        Ok(()) => {
            *pressed = now;
            1
        }
        Err(code) => fail(code),
    }
}

/// Injection tactile du processus : nombre maximal de contacts, bouton tenu.
static TOUCH_INJECTION: Mutex<Option<(u32, bool)>> = Mutex::new(None);

extern "win64" fn initialize_touch_injection(max_count: u32, mode: u32) -> i32 {
    // maxCount de 1 à MAX_TOUCH_COUNT ; dwMode TOUCH_FEEDBACK_DEFAULT (1),
    // INDIRECT (2) ou NONE (3).
    if max_count == 0 || max_count > MAX_TOUCH_COUNT || !(1..=3).contains(&mode) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut state = super::lock(&TOUCH_INJECTION);
    let pressed = state.is_some_and(|s| s.1);
    *state = Some((max_count, pressed));
    1
}

unsafe extern "win64" fn inject_touch_input(count: u32, contacts: *const u8) -> i32 {
    let mut state = super::lock(&TOUCH_INJECTION);
    // Sans InitializeTouchInjection, aucun contexte d'injection : refus.
    let Some((max, pressed)) = state.as_mut() else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    if count == 0 || count > *max || contacts.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    inject(
        contacts,
        count,
        touch_info_size(ptr_size()),
        0,
        PT_TOUCH,
        pressed,
    )
}

struct SyntheticDevice {
    kind: u32,
    max: u32,
    pressed: bool,
}

static SYNTHETIC: Mutex<Option<HashMap<usize, SyntheticDevice>>> = Mutex::new(None);
static NEXT_DEVICE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0x5359_0001);

extern "win64" fn create_synthetic_pointer_device(kind: u32, max_count: u32, mode: u32) -> usize {
    // PT_TOUCH : 1 à MAX_TOUCH_COUNT contacts ; PT_PEN : exactement 1 ;
    // mode POINTER_FEEDBACK_DEFAULT (1), INDIRECT (2) ou NONE (3).
    let count_ok = match kind {
        PT_TOUCH => (1..=MAX_TOUCH_COUNT).contains(&max_count),
        PT_PEN => max_count == 1,
        _ => false,
    };
    if !count_ok || !(1..=3).contains(&mode) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let handle = NEXT_DEVICE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    super::lock(&SYNTHETIC)
        .get_or_insert_with(HashMap::new)
        .insert(
            handle,
            SyntheticDevice {
                kind,
                max: max_count,
                pressed: false,
            },
        );
    handle
}

unsafe extern "win64" fn inject_synthetic_pointer_input(
    device: usize,
    info: *const u8,
    count: u32,
) -> i32 {
    let mut map = super::lock(&SYNTHETIC);
    let Some(d) = map.as_mut().and_then(|m| m.get_mut(&device)) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if count == 0 || count > d.max || info.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let ps = ptr_size();
    // Le type de chaque élément doit être celui du périphérique.
    for i in 0..count as usize {
        if get32(info.add(i * type_info_size(ps)), 0) != d.kind {
            return fail(ERROR_INVALID_PARAMETER);
        }
    }
    let kind = d.kind;
    inject(info, count, type_info_size(ps), ps, kind, &mut d.pressed)
}

extern "win64" fn destroy_synthetic_pointer_device(device: usize) {
    let removed = super::lock(&SYNTHETIC)
        .as_mut()
        .and_then(|m| m.remove(&device));
    // Un contact resté posé est levé, pour ne pas laisser le bouton tenu.
    if removed.is_some_and(|d| d.pressed) {
        let _ = saisie::inject(&[saisie::Event::Button {
            button: 1,
            down: false,
        }]);
    }
}

// ── Informations de pointeur ────────────────────────────────────────────

/// Aucun message n'est de type PT_TOUCH : un pointeur connu (souris,
/// stylet) n'est pas tactile, un identifiant inconnu est invalide.
fn not_touch(id: u32) -> i32 {
    if pointer::known_pointer(id) {
        fail(ERROR_DATATYPE_MISMATCH)
    } else {
        fail(ERROR_INVALID_PARAMETER)
    }
}

unsafe extern "win64" fn get_pointer_touch_info(id: u32, out: *mut u8) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    not_touch(id)
}

unsafe extern "win64" fn get_pointer_touch_info_history(
    id: u32,
    entries: *mut u32,
    out: *mut u8,
) -> i32 {
    if entries.is_null() || (*entries != 0 && out.is_null()) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    not_touch(id)
}

/// Winlux ne livre qu'une entrée par message (historyCount 1) et aucune
/// fenêtre n'a de transformation d'entrée.
unsafe extern "win64" fn get_pointer_input_transform(id: u32, history: u32, out: *mut u8) -> i32 {
    if out.is_null() || history == 0 || history > 1 || !pointer::known_pointer(id) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    fail(ERROR_NO_DATA)
}

/// INPUT_MESSAGE_SOURCE : deviceType, originId. L'origine (matériel ou
/// SendInput) n'est pas distinguée : IMO_UNAVAILABLE.
unsafe fn write_source(out: *mut u8, device: u32) {
    put32(out, 0, device);
    put32(out, 4, 0);
}

unsafe extern "win64" fn get_current_input_message_source(out: *mut u8) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    write_source(out, pointer::current_source());
    1
}

/// GetCIMSSM : à n'appeler que si GetCurrentInputMessageSource rend
/// IMDT_UNAVAILABLE, sinon ERROR_INVALID_PARAMETER (Learn). Un message
/// envoyé ne porte pas son appareil d'origine dans Winlux.
unsafe extern "win64" fn get_cimssm(out: *mut u8) -> i32 {
    if out.is_null() || pointer::current_source() != pointer::IMDT_UNAVAILABLE {
        return fail(ERROR_INVALID_PARAMETER);
    }
    write_source(out, pointer::IMDT_UNAVAILABLE);
    1
}

// ── Cibles de redirection et test de toucher ────────────────────────────

/// Réservé aux programmes qui ont le privilège UI Access (Learn) ; Winlux
/// ne l'accorde à aucun.
extern "win64" fn register_pointer_input_target(_hwnd: usize, _kind: u32) -> i32 {
    fail(ERROR_ACCESS_DENIED)
}

extern "win64" fn unregister_pointer_input_target(_hwnd: usize, _kind: u32) -> i32 {
    fail(ERROR_ACCESS_DENIED)
}

/// Fenêtres inscrites pour WM_TOUCHHITTESTING (TOUCH_HIT_TESTING_*). Le
/// message part avec le toucher en WM_POINTER (#1528).
static HIT_TESTING: Mutex<Option<HashMap<usize, u32>>> = Mutex::new(None);

extern "win64" fn register_touch_hit_testing_window(hwnd: usize, value: u32) -> i32 {
    if !gui::is_window(hwnd) {
        return fail(ERROR_INVALID_WINDOW_HANDLE);
    }
    if value > TOUCH_HIT_TESTING_NONE {
        return fail(ERROR_INVALID_PARAMETER);
    }
    super::lock(&HIT_TESTING)
        .get_or_insert_with(HashMap::new)
        .insert(hwnd, value);
    1
}

#[cfg(test)]
fn hit_testing_of(hwnd: usize) -> Option<u32> {
    super::lock(&HIT_TESTING)
        .as_ref()
        .and_then(|m| m.get(&hwnd).copied())
}

/// TOUCH_HIT_TESTING_INPUT : pointerId, point, boundingBox,
/// nonOccludedBoundingBox, orientation (48 octets, sans pointeur).
struct HitInput {
    point: (i32, i32),
    bounds: [i32; 4],
}

unsafe fn read_hit_input(p: *const u8) -> HitInput {
    let i = |at| get32(p, at) as i32;
    HitInput {
        point: (i(4), i(8)),
        bounds: [i(12), i(16), i(20), i(24)],
    }
}

fn inside_box(b: &[i32; 4], (x, y): (i32, i32)) -> bool {
    x >= b[0] && x < b[2] && y >= b[1] && y < b[3]
}

/// Score d'un élément touché : CLOSEST si le point est dedans (inclusion),
/// FARTHEST s'il ne croise pas la zone de contact (intersection), sinon
/// croissant avec la distance au point le plus proche de l'élément,
/// rapportée à la demi-diagonale de la zone de contact. Learn ne fixe pas
/// l'échelle entre les deux bornes (déduction).
fn score(input: &HitInput, inside: bool, nearest: (i32, i32)) -> (u16, (i32, i32)) {
    if inside {
        return (PROXIMITY_CLOSEST, input.point);
    }
    if !inside_box(&input.bounds, nearest) {
        return (PROXIMITY_FARTHEST, input.point);
    }
    let d = |a: (i32, i32), b: (i32, i32)| {
        (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt()
    };
    let b = &input.bounds;
    let reach = [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])]
        .into_iter()
        .map(|c| d(c, input.point))
        .fold(1.0, f64::max);
    let s = 1.0 + d(nearest, input.point) / reach * f64::from(PROXIMITY_FARTHEST - 2);
    (
        s.round().clamp(1.0, f64::from(PROXIMITY_FARTHEST - 1)) as u16,
        nearest,
    )
}

/// TOUCH_HIT_TESTING_PROXIMITY_EVALUATION : score (UINT16), puis
/// adjustedPoint aligné sur 4.
unsafe fn write_eval(out: *mut u8, (s, p): (u16, (i32, i32))) {
    std::ptr::write_unaligned(out as *mut u16, s);
    put32(out, 4, p.0 as u32);
    put32(out, 8, p.1 as u32);
}

unsafe extern "win64" fn evaluate_proximity_to_rect(
    rect: *const i32,
    input: *const u8,
    out: *mut u8,
) -> i32 {
    if rect.is_null() || input.is_null() || out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let r = [*rect, *rect.add(1), *rect.add(2), *rect.add(3)];
    if r[2] <= r[0] || r[3] <= r[1] {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let input = read_hit_input(input);
    let (x, y) = input.point;
    let nearest = (x.clamp(r[0], r[2] - 1), y.clamp(r[1], r[3] - 1));
    write_eval(out, score(&input, inside_box(&r, input.point), nearest));
    1
}

/// Point du segment [a, b] le plus proche de p.
fn nearest_on_segment(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx * dx + dy * dy;
    let t = if len == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len).clamp(0.0, 1.0)
    };
    (a.0 + t * dx, a.1 + t * dy)
}

fn inside_polygon(poly: &[(i32, i32)], (x, y): (i32, i32)) -> bool {
    let (x, y) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = (f64::from(poly[i].0), f64::from(poly[i].1));
        let (xj, yj) = (f64::from(poly[j].0), f64::from(poly[j].1));
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

unsafe extern "win64" fn evaluate_proximity_to_polygon(
    count: u32,
    points: *const i32,
    input: *const u8,
    out: *mut u8,
) -> i32 {
    // Au moins trois sommets (Learn, numVertices).
    if count < 3 || points.is_null() || input.is_null() || out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let poly: Vec<(i32, i32)> = (0..count as usize)
        .map(|i| (*points.add(2 * i), *points.add(2 * i + 1)))
        .collect();
    let input = read_hit_input(input);
    let p = (f64::from(input.point.0), f64::from(input.point.1));
    let as_f = |v: (i32, i32)| (f64::from(v.0), f64::from(v.1));
    let nearest = (0..poly.len())
        .map(|i| nearest_on_segment(as_f(poly[i]), as_f(poly[(i + 1) % poly.len()]), p))
        .min_by(|a, b| {
            let da = (a.0 - p.0).powi(2) + (a.1 - p.1).powi(2);
            let db = (b.0 - p.0).powi(2) + (b.1 - p.1).powi(2);
            da.total_cmp(&db)
        })
        .map_or(input.point, |n| (n.0.round() as i32, n.1.round() as i32));
    write_eval(
        out,
        score(&input, inside_polygon(&poly, input.point), nearest),
    );
    1
}

// ── Gestes ──────────────────────────────────────────────────────────────

/// GESTURECONFIG gardés par fenêtre : dwID → (dwWant, dwBlock).
static GESTURES: Mutex<Option<HashMap<usize, HashMap<u32, (u32, u32)>>>> = Mutex::new(None);

/// dwID : 0 (réglage global) ou GID_ZOOM (3) à GID_PRESSANDTAP (7).
fn gesture_id_ok(id: u32) -> bool {
    id == 0 || (3..=7).contains(&id)
}

unsafe extern "win64" fn set_gesture_config(
    hwnd: usize,
    reserved: u32,
    count: u32,
    configs: *const u32,
    size: u32,
) -> i32 {
    // cbSize autre que sizeof(GESTURECONFIG) : ERROR_INVALID_PARAMETER
    // (Learn) ; dwReserved doit valoir 0.
    if size != GESTURECONFIG_SIZE || reserved != 0 || count == 0 || configs.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if !gui::is_window(hwnd) {
        return fail(ERROR_INVALID_WINDOW_HANDLE);
    }
    let items: Vec<[u32; 3]> = (0..count as usize)
        .map(|i| {
            [
                *configs.add(3 * i),
                *configs.add(3 * i + 1),
                *configs.add(3 * i + 2),
            ]
        })
        .collect();
    if items.iter().any(|c| !gesture_id_ok(c[0])) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut all = super::lock(&GESTURES);
    let window = all
        .get_or_insert_with(HashMap::new)
        .entry(hwnd)
        .or_default();
    for [id, want, block] in items {
        window.insert(id, (want, block));
    }
    1
}

/// Rend pour chaque dwID demandé le réglage posé par SetGestureConfig sur
/// la fenêtre (ou, avec GCF_INCLUDE_ANCESTORS, sur la plus proche de ses
/// parentes) ; un dwID jamais réglé rend dwWant = dwBlock = 0.
unsafe extern "win64" fn get_gesture_config(
    hwnd: usize,
    reserved: u32,
    flags: u32,
    count: *mut u32,
    configs: *mut u32,
    size: u32,
) -> i32 {
    if size != GESTURECONFIG_SIZE || reserved != 0 || count.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if *count != 0 && configs.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if !gui::is_window(hwnd) {
        return fail(ERROR_INVALID_WINDOW_HANDLE);
    }
    let mut chain = vec![hwnd];
    if flags & GCF_INCLUDE_ANCESTORS != 0 {
        while let Some(p) = chain.last().and_then(|&h| gui::window_parent(h)) {
            if p == 0 || chain.contains(&p) {
                break;
            }
            chain.push(p);
        }
    }
    let all = super::lock(&GESTURES);
    for i in 0..*count as usize {
        let entry = configs.add(3 * i);
        let id = *entry;
        let (want, block) = chain
            .iter()
            .find_map(|h| all.as_ref()?.get(h)?.get(&id).copied())
            .unwrap_or((0, 0));
        *entry.add(1) = want;
        *entry.add(2) = block;
    }
    1
}

/// GetGestureExtraArgs : Winlux n'envoie jamais WM_GESTURE, aucun
/// HGESTUREINFO n'est valide (comme GetGestureInfo).
unsafe extern "win64" fn get_gesture_extra_args(_handle: usize, _size: u32, _out: *mut u8) -> i32 {
    fail(ERROR_INVALID_HANDLE)
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "InitializeTouchInjection" => fn initialize_touch_injection,
        "InjectTouchInput" => fn inject_touch_input,
        "CreateSyntheticPointerDevice" => fn create_synthetic_pointer_device,
        "InjectSyntheticPointerInput" => fn inject_synthetic_pointer_input,
        "DestroySyntheticPointerDevice" => fn destroy_synthetic_pointer_device,
        "GetPointerTouchInfo" => fn get_pointer_touch_info,
        "GetPointerTouchInfoHistory" => fn get_pointer_touch_info_history,
        "GetPointerInputTransform" => fn get_pointer_input_transform,
        "GetCurrentInputMessageSource" => fn get_current_input_message_source,
        "GetCIMSSM" => fn get_cimssm,
        "RegisterPointerInputTarget" => fn register_pointer_input_target,
        "UnregisterPointerInputTarget" => fn unregister_pointer_input_target,
        "RegisterTouchHitTestingWindow" => fn register_touch_hit_testing_window,
        "EvaluateProximityToRect" => fn evaluate_proximity_to_rect,
        "EvaluateProximityToPolygon" => fn evaluate_proximity_to_polygon,
        "SetGestureConfig" => fn set_gesture_config,
        "GetGestureConfig" => fn get_gesture_config,
        "GetGestureExtraArgs" => fn get_gesture_extra_args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn last_error() -> u32 {
        crate::process::last_error()
    }

    #[test]
    fn tailles_des_structures_tactiles_1932() {
        assert_eq!(touch_info_size(8), 144);
        assert_eq!(touch_info_size(4), 132);
        assert_eq!(type_info_size(8), 152);
        assert_eq!(type_info_size(4), 136);
    }

    #[test]
    fn contact_principal_en_souris_1932() {
        let c = |flags| Contact {
            kind: PT_TOUCH,
            flags,
            x: 100,
            y: 50,
        };
        let screen = (201, 101);
        let (down, pressed) = mouse_events(&c(PF_INRANGE | PF_INCONTACT | PF_DOWN), screen, false);
        assert!(pressed);
        assert_eq!(
            down,
            vec![
                saisie::Event::Motion {
                    absolute: true,
                    x: 32767,
                    y: 32767
                },
                saisie::Event::Button {
                    button: 1,
                    down: true
                }
            ]
        );
        let (moved, pressed) =
            mouse_events(&c(PF_INRANGE | PF_INCONTACT | PF_UPDATE), screen, true);
        assert!(pressed && moved.len() == 1);
        let (up, pressed) = mouse_events(&c(PF_INRANGE | PF_UP), screen, true);
        assert!(!pressed);
        assert_eq!(
            up,
            vec![saisie::Event::Button {
                button: 1,
                down: false
            }]
        );
        // Survol : rien n'est tapé.
        assert!(mouse_events(&c(PF_INRANGE | PF_UPDATE), screen, false)
            .0
            .is_empty());
        assert!(valid_flags(PF_UP) && valid_flags(PF_INRANGE | PF_INCONTACT | PF_DOWN));
        assert!(!valid_flags(PF_DOWN));
        assert!(!valid_flags(PF_INCONTACT | PF_UPDATE));
    }

    #[test]
    fn injection_sans_contexte_ni_parametres_valides_1932() {
        let _teb = crate::process::test_teb();
        assert_eq!(initialize_touch_injection(0, 1), 0);
        assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
        assert_eq!(initialize_touch_injection(257, 1), 0);
        assert_eq!(initialize_touch_injection(10, 4), 0);
        assert_eq!(create_synthetic_pointer_device(PT_PEN, 2, 1), 0);
        assert_eq!(create_synthetic_pointer_device(4, 1, 1), 0);
        let device = create_synthetic_pointer_device(PT_TOUCH, 2, 3);
        assert_ne!(device, 0);
        let ps = ptr_size();
        let mut info = vec![0u8; type_info_size(ps) * 3];
        unsafe {
            // Plus de contacts que le maximum déclaré.
            assert_eq!(inject_synthetic_pointer_input(device, info.as_ptr(), 3), 0);
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
            // Type d'élément différent de celui du périphérique.
            put32(info.as_mut_ptr(), 0, PT_PEN);
            assert_eq!(inject_synthetic_pointer_input(device, info.as_ptr(), 1), 0);
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
            assert_eq!(inject_synthetic_pointer_input(0x1234, info.as_ptr(), 1), 0);
            assert_eq!(last_error(), ERROR_INVALID_HANDLE);
        }
        destroy_synthetic_pointer_device(device);
        unsafe {
            assert_eq!(inject_synthetic_pointer_input(device, info.as_ptr(), 1), 0);
            assert_eq!(last_error(), ERROR_INVALID_HANDLE);
        }
    }

    #[test]
    fn aucun_pointeur_tactile_1932() {
        let _teb = crate::process::test_teb();
        let mut out = [0u8; 160];
        let mut n = 1u32;
        unsafe {
            assert_eq!(get_pointer_touch_info(0x7777, out.as_mut_ptr()), 0);
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
            assert_eq!(
                get_pointer_touch_info_history(0x7777, &mut n, out.as_mut_ptr()),
                0
            );
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
            assert_eq!(get_pointer_input_transform(0x7777, 1, out.as_mut_ptr()), 0);
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
            assert_eq!(register_pointer_input_target(0x10, PT_TOUCH), 0);
            assert_eq!(last_error(), ERROR_ACCESS_DENIED);
            assert_eq!(unregister_pointer_input_target(0x10, PT_TOUCH), 0);
            assert_eq!(last_error(), ERROR_ACCESS_DENIED);
            assert_eq!(get_gesture_extra_args(0x10, 4, out.as_mut_ptr()), 0);
            assert_eq!(last_error(), ERROR_INVALID_HANDLE);
        }
    }

    #[test]
    fn source_du_message_courant_1932() {
        let _teb = crate::process::test_teb();
        let mut out = [0xFFu8; 8];
        unsafe {
            assert_eq!(get_current_input_message_source(std::ptr::null_mut()), 0);
            // Message posté par un programme : appareil non identifié.
            pointer::on_take(&gui::Msg::new(0x10, 0x0201, 1, 0));
            assert_eq!(get_current_input_message_source(out.as_mut_ptr()), 1);
            assert_eq!(out, [0; 8]);
            assert_eq!(get_cimssm(out.as_mut_ptr()), 1);
            // Clic venu du serveur X : la souris.
            let mut m = gui::Msg::new(0x10, 0x0201, 1, 0);
            m.l_private = gui::BACKEND_INPUT;
            pointer::on_take(&m);
            assert_eq!(get_current_input_message_source(out.as_mut_ptr()), 1);
            assert_eq!(get32(out.as_ptr(), 0), pointer::IMDT_MOUSE);
            assert_eq!(get_cimssm(out.as_mut_ptr()), 0);
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
            // Pendant un SendMessage : non identifié, puis rendu au retour.
            pointer::while_sent(|| assert_eq!(pointer::current_source(), 0));
            assert_eq!(pointer::current_source(), pointer::IMDT_MOUSE);
            let mut k = gui::Msg::new(0x10, 0x0100, 0x41, 0);
            k.l_private = gui::BACKEND_INPUT;
            pointer::on_take(&k);
            assert_eq!(pointer::current_source(), pointer::IMDT_KEYBOARD);
        }
    }

    fn hit_input(point: (i32, i32), bounds: [i32; 4]) -> [u8; 48] {
        let mut b = [0u8; 48];
        for (at, v) in [(4, point.0), (8, point.1)]
            .into_iter()
            .chain(bounds.iter().enumerate().map(|(i, v)| (12 + 4 * i, *v)))
        {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    fn eval(out: &[u8; 12]) -> (u16, i32, i32) {
        (
            u16::from_le_bytes([out[0], out[1]]),
            i32::from_le_bytes(out[4..8].try_into().unwrap()),
            i32::from_le_bytes(out[8..12].try_into().unwrap()),
        )
    }

    #[test]
    fn proximite_d_un_rectangle_et_d_un_polygone_1932() {
        let _teb = crate::process::test_teb();
        let input = hit_input((100, 100), [90, 90, 111, 111]);
        let mut out = [0u8; 12];
        unsafe {
            // Point dans l'élément : inchangé, score le plus proche.
            let inside = [95, 95, 120, 120];
            assert_eq!(
                evaluate_proximity_to_rect(inside.as_ptr(), input.as_ptr(), out.as_mut_ptr()),
                1
            );
            assert_eq!(eval(&out), (PROXIMITY_CLOSEST, 100, 100));
            // Élément à côté, dans la zone de contact : point ajusté dessus.
            let near = [104, 80, 140, 120];
            assert_eq!(
                evaluate_proximity_to_rect(near.as_ptr(), input.as_ptr(), out.as_mut_ptr()),
                1
            );
            let (s, x, y) = eval(&out);
            assert!(s > PROXIMITY_CLOSEST && s < PROXIMITY_FARTHEST);
            assert_eq!((x, y), (104, 100));
            // Un élément plus loin a un score plus grand.
            let far = [108, 80, 140, 120];
            evaluate_proximity_to_rect(far.as_ptr(), input.as_ptr(), out.as_mut_ptr());
            assert!(eval(&out).0 > s);
            // Hors de la zone de contact : le plus loin.
            let away = [200, 200, 220, 220];
            evaluate_proximity_to_rect(away.as_ptr(), input.as_ptr(), out.as_mut_ptr());
            assert_eq!(eval(&out), (PROXIMITY_FARTHEST, 100, 100));
            // Triangle qui contient le point, puis triangle voisin.
            let tri = [80, 80, 130, 80, 80, 130];
            assert_eq!(
                evaluate_proximity_to_polygon(3, tri.as_ptr(), input.as_ptr(), out.as_mut_ptr()),
                1
            );
            assert_eq!(eval(&out).0, PROXIMITY_CLOSEST);
            let side = [104, 90, 130, 90, 130, 110, 104, 110];
            evaluate_proximity_to_polygon(4, side.as_ptr(), input.as_ptr(), out.as_mut_ptr());
            let (s, x, y) = eval(&out);
            assert!(s > PROXIMITY_CLOSEST && s < PROXIMITY_FARTHEST);
            assert_eq!((x, y), (104, 100));
            assert_eq!(
                evaluate_proximity_to_polygon(2, tri.as_ptr(), input.as_ptr(), out.as_mut_ptr()),
                0
            );
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
        }
    }

    #[test]
    fn reglages_de_gestes_et_test_de_toucher_par_fenetre_1932() {
        let _teb = crate::process::test_teb();
        let hwnd = gui::alloc_hwnd();
        gui::insert_window(hwnd, gui::tests::fenetre_essai(0x00cf_0000, 0, 0));
        let set = [4u32, 0x1 | 0x2, 0x4, 5, 0x1, 0];
        let mut get = [5u32, 9, 9, 4, 9, 9, 3, 9, 9];
        let mut n = 3u32;
        unsafe {
            assert_eq!(set_gesture_config(hwnd, 0, 2, set.as_ptr(), 8), 0);
            assert_eq!(last_error(), ERROR_INVALID_PARAMETER);
            assert_eq!(
                set_gesture_config(hwnd, 0, 2, set.as_ptr(), GESTURECONFIG_SIZE),
                1
            );
            assert_eq!(
                get_gesture_config(hwnd, 0, 0, &mut n, get.as_mut_ptr(), GESTURECONFIG_SIZE),
                1
            );
        }
        assert_eq!(get, [5, 0x1, 0, 4, 0x3, 0x4, 3, 0, 0]);
        assert_eq!(register_touch_hit_testing_window(hwnd, 3), 0);
        assert_eq!(register_touch_hit_testing_window(hwnd, 1), 1);
        assert_eq!(hit_testing_of(hwnd), Some(1));
        assert_eq!(register_touch_hit_testing_window(0x7FFF_FFF0, 1), 0);
        assert_eq!(last_error(), ERROR_INVALID_WINDOW_HANDLE);
        gui::destroy_window(hwnd);
    }
}
