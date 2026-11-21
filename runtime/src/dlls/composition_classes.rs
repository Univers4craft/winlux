//! Classes créées par `ICompositor2` (#2715) : pinceaux de masque et à neuf
//! cases, ombre portée, lumières (ambiante, distante, ponctuelle, spot),
//! groupe d'animations, collection d'animations implicites, easing en
//! escalier ; plus les collections et parcours qu'elles exposent
//! (`VisualUnorderedCollection`, `IIterator`, `IMapView`, `IKeyValuePair`).
//!
//! Chaque propriété est gardée et relue. Le rendu logiciel de Winlux ne les
//! applique pas encore (masque, neuf cases, ombre, lumières, animations) :
//! le journal le dit une fois par classe, à la création.
//!
//! Interfaces, identifiants et ordre des méthodes : windows-rs 0.58
//! (MIT/Apache), projection des métadonnées WinRT publiques. Identifiants
//! des interfaces paramétrées : système de types publié de Windows Runtime
//! (`pinterface_iid`).

use std::sync::OnceLock;

use super::super::comobj::pinterface_iid;
use super::*;

const E_BOUNDS: i32 = 0x8000_000Bu32 as i32;

// ---------------------------------------------------------------------
// Identifiants
// ---------------------------------------------------------------------

const IID_MASK_BRUSH: Guid = iid("522cf09e-be6b-4f41-be49-f9226d471b4a");
const IID_NINE_GRID_BRUSH: Guid = iid("f25154e4-bc8c-4be7-b80f-8685b83c0186");
const IID_DROP_SHADOW: Guid = iid("cb977c07-a154-4851-85e7-a8924c84fad8");
const IID_DROP_SHADOW2: Guid = iid("6c4218bc-15b9-4c2d-8d4a-0767df11977a");
const IID_SHADOW: Guid = iid("329e52e2-4335-49cc-b14a-37782d10f0c4");
const IID_AMBIENT: Guid = iid("a48130a1-b7c4-46f7-b9bf-daf43a44e6ee");
const IID_AMBIENT2: Guid = iid("3b64a6bf-5f97-4c94-86e5-042dd386b27d");
const IID_DISTANT: Guid = iid("318cfafc-5ce3-4b55-ab5d-07a00353ac99");
const IID_DISTANT2: Guid = iid("dbcdaa1c-294b-48d7-b60e-76df64aa392b");
const IID_POINT: Guid = iid("b18545b3-0c5a-4ab0-bedc-4f3546948272");
const IID_POINT2: Guid = iid("efe98f2c-0678-4f69-b164-a810d995bcb7");
const IID_POINT3: Guid = iid("4c0a8367-d4e9-468a-87ae-7ba43ab29485");
const IID_SPOT: Guid = iid("5a9fe273-44a1-4f95-a422-8fa5116bdb44");
const IID_SPOT2: Guid = iid("64ee615e-0686-4dea-a9e8-bc3a8c701459");
const IID_SPOT3: Guid = iid("e4d03eea-131f-480e-859e-b82705b74360");
const IID_LIGHT: Guid = iid("41a6d7c2-2e5d-4bc1-b09e-8f0a03e3d8d3");
const IID_LIGHT2: Guid = iid("a7bcda72-f35d-425d-9b98-23f4205f6669");
const IID_LIGHT3: Guid = iid("4b0b00e4-df07-4959-b7a4-4f7e4233f838");
const IID_ANIMATION_GROUP: Guid = iid("5e7cc90c-cd14-4e07-8a55-c72527aabdac");
const IID_ANIMATION_BASE: Guid = iid("1c2c2999-e818-48d3-a6dd-d78c82f8ace9");
const IID_IMPLICIT: Guid = iid("0598a3ff-0a92-4c9d-a427-b25519250dbf");
const IID_STEP_EASING: Guid = iid("d0caa74b-560c-4a0b-a5f6-206ca8c3ecd6");
const IID_EASING: Guid = iid("5145e356-bf79-4ea8-8cc2-6b5b472e6c9a");
const IID_VISUAL_SET: Guid = iid("338faa70-54c8-40a7-8029-c9ceeb0aa250");

/// Identifiants des interfaces paramétrées.
pub(super) struct Generic {
    pub iterable_visual: Guid,
    pub iterator_visual: Guid,
    pub iterable_animation: Guid,
    pub iterator_animation: Guid,
    pub map: Guid,
    pub map_view: Guid,
    pub iterable_pair: Guid,
    pub iterator_pair: Guid,
    pub pair: Guid,
}

pub(super) fn generic() -> &'static Generic {
    static G: OnceLock<Generic> = OnceLock::new();
    G.get_or_init(|| {
        let iterable = "{faa585ea-6214-4217-afda-7f46de5869b3}";
        let iterator = "{6a79e863-4300-459a-9966-cbb660963ee1}";
        let visual = "rc(Windows.UI.Composition.Visual;{117e202d-a859-4c89-873b-c2aa566788e3})";
        let animation = "rc(Windows.UI.Composition.CompositionAnimation;\
                         {464c4c2c-1caa-4061-9b40-e13fde1503ca})";
        let base = "{1c2c2999-e818-48d3-a6dd-d78c82f8ace9}";
        let pair = format!("pinterface({{02b51929-c1c4-4a7e-8940-0312b5c18500}};string;{base})");
        let p = |g: &str, t: &str| pinterface_iid(&format!("pinterface({g};{t})"));
        Generic {
            iterable_visual: p(iterable, visual),
            iterator_visual: p(iterator, visual),
            iterable_animation: p(iterable, animation),
            iterator_animation: p(iterator, animation),
            map: pinterface_iid(&format!(
                "pinterface({{3c2925fe-8519-45c1-aa79-197b6718c1c1}};string;{base})"
            )),
            map_view: pinterface_iid(&format!(
                "pinterface({{e480ce40-a338-4ada-adcf-272272e48cb9}};string;{base})"
            )),
            iterable_pair: p(iterable, &pair),
            iterator_pair: p(iterator, &pair),
            pair: pinterface_iid(&pair),
        }
    })
}

// ---------------------------------------------------------------------
// Données
// ---------------------------------------------------------------------

/// Propriétés d'un objet de ce module.
pub(super) struct Props {
    /// Valeurs de 4 octets (f32, i32, Color, booléens), par indice.
    pub cells: [u32; 24],
    /// Objets tenus (Mask, Source, CoordinateSpace…).
    pub objs: [*mut c_void; 2],
    /// Éléments tenus (collections, groupe, parcours).
    pub list: Vec<*mut c_void>,
    /// Table nom → animation (valeurs tenues).
    pub map: Vec<(String, *mut c_void)>,
    /// Targets et ExclusionsFromTargets d'une lumière (tenus).
    pub sets: [*mut Obj; 2],
    /// Position d'un parcours.
    pub pos: usize,
    /// Clé d'une paire.
    pub key: String,
}

impl Props {
    fn new(cells: &[(usize, u32)]) -> Box<Props> {
        let mut p = Box::new(Props {
            cells: [0; 24],
            objs: [std::ptr::null_mut(); 2],
            list: Vec::new(),
            map: Vec::new(),
            sets: [std::ptr::null_mut(); 2],
            pos: 0,
            key: String::new(),
        });
        for &(i, v) in cells {
            p.cells[i] = v;
        }
        p
    }
}

pub(super) unsafe fn destroy(p: Box<Props>) {
    for o in p.objs {
        com_release(o);
    }
    for o in p.list {
        com_release(o);
    }
    for (_, o) in p.map {
        com_release(o);
    }
    for s in p.sets {
        if !s.is_null() {
            obj_release(s);
        }
    }
}

unsafe fn with<R>(this: *mut Slot, f: impl FnOnce(&mut Props) -> R) -> Option<R> {
    match &mut lock(&(*owner(this)).st).data {
        Data::Props(p) => Some(f(p)),
        _ => None,
    }
}

const ONE: u32 = 0x3f80_0000; // 1.0f32
const WHITE: u32 = 0xffff_ffff; // Color A=R=G=B=255
const BLACK: u32 = 0x0000_00ff; // Color A=255, R=G=B=0 (octets A, R, G, B)

// ---------------------------------------------------------------------
// Accesseurs génériques
// ---------------------------------------------------------------------

unsafe extern "win64" fn get_u32<const I: usize>(this: *mut Slot, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| p.cells[I]) {
        Some(v) => {
            *out = v;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn set_u32<const I: usize>(this: *mut Slot, v: u32) -> i32 {
    with(this, |p| p.cells[I] = v).map_or(RO_E_CLOSED, |_| S_OK)
}

unsafe extern "win64" fn get_f32<const I: usize>(this: *mut Slot, out: *mut f32) -> i32 {
    get_u32::<I>(this, out as *mut u32)
}

unsafe extern "win64" fn set_f32<const I: usize>(this: *mut Slot, v: f32) -> i32 {
    set_u32::<I>(this, v.to_bits())
}

unsafe extern "win64" fn get_bool<const I: usize>(this: *mut Slot, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| p.cells[I]) {
        Some(v) => {
            *out = (v != 0) as u8;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn set_bool<const I: usize>(this: *mut Slot, v: u8) -> i32 {
    set_u32::<I>(this, (v != 0) as u32)
}

/// Vector3 : trois cellules à partir de `I`. Une structure de 12 octets
/// passe par adresse dans la convention x64.
unsafe extern "win64" fn get_v3<const I: usize>(this: *mut Slot, out: *mut [f32; 3]) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| {
        [
            f32::from_bits(p.cells[I]),
            f32::from_bits(p.cells[I + 1]),
            f32::from_bits(p.cells[I + 2]),
        ]
    }) {
        Some(v) => {
            *out = v;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn set_v3<const I: usize>(this: *mut Slot, v: *const [f32; 3]) -> i32 {
    if v.is_null() {
        return E_POINTER;
    }
    let v = *v;
    with(this, |p| {
        for (k, x) in v.iter().enumerate() {
            p.cells[I + k] = x.to_bits();
        }
    })
    .map_or(RO_E_CLOSED, |_| S_OK)
}

unsafe extern "win64" fn get_obj<const I: usize>(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    match with(this, |p| p.objs[I]) {
        Some(o) => put_com(out, o),
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn set_obj<const I: usize>(this: *mut Slot, v: *mut c_void) -> i32 {
    com_add_ref(v);
    match with(this, |p| std::mem::replace(&mut p.objs[I], v)) {
        Some(old) => {
            com_release(old);
            S_OK
        }
        None => {
            com_release(v);
            RO_E_CLOSED
        }
    }
}

macro_rules! prop_f32 {
    ($i:literal) => {
        [get_f32::<$i> as *const (), set_f32::<$i> as *const ()]
    };
}
macro_rules! prop_u32 {
    ($i:literal) => {
        [get_u32::<$i> as *const (), set_u32::<$i> as *const ()]
    };
}
macro_rules! prop_bool {
    ($i:literal) => {
        [get_bool::<$i> as *const (), set_bool::<$i> as *const ()]
    };
}
macro_rules! prop_v3 {
    ($i:literal) => {
        [get_v3::<$i> as *const (), set_v3::<$i> as *const ()]
    };
}
macro_rules! prop_obj {
    ($i:literal) => {
        [get_obj::<$i> as *const (), set_obj::<$i> as *const ()]
    };
}

/// Concatène des paires d'accesseurs après IInspectable.
const fn props_table<const N: usize>(pairs: &[[*const (); 2]]) -> [*const (); N] {
    assert!(6 + pairs.len() * 2 == N, "taille de table");
    let mut out = [std::ptr::null::<()>(); N];
    let mut i = 0;
    while i < 6 {
        out[i] = INSPECTABLE[i];
        i += 1;
    }
    let mut k = 0;
    while k < pairs.len() {
        out[6 + 2 * k] = pairs[k][0];
        out[7 + 2 * k] = pairs[k][1];
        k += 1;
    }
    out
}

// ---------------------------------------------------------------------
// CompositionMaskBrush : objs 0 = Mask, 1 = Source
// ---------------------------------------------------------------------

static MASK_BRUSH_VT: VTable<10> = VTable(props_table::<10>(&[prop_obj!(0), prop_obj!(1)]));

// ---------------------------------------------------------------------
// CompositionNineGridBrush
// cellules : 0 BottomInset, 1 BottomInsetScale, 2 IsCenterHollow,
// 3 LeftInset, 4 LeftInsetScale, 5 RightInset, 6 RightInsetScale,
// 7 TopInset, 8 TopInsetScale ; objs 0 = Source
// ---------------------------------------------------------------------

unsafe fn set_four(this: *mut Slot, idx: [usize; 4], v: [f32; 4]) -> i32 {
    with(this, |p| {
        for k in 0..4 {
            p.cells[idx[k]] = v[k].to_bits();
        }
    })
    .map_or(RO_E_CLOSED, |_| S_OK)
}

unsafe extern "win64" fn nine_set_insets(this: *mut Slot, v: f32) -> i32 {
    set_four(this, [3, 7, 5, 0], [v; 4])
}

/// SetInsets(left, top, right, bottom).
unsafe extern "win64" fn nine_set_insets_with(
    this: *mut Slot,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> i32 {
    set_four(this, [3, 7, 5, 0], [left, top, right, bottom])
}

unsafe extern "win64" fn nine_set_scales(this: *mut Slot, v: f32) -> i32 {
    set_four(this, [4, 8, 6, 1], [v; 4])
}

/// SetInsetScales(left, top, right, bottom).
unsafe extern "win64" fn nine_set_scales_with(
    this: *mut Slot,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> i32 {
    set_four(this, [4, 8, 6, 1], [left, top, right, bottom])
}

static NINE_GRID_VT: VTable<30> = VTable({
    let a: [*const (); 26] = props_table::<26>(&[
        prop_f32!(0),
        prop_f32!(1),
        prop_bool!(2),
        prop_f32!(3),
        prop_f32!(4),
        prop_f32!(5),
        prop_f32!(6),
        prop_obj!(0),
        prop_f32!(7),
        prop_f32!(8),
    ]);
    table::<30>(
        &a,
        &[
            nine_set_insets as *const (),
            nine_set_insets_with as *const (),
            nine_set_scales as *const (),
            nine_set_scales_with as *const (),
        ],
    )
});

// ---------------------------------------------------------------------
// DropShadow : 0 BlurRadius, 1 Color, 2-4 Offset, 5 Opacity,
// 6 SourcePolicy ; objs 0 = Mask
// ---------------------------------------------------------------------

static DROP_SHADOW_VT: VTable<16> = VTable(props_table::<16>(&[
    prop_f32!(0),
    prop_u32!(1),
    prop_obj!(0),
    prop_v3!(2),
    prop_f32!(5),
]));
static DROP_SHADOW2_VT: VTable<8> = VTable(props_table::<8>(&[prop_u32!(6)]));

// ---------------------------------------------------------------------
// Lumières. Cellule 23 : IsEnabled (ICompositionLight3).
// ---------------------------------------------------------------------

const ENABLED: usize = 23;

/// Targets (0) ou ExclusionsFromTargets (1) : collection créée à la
/// demande et gardée.
unsafe fn light_set(this: *mut Slot, which: usize, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let obj = owner(this);
    let set = with(this, |p| {
        if p.sets[which].is_null() {
            p.sets[which] = new_obj(
                Kind::VisualSet,
                (*obj).compositor,
                Data::Props(Props::new(&[])),
            );
        }
        p.sets[which]
    });
    match set {
        Some(s) => {
            obj_add_ref(s);
            *out = default_iface(s);
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn light_targets(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    light_set(this, 0, out)
}

unsafe extern "win64" fn light_exclusions(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    light_set(this, 1, out)
}

static LIGHT_VT: VTable<7> = VTable(table::<7>(&INSPECTABLE, &[light_targets as *const ()]));
static LIGHT2_VT: VTable<7> = VTable(table::<7>(&INSPECTABLE, &[light_exclusions as *const ()]));
static LIGHT3_VT: VTable<8> = VTable(props_table::<8>(&[prop_bool!(23)]));

// AmbientLight : 0 Color, 1 Intensity.
static AMBIENT_VT: VTable<8> = VTable(props_table::<8>(&[prop_u32!(0)]));
static AMBIENT2_VT: VTable<8> = VTable(props_table::<8>(&[prop_f32!(1)]));

// DistantLight : 0 Color, objs 0 CoordinateSpace, 1-3 Direction, 4 Intensity.
static DISTANT_VT: VTable<12> = VTable(props_table::<12>(&[
    prop_u32!(0),
    prop_obj!(0),
    prop_v3!(1),
]));
static DISTANT2_VT: VTable<8> = VTable(props_table::<8>(&[prop_f32!(4)]));

// PointLight : 0 Color, 1 ConstantAttenuation, objs 0 CoordinateSpace,
// 2 LinearAttenuation, 3-5 Offset, 6 QuadraticAttenuation, 7 Intensity,
// 8 MinAttenuationCutoff, 9 MaxAttenuationCutoff.
static POINT_VT: VTable<18> = VTable(props_table::<18>(&[
    prop_u32!(0),
    prop_f32!(1),
    prop_obj!(0),
    prop_f32!(2),
    prop_v3!(3),
    prop_f32!(6),
]));
static POINT2_VT: VTable<8> = VTable(props_table::<8>(&[prop_f32!(7)]));
static POINT3_VT: VTable<10> = VTable(props_table::<10>(&[prop_f32!(8), prop_f32!(9)]));

// SpotLight : 0 ConstantAttenuation, objs 0 CoordinateSpace, 1-3 Direction,
// 4 InnerConeAngle (radians), 5 InnerConeColor, 6 LinearAttenuation,
// 7-9 Offset, 10 OuterConeAngle (radians), 11 OuterConeColor,
// 12 QuadraticAttenuation, 13 InnerConeIntensity, 14 OuterConeIntensity,
// 15 MinAttenuationCutoff, 16 MaxAttenuationCutoff. Les angles en degrés
// sont la même valeur, convertie.

unsafe extern "win64" fn get_degrees<const I: usize>(this: *mut Slot, out: *mut f32) -> i32 {
    let r = get_f32::<I>(this, out);
    if r == S_OK {
        *out = (*out).to_degrees();
    }
    r
}

unsafe extern "win64" fn set_degrees<const I: usize>(this: *mut Slot, v: f32) -> i32 {
    set_f32::<I>(this, v.to_radians())
}

static SPOT_VT: VTable<30> = VTable(props_table::<30>(&[
    prop_f32!(0),
    prop_obj!(0),
    prop_v3!(1),
    prop_f32!(4),
    [get_degrees::<4> as *const (), set_degrees::<4> as *const ()],
    prop_u32!(5),
    prop_f32!(6),
    prop_v3!(7),
    prop_f32!(10),
    [
        get_degrees::<10> as *const (),
        set_degrees::<10> as *const (),
    ],
    prop_u32!(11),
    prop_f32!(12),
]));
static SPOT2_VT: VTable<10> = VTable(props_table::<10>(&[prop_f32!(13), prop_f32!(14)]));
static SPOT3_VT: VTable<10> = VTable(props_table::<10>(&[prop_f32!(15), prop_f32!(16)]));

// ---------------------------------------------------------------------
// StepEasingFunction : 0 FinalStep, 1 InitialStep,
// 2 IsFinalStepSingleFrame, 3 IsInitialStepSingleFrame, 4 StepCount
// ---------------------------------------------------------------------

static STEP_EASING_VT: VTable<16> = VTable(props_table::<16>(&[
    prop_u32!(0),
    prop_u32!(1),
    prop_bool!(2),
    prop_bool!(3),
    prop_u32!(4),
]));

// ---------------------------------------------------------------------
// Ensembles : VisualUnorderedCollection et CompositionAnimationGroup
// (Count, Add, Remove, RemoveAll), avec leur parcours.
// ---------------------------------------------------------------------

/// Même objet COM ? (comparaison des IUnknown).
unsafe fn same(a: *mut c_void, b: *mut c_void) -> bool {
    if a == b {
        return true;
    }
    if a.is_null() || b.is_null() {
        return false;
    }
    let (ua, ub) = (com_qi(a, &IID_IUNKNOWN), com_qi(b, &IID_IUNKNOWN));
    com_release(ua);
    com_release(ub);
    !ua.is_null() && ua == ub
}

unsafe extern "win64" fn set_count(this: *mut Slot, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| p.list.len() as i32) {
        Some(n) => {
            *out = n;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn set_add(this: *mut Slot, v: *mut c_void) -> i32 {
    if v.is_null() {
        return E_INVALIDARG;
    }
    let present = with(this, |p| p.list.iter().any(|&o| same(o, v)));
    match present {
        None => RO_E_CLOSED,
        Some(true) => S_OK,
        Some(false) => {
            com_add_ref(v);
            with(this, |p| p.list.push(v));
            S_OK
        }
    }
}

unsafe extern "win64" fn set_remove(this: *mut Slot, v: *mut c_void) -> i32 {
    let found = with(this, |p| {
        p.list
            .iter()
            .position(|&o| same(o, v))
            .map(|i| p.list.remove(i))
    });
    match found {
        None => RO_E_CLOSED,
        Some(Some(o)) => {
            com_release(o);
            S_OK
        }
        Some(None) => S_OK,
    }
}

unsafe extern "win64" fn set_remove_all(this: *mut Slot) -> i32 {
    match with(this, |p| std::mem::take(&mut p.list)) {
        Some(items) => {
            for o in items {
                com_release(o);
            }
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

static SET_VT: VTable<10> = VTable(table::<10>(
    &INSPECTABLE,
    &[
        set_count as *const (),
        set_add as *const (),
        set_remove as *const (),
        set_remove_all as *const (),
    ],
));

/// Parcours figé d'éléments tenus.
unsafe fn new_iterator(items: Vec<*mut c_void>, iterator_iid: Guid) -> *mut Obj {
    for &o in &items {
        com_add_ref(o);
    }
    let mut p = Props::new(&[]);
    p.list = items;
    new_obj_ifaces(
        Kind::Iterator,
        std::ptr::null_mut(),
        Data::Props(p),
        vec![(iterator_iid, ITERATOR_VT.ptr())],
    )
}

unsafe extern "win64" fn set_first(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let obj = owner(this);
    let g = generic();
    let iid = if (*obj).kind == Kind::AnimationGroup {
        g.iterator_animation
    } else {
        g.iterator_visual
    };
    match with(this, |p| p.list.clone()) {
        Some(items) => put_obj(out, new_iterator(items, iid)),
        None => RO_E_CLOSED,
    }
}

static ITERABLE_VT: VTable<7> = VTable(table::<7>(&INSPECTABLE, &[set_first as *const ()]));

unsafe extern "win64" fn iter_current(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| p.list.get(p.pos).copied()) {
        Some(Some(o)) => put_com(out, o),
        Some(None) => {
            *out = std::ptr::null_mut();
            E_BOUNDS
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn iter_has_current(this: *mut Slot, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| p.pos < p.list.len()) {
        Some(b) => {
            *out = b as u8;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn iter_move_next(this: *mut Slot, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| {
        if p.pos < p.list.len() {
            p.pos += 1;
        }
        p.pos < p.list.len()
    }) {
        Some(b) => {
            *out = b as u8;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn iter_get_many(
    this: *mut Slot,
    capacity: u32,
    items: *mut *mut c_void,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (items.is_null() && capacity > 0) {
        return E_POINTER;
    }
    let got = with(this, |p| {
        let n = (p.list.len() - p.pos).min(capacity as usize);
        let v: Vec<_> = p.list[p.pos..p.pos + n].to_vec();
        p.pos += n;
        v
    });
    match got {
        Some(v) => {
            for (k, o) in v.iter().enumerate() {
                com_add_ref(*o);
                *items.add(k) = *o;
            }
            *actual = v.len() as u32;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

static ITERATOR_VT: VTable<10> = VTable(table::<10>(
    &INSPECTABLE,
    &[
        iter_current as *const (),
        iter_has_current as *const (),
        iter_move_next as *const (),
        iter_get_many as *const (),
    ],
));

// ---------------------------------------------------------------------
// ImplicitAnimationCollection : IMap<String, ICompositionAnimationBase>,
// sa vue figée (IMapView) et ses paires (IKeyValuePair).
// ---------------------------------------------------------------------

unsafe fn lookup_in(this: *mut Slot, key: usize, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let k = hstring(key);
    match with(this, |p| {
        p.map.iter().find(|(n, _)| *n == k).map(|(_, v)| *v)
    }) {
        Some(Some(v)) => put_com(out, v),
        Some(None) => E_BOUNDS,
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn map_lookup(this: *mut Slot, key: usize, out: *mut *mut c_void) -> i32 {
    lookup_in(this, key, out)
}

unsafe extern "win64" fn map_size(this: *mut Slot, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match with(this, |p| p.map.len() as u32) {
        Some(n) => {
            *out = n;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

unsafe extern "win64" fn map_has_key(this: *mut Slot, key: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let k = hstring(key);
    match with(this, |p| p.map.iter().any(|(n, _)| *n == k)) {
        Some(b) => {
            *out = b as u8;
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

/// Copie tenue de la table, pour une vue ou un parcours.
unsafe fn snapshot(this: *mut Slot) -> Option<Vec<(String, *mut c_void)>> {
    with(this, |p| {
        for (_, v) in &p.map {
            com_add_ref(*v);
        }
        p.map.clone()
    })
}

unsafe extern "win64" fn map_get_view(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let Some(map) = snapshot(this) else {
        return RO_E_CLOSED;
    };
    let mut p = Props::new(&[]);
    p.map = map;
    let g = generic();
    put_obj(
        out,
        new_obj_ifaces(
            Kind::MapView,
            std::ptr::null_mut(),
            Data::Props(p),
            vec![
                (g.map_view, MAP_VIEW_VT.ptr()),
                (g.iterable_pair, PAIRS_VT.ptr()),
            ],
        ),
    )
}

unsafe extern "win64" fn map_insert(
    this: *mut Slot,
    key: usize,
    value: *mut c_void,
    replaced: *mut u8,
) -> i32 {
    if replaced.is_null() {
        return E_POINTER;
    }
    let k = hstring(key);
    com_add_ref(value);
    let old = with(this, |p| match p.map.iter_mut().find(|(n, _)| *n == k) {
        Some(e) => Some(std::mem::replace(&mut e.1, value)),
        None => {
            p.map.push((k, value));
            None
        }
    });
    match old {
        None => {
            com_release(value);
            RO_E_CLOSED
        }
        Some(o) => {
            *replaced = o.is_some() as u8;
            if let Some(o) = o {
                com_release(o);
            }
            S_OK
        }
    }
}

unsafe extern "win64" fn map_remove(this: *mut Slot, key: usize) -> i32 {
    let k = hstring(key);
    let old = with(this, |p| {
        p.map
            .iter()
            .position(|(n, _)| *n == k)
            .map(|i| p.map.remove(i).1)
    });
    match old {
        None => RO_E_CLOSED,
        Some(None) => E_BOUNDS,
        Some(Some(o)) => {
            com_release(o);
            S_OK
        }
    }
}

unsafe extern "win64" fn map_clear(this: *mut Slot) -> i32 {
    match with(this, |p| std::mem::take(&mut p.map)) {
        Some(m) => {
            for (_, o) in m {
                com_release(o);
            }
            S_OK
        }
        None => RO_E_CLOSED,
    }
}

static MAP_VT: VTable<13> = VTable(table::<13>(
    &INSPECTABLE,
    &[
        map_lookup as *const (),
        map_size as *const (),
        map_has_key as *const (),
        map_get_view as *const (),
        map_insert as *const (),
        map_remove as *const (),
        map_clear as *const (),
    ],
));

/// Split : la documentation permet deux vues nulles quand la vue ne se
/// partage pas.
unsafe extern "win64" fn view_split(
    _: *mut Slot,
    first: *mut *mut c_void,
    second: *mut *mut c_void,
) -> i32 {
    if first.is_null() || second.is_null() {
        return E_POINTER;
    }
    *first = std::ptr::null_mut();
    *second = std::ptr::null_mut();
    S_OK
}

static MAP_VIEW_VT: VTable<10> = VTable(table::<10>(
    &INSPECTABLE,
    &[
        map_lookup as *const (),
        map_size as *const (),
        map_has_key as *const (),
        view_split as *const (),
    ],
));

/// IIterable<IKeyValuePair<String, ICompositionAnimationBase>>::First.
unsafe extern "win64" fn pairs_first(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let Some(map) = snapshot(this) else {
        return RO_E_CLOSED;
    };
    let g = generic();
    let pairs: Vec<*mut c_void> = map
        .into_iter()
        .map(|(k, v)| {
            let mut p = Props::new(&[]);
            p.key = k;
            p.objs[0] = v; // déjà tenue par snapshot
            default_iface(new_obj_ifaces(
                Kind::Pair,
                std::ptr::null_mut(),
                Data::Props(p),
                vec![(g.pair, PAIR_VT.ptr())],
            ))
        })
        .collect();
    let it = new_iterator(pairs.clone(), g.iterator_pair);
    for p in pairs {
        com_release(p);
    }
    put_obj(out, it)
}

static PAIRS_VT: VTable<7> = VTable(table::<7>(&INSPECTABLE, &[pairs_first as *const ()]));

unsafe extern "win64" fn pair_key(this: *mut Slot, out: *mut usize) -> i32 {
    match with(this, |p| p.key.clone()) {
        Some(k) => create_hstring(&k, out),
        None => RO_E_CLOSED,
    }
}

static PAIR_VT: VTable<8> = VTable(table::<8>(
    &INSPECTABLE,
    &[pair_key as *const (), get_obj::<0> as *const ()],
));

// ---------------------------------------------------------------------
// Classes, interfaces et création
// ---------------------------------------------------------------------

pub(super) fn class(kind: Kind) -> &'static str {
    match kind {
        Kind::MaskBrush => "Windows.UI.Composition.CompositionMaskBrush",
        Kind::NineGridBrush => "Windows.UI.Composition.CompositionNineGridBrush",
        Kind::DropShadow => "Windows.UI.Composition.DropShadow",
        Kind::AmbientLight => "Windows.UI.Composition.AmbientLight",
        Kind::DistantLight => "Windows.UI.Composition.DistantLight",
        Kind::PointLight => "Windows.UI.Composition.PointLight",
        Kind::SpotLight => "Windows.UI.Composition.SpotLight",
        Kind::AnimationGroup => "Windows.UI.Composition.CompositionAnimationGroup",
        Kind::ImplicitAnimations => "Windows.UI.Composition.ImplicitAnimationCollection",
        Kind::StepEasing => "Windows.UI.Composition.StepEasingFunction",
        Kind::VisualSet => "Windows.UI.Composition.VisualUnorderedCollection",
        Kind::Iterator => "Windows.Foundation.Collections.IIterator`1",
        Kind::MapView => "Windows.Foundation.Collections.IMapView`2",
        Kind::Pair => "Windows.Foundation.Collections.IKeyValuePair`2",
        _ => "",
    }
}

/// Interfaces d'un objet de ce module (la première par défaut), sans celles
/// de CompositionObject que `Kind::ifaces` ajoute.
pub(super) fn ifaces(kind: Kind) -> Vec<(Guid, *const ())> {
    let lights = |own: &[(Guid, *const ())]| {
        let mut v = own.to_vec();
        v.extend_from_slice(&[
            (IID_LIGHT, LIGHT_VT.ptr()),
            (IID_LIGHT2, LIGHT2_VT.ptr()),
            (IID_LIGHT3, LIGHT3_VT.ptr()),
        ]);
        v
    };
    let g = generic();
    match kind {
        Kind::MaskBrush => vec![
            (IID_MASK_BRUSH, MASK_BRUSH_VT.ptr()),
            (IID_BRUSH, EMPTY_VT.ptr()),
        ],
        Kind::NineGridBrush => vec![
            (IID_NINE_GRID_BRUSH, NINE_GRID_VT.ptr()),
            (IID_BRUSH, EMPTY_VT.ptr()),
        ],
        Kind::DropShadow => vec![
            (IID_DROP_SHADOW, DROP_SHADOW_VT.ptr()),
            (IID_DROP_SHADOW2, DROP_SHADOW2_VT.ptr()),
            (IID_SHADOW, EMPTY_VT.ptr()),
        ],
        Kind::AmbientLight => lights(&[
            (IID_AMBIENT, AMBIENT_VT.ptr()),
            (IID_AMBIENT2, AMBIENT2_VT.ptr()),
        ]),
        Kind::DistantLight => lights(&[
            (IID_DISTANT, DISTANT_VT.ptr()),
            (IID_DISTANT2, DISTANT2_VT.ptr()),
        ]),
        Kind::PointLight => lights(&[
            (IID_POINT, POINT_VT.ptr()),
            (IID_POINT2, POINT2_VT.ptr()),
            (IID_POINT3, POINT3_VT.ptr()),
        ]),
        Kind::SpotLight => lights(&[
            (IID_SPOT, SPOT_VT.ptr()),
            (IID_SPOT2, SPOT2_VT.ptr()),
            (IID_SPOT3, SPOT3_VT.ptr()),
        ]),
        Kind::AnimationGroup => vec![
            (IID_ANIMATION_GROUP, SET_VT.ptr()),
            (IID_ANIMATION_BASE, EMPTY_VT.ptr()),
            (g.iterable_animation, ITERABLE_VT.ptr()),
        ],
        Kind::ImplicitAnimations => vec![
            (IID_IMPLICIT, EMPTY_VT.ptr()),
            (IID_ANIMATION_BASE, EMPTY_VT.ptr()),
            (g.map, MAP_VT.ptr()),
            (g.iterable_pair, PAIRS_VT.ptr()),
        ],
        Kind::StepEasing => vec![
            (IID_STEP_EASING, STEP_EASING_VT.ptr()),
            (IID_EASING, EMPTY_VT.ptr()),
        ],
        Kind::VisualSet => vec![
            (IID_VISUAL_SET, SET_VT.ptr()),
            (g.iterable_visual, ITERABLE_VT.ptr()),
        ],
        _ => Vec::new(),
    }
}

/// Tables de ce module (pour reconnaître nos objets).
pub(super) fn is_ours(vt: *const ()) -> bool {
    [
        MASK_BRUSH_VT.ptr(),
        NINE_GRID_VT.ptr(),
        DROP_SHADOW_VT.ptr(),
        DROP_SHADOW2_VT.ptr(),
        LIGHT_VT.ptr(),
        LIGHT2_VT.ptr(),
        LIGHT3_VT.ptr(),
        AMBIENT_VT.ptr(),
        AMBIENT2_VT.ptr(),
        DISTANT_VT.ptr(),
        DISTANT2_VT.ptr(),
        POINT_VT.ptr(),
        POINT2_VT.ptr(),
        POINT3_VT.ptr(),
        SPOT_VT.ptr(),
        SPOT2_VT.ptr(),
        SPOT3_VT.ptr(),
        STEP_EASING_VT.ptr(),
        SET_VT.ptr(),
        ITERABLE_VT.ptr(),
        ITERATOR_VT.ptr(),
        MAP_VT.ptr(),
        MAP_VIEW_VT.ptr(),
        PAIRS_VT.ptr(),
        PAIR_VT.ptr(),
    ]
    .contains(&vt)
}

/// Crée un objet et note une fois au journal ce que le rendu n'applique pas.
unsafe fn create(
    this: *mut Slot,
    kind: Kind,
    cells: &[(usize, u32)],
    out: *mut *mut c_void,
) -> i32 {
    static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let name = class(kind);
    {
        let mut seen = lock(&SEEN);
        if !seen.contains(&name) {
            seen.push(name);
            crate::log_warn!(
                "composition",
                "{name} : propriétés gardées, non appliquées au dessin (#2715)"
            );
        }
    }
    put_obj(
        out,
        new_obj(kind, owner(this), Data::Props(Props::new(cells))),
    )
}

pub(super) unsafe extern "win64" fn create_ambient(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    create(
        this,
        Kind::AmbientLight,
        &[(0, WHITE), (1, ONE), (ENABLED, 1)],
        out,
    )
}

pub(super) unsafe extern "win64" fn create_group(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    create(this, Kind::AnimationGroup, &[], out)
}

pub(super) unsafe extern "win64" fn create_distant(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    create(
        this,
        Kind::DistantLight,
        &[(0, WHITE), (3, (-1.0f32).to_bits()), (4, ONE), (ENABLED, 1)],
        out,
    )
}

pub(super) unsafe extern "win64" fn create_drop_shadow(
    this: *mut Slot,
    out: *mut *mut c_void,
) -> i32 {
    create(
        this,
        Kind::DropShadow,
        &[(0, 9.0f32.to_bits()), (1, BLACK), (5, ONE)],
        out,
    )
}

pub(super) unsafe extern "win64" fn create_implicit(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    create(this, Kind::ImplicitAnimations, &[], out)
}

pub(super) unsafe extern "win64" fn create_mask_brush(
    this: *mut Slot,
    out: *mut *mut c_void,
) -> i32 {
    create(this, Kind::MaskBrush, &[], out)
}

pub(super) unsafe extern "win64" fn create_nine_grid(
    this: *mut Slot,
    out: *mut *mut c_void,
) -> i32 {
    create(
        this,
        Kind::NineGridBrush,
        &[(1, ONE), (4, ONE), (6, ONE), (8, ONE)],
        out,
    )
}

pub(super) unsafe extern "win64" fn create_point(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    create(
        this,
        Kind::PointLight,
        &[(0, WHITE), (1, ONE), (7, ONE), (ENABLED, 1)],
        out,
    )
}

pub(super) unsafe extern "win64" fn create_spot(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    create(
        this,
        Kind::SpotLight,
        &[
            (0, ONE),
            (3, (-1.0f32).to_bits()),
            (5, WHITE),
            (11, WHITE),
            (13, ONE),
            (14, ONE),
            (ENABLED, 1),
        ],
        out,
    )
}

pub(super) unsafe extern "win64" fn create_step(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    create_step_count(this, 1, out)
}

pub(super) unsafe extern "win64" fn create_step_count(
    this: *mut Slot,
    steps: i32,
    out: *mut *mut c_void,
) -> i32 {
    create(
        this,
        Kind::StepEasing,
        &[(0, steps as u32), (4, steps as u32)],
        out,
    )
}

#[cfg(test)]
mod tests {
    use super::super::super::comobj::E_NOTIMPL;
    use super::*;

    type Out = unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32;

    unsafe fn qi_ok(p: *mut c_void, iid: &Guid) -> *mut c_void {
        let q = com_qi(p, iid);
        assert!(
            !q.is_null(),
            "QI {}",
            super::super::super::comobj::guid_str(iid)
        );
        q
    }

    unsafe fn out0(p: *mut c_void, slot: usize) -> *mut c_void {
        let f: Out = vcall(p, slot);
        let mut o = std::ptr::null_mut();
        assert_eq!(f(p, &mut o), S_OK, "emplacement {slot}");
        assert!(!o.is_null());
        o
    }

    unsafe fn f32_rt(p: *mut c_void, get: usize, v: f32) -> f32 {
        let set: unsafe extern "win64" fn(*mut c_void, f32) -> i32 = vcall(p, get + 1);
        let g: unsafe extern "win64" fn(*mut c_void, *mut f32) -> i32 = vcall(p, get);
        assert_eq!(set(p, v), S_OK);
        let mut r = f32::NAN;
        assert_eq!(g(p, &mut r), S_OK);
        r
    }

    unsafe fn f32_get(p: *mut c_void, get: usize) -> f32 {
        let g: unsafe extern "win64" fn(*mut c_void, *mut f32) -> i32 = vcall(p, get);
        let mut r = f32::NAN;
        assert_eq!(g(p, &mut r), S_OK);
        r
    }

    /// #2715 : chaque création de ICompositor2 rend un objet de sa classe,
    /// avec ses interfaces ; les propriétés sont gardées et relues.
    #[test]
    fn classes_du_compositeur2_2715() {
        unsafe {
            let comp = default_iface(new_compositor());
            let c2 = qi_ok(comp, &IID_COMPOSITOR2);
            // Plus aucune création ne rend E_NOTIMPL.
            for slot in 6..18 {
                let f: Out = vcall(c2, slot);
                let mut o = std::ptr::null_mut();
                let hr = f(c2, &mut o);
                assert_ne!(hr, E_NOTIMPL, "emplacement {slot}");
                assert_eq!(hr, S_OK, "emplacement {slot}");
                com_release(o);
            }

            // CompositionMaskBrush : Mask et Source.
            let mask = out0(c2, 13);
            assert_eq!(ours(mask).map(|o| (*o).kind), Some(Kind::MaskBrush));
            com_release(qi_ok(mask, &IID_BRUSH));
            let color = out0(comp, 7);
            let set: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 = vcall(mask, 9);
            assert_eq!(set(mask, color), S_OK);
            let src = out0(mask, 8);
            assert_eq!(src, color);
            com_release(src);
            let m = {
                let f: Out = vcall(mask, 6);
                let mut o = 1 as *mut c_void;
                assert_eq!(f(mask, &mut o), S_OK);
                o
            };
            assert!(m.is_null());

            // CompositionNineGridBrush : échelles à 1 par défaut, SetInsets(l, t, r, b).
            let nine = out0(c2, 14);
            assert_eq!(f32_get(nine, 24), 1.0); // TopInsetScale
            let set4: unsafe extern "win64" fn(*mut c_void, f32, f32, f32, f32) -> i32 =
                vcall(nine, 27);
            assert_eq!(set4(nine, 1.0, 2.0, 3.0, 4.0), S_OK);
            assert_eq!(f32_get(nine, 12), 1.0); // Left
            assert_eq!(f32_get(nine, 22), 2.0); // Top
            assert_eq!(f32_get(nine, 16), 3.0); // Right
            assert_eq!(f32_get(nine, 6), 4.0); // Bottom
            let hollow: unsafe extern "win64" fn(*mut c_void, u8) -> i32 = vcall(nine, 11);
            assert_eq!(hollow(nine, 1), S_OK);
            let mut b = 0u8;
            let get_h: unsafe extern "win64" fn(*mut c_void, *mut u8) -> i32 = vcall(nine, 10);
            assert_eq!(get_h(nine, &mut b), S_OK);
            assert_eq!(b, 1);

            // DropShadow : rayon 9 par défaut, couleur noire, Offset en Vector3.
            let shadow = out0(c2, 10);
            assert_eq!(f32_get(shadow, 6), 9.0);
            let mut c = 0u32;
            let get_c: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 = vcall(shadow, 8);
            assert_eq!(get_c(shadow, &mut c), S_OK);
            assert_eq!(c, BLACK);
            let set_off: unsafe extern "win64" fn(*mut c_void, *const [f32; 3]) -> i32 =
                vcall(shadow, 13);
            let get_off: unsafe extern "win64" fn(*mut c_void, *mut [f32; 3]) -> i32 =
                vcall(shadow, 12);
            assert_eq!(set_off(shadow, &[1.0, 2.0, 3.0]), S_OK);
            let mut v = [0f32; 3];
            assert_eq!(get_off(shadow, &mut v), S_OK);
            assert_eq!(v, [1.0, 2.0, 3.0]);
            com_release(qi_ok(shadow, &IID_DROP_SHADOW2));
            com_release(qi_ok(shadow, &IID_SHADOW));

            // SpotLight : angles en degrés et en radians, cibles.
            let spot = out0(c2, 16);
            assert!((f32_rt(spot, 14, 90.0) - 90.0).abs() < 1e-4);
            assert!((f32_get(spot, 12) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
            let light = qi_ok(spot, &IID_LIGHT);
            let targets = out0(light, 6);
            let add: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 = vcall(targets, 7);
            let count: unsafe extern "win64" fn(*mut c_void, *mut i32) -> i32 = vcall(targets, 6);
            let visual = out0(comp, 9);
            assert_eq!(add(targets, visual), S_OK);
            assert_eq!(add(targets, visual), S_OK);
            let mut n = 0;
            assert_eq!(count(targets, &mut n), S_OK);
            assert_eq!(n, 1);
            let iterable = qi_ok(targets, &generic().iterable_visual);
            let it = out0(iterable, 6);
            com_release(qi_ok(it, &generic().iterator_visual));
            let cur = out0(it, 6);
            assert!(same(cur, visual));
            com_release(cur);
            let next: unsafe extern "win64" fn(*mut c_void, *mut u8) -> i32 = vcall(it, 8);
            let mut more = 1u8;
            assert_eq!(next(it, &mut more), S_OK);
            assert_eq!(more, 0);
            let l3 = qi_ok(spot, &IID_LIGHT3);
            let mut on = 0u8;
            let get_on: unsafe extern "win64" fn(*mut c_void, *mut u8) -> i32 = vcall(l3, 6);
            assert_eq!(get_on(l3, &mut on), S_OK);
            assert_eq!(on, 1);
            for p in [l3, it, iterable, targets, light, spot] {
                com_release(p);
            }

            // ImplicitAnimationCollection : table nom → animation, vue, paires.
            let implicit = out0(c2, 11);
            let map = qi_ok(implicit, &generic().map);
            let key: Vec<u16> = "Offset".encode_utf16().collect();
            let mut h = 0usize;
            super::super::super::electron_win32::WindowsCreateString(
                key.as_ptr(),
                key.len() as u32,
                &mut h,
            );
            let group = out0(c2, 7);
            let insert: unsafe extern "win64" fn(*mut c_void, usize, *mut c_void, *mut u8) -> i32 =
                vcall(map, 10);
            let mut replaced = 9u8;
            assert_eq!(insert(map, h, group, &mut replaced), S_OK);
            assert_eq!(replaced, 0);
            let lookup: unsafe extern "win64" fn(*mut c_void, usize, *mut *mut c_void) -> i32 =
                vcall(map, 6);
            let mut found = std::ptr::null_mut();
            assert_eq!(lookup(map, h, &mut found), S_OK);
            assert_eq!(found, group);
            com_release(found);
            let view = out0(map, 9);
            com_release(qi_ok(view, &generic().map_view));
            let pairs = qi_ok(view, &generic().iterable_pair);
            let it = out0(pairs, 6);
            let pair = out0(it, 6);
            com_release(qi_ok(pair, &generic().pair));
            let get_key: unsafe extern "win64" fn(*mut c_void, *mut usize) -> i32 = vcall(pair, 6);
            let mut k = 0usize;
            assert_eq!(get_key(pair, &mut k), S_OK);
            assert_eq!(hstring(k), "Offset");
            let value = out0(pair, 7);
            assert_eq!(value, group);
            com_release(value);
            let remove: unsafe extern "win64" fn(*mut c_void, usize) -> i32 = vcall(map, 11);
            assert_eq!(remove(map, h), S_OK);
            assert_eq!(remove(map, h), E_BOUNDS);
            for p in [pair, it, pairs, view, group, map, implicit] {
                com_release(p);
            }

            // StepEasingFunction : nombre de pas donné à la création.
            let with_count: unsafe extern "win64" fn(*mut c_void, i32, *mut *mut c_void) -> i32 =
                vcall(c2, 18);
            let mut step = std::ptr::null_mut();
            assert_eq!(with_count(c2, 4, &mut step), S_OK);
            let mut sc = 0u32;
            let get_sc: unsafe extern "win64" fn(*mut c_void, *mut u32) -> i32 = vcall(step, 14);
            assert_eq!(get_sc(step, &mut sc), S_OK);
            assert_eq!(sc, 4);
            com_release(qi_ok(step, &IID_EASING));

            for p in [step, visual, shadow, nine, color, mask, c2, comp] {
                com_release(p);
            }
        }
    }
}
