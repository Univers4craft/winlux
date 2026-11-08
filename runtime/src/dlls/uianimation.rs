//! Gestionnaire d'animations (« Windows Animation Manager », UIAnimation.dll)
//! écrit pour Winlux : UIAnimationManager, UIAnimationTimer et
//! UIAnimationTransitionLibrary et UIAnimationTransitionFactory (transitions
//! calculées par un interpolateur de l'application), interfaces de version 1
//! (#561, Paint.NET).
//!
//! L'ordre des méthodes suit `uianimation.h` (MinGW-w64, domaine public).
//! Les variables suivent les transitions de la bibliothèque (linéaire,
//! accélération/décélération, cubique, sinusoïdale, parabolique...) dans le
//! temps de la minuterie. Les valeurs avancent quand l'application appelle
//! IUIAnimationManager::Update (ou OnUpdate de son gestionnaire de minuterie),
//! et aussi quand elle lit une variable : la minuterie ne lance pas de fil à
//! elle, la lecture suffit à rendre la valeur de l'instant.

use super::audio_com::{
    guid, put, Com, Iface, VTable, E_INVALIDARG, E_POINTER, IID_IUNKNOWN, S_OK,
};
use super::ole32::Guid;
use std::cell::Cell;
use std::ffi::c_void;
use std::sync::Mutex;

const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const S_FALSE: i32 = 1;
/// UI_E_STORYBOARD_ACTIVE.
const UI_E_STORYBOARD_ACTIVE: i32 = 0x8002_0001u32 as i32;

const CLSID_MANAGER: Guid = guid(
    0x4c1fc63a,
    0x695c,
    0x47e8,
    [0xa3, 0x39, 0x1a, 0x19, 0x4b, 0xe3, 0xd0, 0xb8],
);
const CLSID_LIBRARY: Guid = guid(
    0x1d6322ad,
    0xaa85,
    0x4ef5,
    [0xa8, 0x28, 0x86, 0xd7, 0x10, 0x67, 0xd1, 0x45],
);
const CLSID_FACTORY: Guid = guid(
    0x8a9b1cdd,
    0xfcd7,
    0x419c,
    [0x8b, 0x44, 0x42, 0xfd, 0x17, 0xdb, 0x18, 0x87],
);
const IID_FACTORY: Guid = guid(
    0xfcd91e03,
    0x3e3b,
    0x45ad,
    [0xbb, 0xb1, 0x6d, 0xfc, 0x81, 0x53, 0x74, 0x3d],
);
const CLSID_TIMER: Guid = guid(
    0xbfcd4a0c,
    0x06b6,
    0x4384,
    [0xb7, 0x68, 0x0d, 0xaa, 0x79, 0x2c, 0x38, 0x0e],
);

const IID_MANAGER: Guid = guid(
    0x9169896c,
    0xac8d,
    0x4e7d,
    [0x94, 0xe5, 0x67, 0xfa, 0x4d, 0xc2, 0xf2, 0xe8],
);
const IID_VARIABLE: Guid = guid(
    0x8ceeb155,
    0x2849,
    0x4ce5,
    [0x94, 0x48, 0x91, 0xff, 0x70, 0xe1, 0xe4, 0xd9],
);
const IID_STORYBOARD: Guid = guid(
    0xa8ff128f,
    0x9bf9,
    0x4af1,
    [0x9e, 0x67, 0xe5, 0xe4, 0x10, 0xde, 0xfb, 0x84],
);
const IID_TRANSITION: Guid = guid(
    0xdc6ce252,
    0xf731,
    0x41cf,
    [0xb6, 0x10, 0x61, 0x4b, 0x6c, 0xa0, 0x49, 0xad],
);
const IID_LIBRARY: Guid = guid(
    0xca5a14b1,
    0xd24f,
    0x48b8,
    [0x8f, 0xe4, 0xc7, 0x81, 0x69, 0xba, 0x95, 0x4e],
);
const IID_TIMER: Guid = guid(
    0x6b0efad1,
    0xa053,
    0x41d6,
    [0x90, 0x85, 0x33, 0xa6, 0x89, 0x14, 0x46, 0x65],
);
const IID_TIMER_UPDATE: Guid = guid(
    0x195509b7,
    0x5d5e,
    0x4e3e,
    [0xb2, 0x78, 0xee, 0x37, 0x59, 0xb3, 0x67, 0xad],
);

// États publics (uianimation.h).
const SB_BUILDING: u32 = 0;
const SB_SCHEDULED: u32 = 1;
const SB_PLAYING: u32 = 3;
const SB_FINISHED: u32 = 5;
const SCHEDULING_SUCCEEDED: u32 = 3;
const SCHEDULING_ALREADY: u32 = 2;
const MANAGER_IDLE: u32 = 0;
const MANAGER_BUSY: u32 = 1;

/// UI_ANIMATION_KEYFRAME_STORYBOARD_START.
const KEYFRAME_START: usize = usize::MAX;

/// Horloge de la minuterie : secondes depuis le démarrage du processus.
fn now() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}

unsafe fn com_call(p: *mut c_void, slot: usize) -> u32 {
    let vt = *(p as *const *const usize);
    let f: unsafe extern "win64" fn(*mut c_void) -> u32 = std::mem::transmute(*vt.add(slot));
    f(p)
}
unsafe fn add_ref_raw(p: *mut c_void) {
    if !p.is_null() {
        com_call(p, 1);
    }
}
unsafe fn release_raw(p: *mut c_void) {
    if !p.is_null() {
        com_call(p, 2);
    }
}

unsafe fn qi_simple<T>(
    this: *mut Com<T>,
    riid: *const Guid,
    out: *mut *mut c_void,
    iids: &[Guid],
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(iid) = riid.as_ref() else {
        return E_INVALIDARG;
    };
    if *iid == IID_IUNKNOWN || iids.contains(iid) {
        Com::add_ref(this);
        *out = this.cast();
        return S_OK;
    }
    super::comobj::log_no_interface("uianimation", iid);
    E_NOINTERFACE
}

unsafe extern "win64" fn add_ref<T>(this: *mut Com<T>) -> u32 {
    Com::add_ref(this)
}
unsafe extern "win64" fn release<T>(this: *mut Com<T>) -> u32 {
    Com::release(this)
}

// ───────────────────────── Transitions ─────────────────────────

#[derive(Clone, Copy, Debug)]
enum Kind {
    Instantaneous {
        fin: f64,
    },
    Constant {
        d: f64,
    },
    Discrete {
        delay: f64,
        fin: f64,
        hold: f64,
    },
    Linear {
        d: f64,
        fin: f64,
    },
    LinearSpeed {
        speed: f64,
        fin: f64,
    },
    SinVelocity {
        d: f64,
        period: f64,
    },
    SinRange {
        d: f64,
        min: f64,
        max: f64,
        period: f64,
        slope: u32,
    },
    AccelDecel {
        d: f64,
        fin: f64,
        acc: f64,
        dec: f64,
    },
    Reversal {
        d: f64,
    },
    Cubic {
        d: f64,
        fin: f64,
        fin_vel: f64,
    },
    SmoothStop {
        max_d: f64,
        fin: f64,
    },
    Parabolic {
        fin: f64,
        fin_vel: f64,
        acc: f64,
    },
    /// IUIAnimationInterpolator de l'application. La référence appartient à
    /// la transition, que le storyboard garde tant que la courbe sert.
    Custom {
        interp: usize,
    },
}

/// Appels de l'interpolateur de l'application (ordre de `uianimation.h`) :
/// 3 SetInitialValueAndVelocity, 5 GetDuration, 6 GetFinalValue,
/// 7 InterpolateValue, 8 InterpolateVelocity.
unsafe fn interp_start(p: usize, v0: f64, u0: f64) {
    type F = unsafe extern "win64" fn(usize, f64, f64) -> i32;
    let vt = *(p as *const *const usize);
    let f: F = std::mem::transmute(*vt.add(3));
    f(p, v0, u0);
}
unsafe fn interp_get(p: usize, slot: usize) -> f64 {
    type F = unsafe extern "win64" fn(usize, *mut f64) -> i32;
    let vt = *(p as *const *const usize);
    let f: F = std::mem::transmute(*vt.add(slot));
    let mut v = 0.0;
    if f(p, &mut v) < 0 {
        return 0.0;
    }
    v
}
unsafe fn interp_at(p: usize, slot: usize, t: f64) -> f64 {
    type F = unsafe extern "win64" fn(usize, f64, *mut f64) -> i32;
    let vt = *(p as *const *const usize);
    let f: F = std::mem::transmute(*vt.add(slot));
    let mut v = 0.0;
    if f(p, t, &mut v) < 0 {
        return 0.0;
    }
    v
}

#[derive(Clone, Copy, Debug)]
struct Curve {
    kind: Kind,
    v0: f64,
    u0: f64,
}

/// Hermite cubique de (v0, u0) à (v1, u1) sur `d` secondes.
fn hermite(v0: f64, u0: f64, v1: f64, u1: f64, d: f64, t: f64) -> f64 {
    if d <= 0.0 {
        return v1;
    }
    let s = (t / d).clamp(0.0, 1.0);
    let (s2, s3) = (s * s, s * s * s);
    (2.0 * s3 - 3.0 * s2 + 1.0) * v0
        + (s3 - 2.0 * s2 + s) * d * u0
        + (-2.0 * s3 + 3.0 * s2) * v1
        + (s3 - s2) * d * u1
}

impl Curve {
    fn duration(&self) -> f64 {
        let (v0, u0) = (self.v0, self.u0);
        match self.kind {
            Kind::Instantaneous { .. } => 0.0,
            Kind::Constant { d } | Kind::SinVelocity { d, .. } | Kind::SinRange { d, .. } => d,
            Kind::Discrete { delay, hold, .. } => delay + hold,
            Kind::Linear { d, .. } | Kind::AccelDecel { d, .. } | Kind::Reversal { d } => d,
            Kind::Cubic { d, .. } => d,
            Kind::LinearSpeed { speed, fin } => {
                if speed > 0.0 {
                    (fin - v0).abs() / speed
                } else {
                    0.0
                }
            }
            Kind::SmoothStop { max_d, fin } => {
                let dist = fin - v0;
                if u0 != 0.0 && dist.signum() == u0.signum() {
                    (2.0 * dist / u0).min(max_d)
                } else {
                    max_d
                }
            }
            Kind::Custom { interp } => unsafe {
                interp_start(interp, v0, u0);
                interp_get(interp, 5).max(0.0)
            },
            Kind::Parabolic { fin, fin_vel, acc } => {
                // fin_vel² = u0² + 2·a·Δ : on suit la racine qui atteint fin.
                if acc == 0.0 {
                    return if u0 != 0.0 {
                        ((fin - v0) / u0).max(0.0)
                    } else {
                        0.0
                    };
                }
                let t = (fin_vel - u0) / acc;
                if t.is_finite() && t > 0.0 {
                    t
                } else {
                    0.0
                }
            }
        }
    }

    fn final_value(&self) -> f64 {
        match self.kind {
            Kind::Instantaneous { fin }
            | Kind::Discrete { fin, .. }
            | Kind::Linear { fin, .. }
            | Kind::LinearSpeed { fin, .. }
            | Kind::AccelDecel { fin, .. }
            | Kind::Cubic { fin, .. }
            | Kind::SmoothStop { fin, .. }
            | Kind::Parabolic { fin, .. } => fin,
            Kind::Custom { interp } => unsafe {
                interp_start(interp, self.v0, self.u0);
                interp_get(interp, 6)
            },
            _ => self.value(self.duration()),
        }
    }

    fn value(&self, t: f64) -> f64 {
        let d = self.duration();
        let t = t.clamp(0.0, d.max(0.0));
        let (v0, u0) = (self.v0, self.u0);
        match self.kind {
            Kind::Custom { interp } => unsafe {
                interp_start(interp, v0, u0);
                interp_at(interp, 7, t)
            },
            Kind::Instantaneous { fin } => fin,
            Kind::Constant { .. } => v0,
            Kind::Discrete { delay, fin, .. } => {
                if t < delay {
                    v0
                } else {
                    fin
                }
            }
            Kind::Linear { fin, .. } | Kind::LinearSpeed { fin, .. } => {
                if d <= 0.0 {
                    fin
                } else {
                    v0 + (fin - v0) * t / d
                }
            }
            Kind::SinVelocity { period, .. } => {
                if period <= 0.0 {
                    v0
                } else {
                    let w = std::f64::consts::TAU / period;
                    v0 + u0 / w * (w * t).sin()
                }
            }
            Kind::SinRange {
                min,
                max,
                period,
                slope,
                ..
            } => {
                let (mid, amp) = ((min + max) / 2.0, (max - min) / 2.0);
                if period <= 0.0 || amp == 0.0 {
                    return v0;
                }
                let mut phase = ((v0 - mid) / amp).clamp(-1.0, 1.0).asin();
                // UI_ANIMATION_SLOPE_DECREASING (1) : on part sur la pente descendante.
                if slope == 1 {
                    phase = std::f64::consts::PI - phase;
                }
                mid + amp * (std::f64::consts::TAU * t / period + phase).sin()
            }
            Kind::AccelDecel { fin, acc, dec, .. } => {
                if d <= 0.0 {
                    return fin;
                }
                let (ta, td) = (acc.clamp(0.0, 1.0) * d, dec.clamp(0.0, 1.0) * d);
                let cruise = d - ta / 2.0 - td / 2.0;
                if cruise <= 0.0 {
                    return v0 + (fin - v0) * t / d;
                }
                let vm = (fin - v0) / cruise;
                let x = if t < ta {
                    0.5 * vm * t * t / ta
                } else if t <= d - td {
                    0.5 * vm * ta + vm * (t - ta)
                } else {
                    let r = d - t;
                    (fin - v0) - 0.5 * vm * r * r / td.max(f64::EPSILON)
                };
                v0 + x
            }
            Kind::Reversal { .. } => {
                if d <= 0.0 {
                    v0
                } else {
                    v0 + u0 * t - u0 * t * t / d
                }
            }
            Kind::Cubic { fin, fin_vel, .. } => hermite(v0, u0, fin, fin_vel, d, t),
            Kind::SmoothStop { fin, .. } => hermite(v0, u0, fin, 0.0, d, t),
            Kind::Parabolic { acc, fin, .. } => {
                if d <= 0.0 {
                    fin
                } else {
                    v0 + u0 * t + 0.5 * acc * t * t
                }
            }
        }
    }

    fn velocity(&self, t: f64) -> f64 {
        if let Kind::Custom { interp } = self.kind {
            let t = t.clamp(0.0, self.duration().max(0.0));
            // SAFETY : la transition garde l'interpolateur en vie.
            return unsafe {
                interp_start(interp, self.v0, self.u0);
                interp_at(interp, 8, t)
            };
        }
        let h = 1e-4;
        (self.value(t + h) - self.value((t - h).max(0.0))) / (h + h.min(t))
    }
}

struct Transition {
    curve: Mutex<Curve>,
}
type TransitionObj = Com<Transition>;

impl Drop for Transition {
    fn drop(&mut self) {
        if let Kind::Custom { interp } = crate::dlls::lock(&self.curve).kind {
            // SAFETY : référence prise par la fabrique à la création.
            unsafe { release_raw(interp as *mut c_void) };
        }
    }
}

// ───────────────────────── Fabrique de transitions ─────────────────────────

struct Factory;
type FactoryObj = Com<Factory>;

unsafe extern "win64" fn fac_qi(
    this: *mut FactoryObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_simple(this, riid, out, &[IID_FACTORY])
}
unsafe extern "win64" fn fac_create(
    _: *mut FactoryObj,
    interp: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if interp.is_null() {
        return E_INVALIDARG;
    }
    add_ref_raw(interp);
    let hr = new_transition(
        Kind::Custom {
            interp: interp as usize,
        },
        out,
    );
    if hr < 0 {
        release_raw(interp);
    }
    hr
}
static FACTORY_VT: VTable<4> = VTable([
    fac_qi as *const (),
    add_ref::<Factory> as *const (),
    release::<Factory> as *const (),
    fac_create as *const (),
]);

unsafe fn new_transition(kind: Kind, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let t = Transition {
        curve: Mutex::new(Curve {
            kind,
            v0: 0.0,
            u0: 0.0,
        }),
    };
    put(out, Com::create(TRANSITION_VT.ptr(), t))
}

unsafe extern "win64" fn tr_qi(
    this: *mut TransitionObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_simple(this, riid, out, &[IID_TRANSITION])
}
unsafe extern "win64" fn tr_set_initial(this: *mut TransitionObj, v: f64) -> i32 {
    crate::dlls::lock(&Com::get(this).curve).v0 = v;
    S_OK
}
unsafe extern "win64" fn tr_set_velocity(this: *mut TransitionObj, u: f64) -> i32 {
    crate::dlls::lock(&Com::get(this).curve).u0 = u;
    S_OK
}
unsafe extern "win64" fn tr_duration_known(_: *mut TransitionObj) -> i32 {
    S_OK
}
unsafe extern "win64" fn tr_duration(this: *mut TransitionObj, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = crate::dlls::lock(&Com::get(this).curve).duration();
    S_OK
}

static TRANSITION_VT: VTable<7> = VTable([
    tr_qi as *const (),
    add_ref::<Transition> as *const (),
    release::<Transition> as *const (),
    tr_set_initial as *const (),
    tr_set_velocity as *const (),
    tr_duration_known as *const (),
    tr_duration as *const (),
]);

/// Courbe d'une transition de la bibliothèque Winlux (les transitions
/// d'autres fabriques sont refusées par AddTransition).
unsafe fn transition_kind(p: *mut c_void) -> Option<Kind> {
    if p.is_null() || *(p as *const *const ()) != TRANSITION_VT.ptr() {
        return None;
    }
    Some(crate::dlls::lock(&Com::get(p as *mut TransitionObj).curve).kind)
}

// ───────────────────────── Bibliothèque de transitions ─────────────────────────

struct Library;
type LibraryObj = Com<Library>;

unsafe extern "win64" fn lib_qi(
    this: *mut LibraryObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_simple(this, riid, out, &[IID_LIBRARY])
}
unsafe extern "win64" fn lib_instant(_: *mut LibraryObj, fin: f64, out: *mut *mut c_void) -> i32 {
    new_transition(Kind::Instantaneous { fin }, out)
}
unsafe extern "win64" fn lib_constant(_: *mut LibraryObj, d: f64, out: *mut *mut c_void) -> i32 {
    new_transition(Kind::Constant { d }, out)
}
unsafe extern "win64" fn lib_discrete(
    _: *mut LibraryObj,
    delay: f64,
    fin: f64,
    hold: f64,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(Kind::Discrete { delay, fin, hold }, out)
}
unsafe extern "win64" fn lib_linear(
    _: *mut LibraryObj,
    d: f64,
    fin: f64,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(Kind::Linear { d, fin }, out)
}
unsafe extern "win64" fn lib_linear_speed(
    _: *mut LibraryObj,
    speed: f64,
    fin: f64,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(Kind::LinearSpeed { speed, fin }, out)
}
unsafe extern "win64" fn lib_sin_velocity(
    _: *mut LibraryObj,
    d: f64,
    period: f64,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(Kind::SinVelocity { d, period }, out)
}
unsafe extern "win64" fn lib_sin_range(
    _: *mut LibraryObj,
    d: f64,
    min: f64,
    max: f64,
    period: f64,
    slope: u32,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(
        Kind::SinRange {
            d,
            min,
            max,
            period,
            slope,
        },
        out,
    )
}
unsafe extern "win64" fn lib_accel_decel(
    _: *mut LibraryObj,
    d: f64,
    fin: f64,
    acc: f64,
    dec: f64,
    out: *mut *mut c_void,
) -> i32 {
    if acc < 0.0 || dec < 0.0 || acc + dec > 1.0 {
        return E_INVALIDARG;
    }
    new_transition(Kind::AccelDecel { d, fin, acc, dec }, out)
}
unsafe extern "win64" fn lib_reversal(_: *mut LibraryObj, d: f64, out: *mut *mut c_void) -> i32 {
    new_transition(Kind::Reversal { d }, out)
}
unsafe extern "win64" fn lib_cubic(
    _: *mut LibraryObj,
    d: f64,
    fin: f64,
    fin_vel: f64,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(Kind::Cubic { d, fin, fin_vel }, out)
}
unsafe extern "win64" fn lib_smooth_stop(
    _: *mut LibraryObj,
    max_d: f64,
    fin: f64,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(Kind::SmoothStop { max_d, fin }, out)
}
unsafe extern "win64" fn lib_parabolic(
    _: *mut LibraryObj,
    fin: f64,
    fin_vel: f64,
    acc: f64,
    out: *mut *mut c_void,
) -> i32 {
    new_transition(Kind::Parabolic { fin, fin_vel, acc }, out)
}

static LIBRARY_VT: VTable<15> = VTable([
    lib_qi as *const (),
    add_ref::<Library> as *const (),
    release::<Library> as *const (),
    lib_instant as *const (),
    lib_constant as *const (),
    lib_discrete as *const (),
    lib_linear as *const (),
    lib_linear_speed as *const (),
    lib_sin_velocity as *const (),
    lib_sin_range as *const (),
    lib_accel_decel as *const (),
    lib_reversal as *const (),
    lib_cubic as *const (),
    lib_smooth_stop as *const (),
    lib_parabolic as *const (),
]);

// ───────────────────────── Variables ─────────────────────────

struct VarState {
    value: f64,
    previous: f64,
    final_value: f64,
    lower: f64,
    upper: f64,
    /// UI_ANIMATION_ROUNDING_MODE : au plus proche (0), plancher (1), plafond (2).
    rounding: u32,
    tag: (*mut c_void, u32),
    storyboard: *mut c_void,
    handler: *mut c_void,
    int_handler: *mut c_void,
}

struct Variable {
    manager: *mut ManagerObj,
    st: Mutex<VarState>,
}

impl Drop for Variable {
    fn drop(&mut self) {
        let st = self.st.get_mut().unwrap_or_else(|e| e.into_inner());
        unsafe {
            release_raw(st.tag.0);
            release_raw(st.handler);
            release_raw(st.int_handler);
            Com::release(self.manager);
        }
    }
}

type VariableObj = Com<Variable>;

impl VarState {
    fn clamp(&self, v: f64) -> f64 {
        v.max(self.lower).min(self.upper)
    }
    fn int(&self, v: f64) -> i32 {
        (match self.rounding {
            1 => v.floor(),
            2 => v.ceil(),
            _ => v.round(),
        }) as i32
    }
}

unsafe fn var(this: *mut VariableObj) -> std::sync::MutexGuard<'static, VarState> {
    crate::dlls::lock(&(*this).inner.st)
}

/// Lecture : le gestionnaire avance d'abord jusqu'à l'instant présent.
unsafe fn catch_up(this: *mut VariableObj) {
    thread_local!(static BUSY: Cell<bool> = const { Cell::new(false) });
    if BUSY.with(|b| b.replace(true)) {
        return;
    }
    let m = (*this).inner.manager;
    let mut res = 0;
    manager_update(m, now(), &mut res);
    BUSY.with(|b| b.set(false));
}

unsafe extern "win64" fn var_qi(
    this: *mut VariableObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_simple(this, riid, out, &[IID_VARIABLE])
}
unsafe extern "win64" fn var_value(this: *mut VariableObj, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    catch_up(this);
    *out = var(this).value;
    S_OK
}
unsafe extern "win64" fn var_final(this: *mut VariableObj, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = var(this).final_value;
    S_OK
}
unsafe extern "win64" fn var_previous(this: *mut VariableObj, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = var(this).previous;
    S_OK
}
unsafe extern "win64" fn var_int(this: *mut VariableObj, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    catch_up(this);
    let s = var(this);
    *out = s.int(s.value);
    S_OK
}
unsafe extern "win64" fn var_final_int(this: *mut VariableObj, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let s = var(this);
    *out = s.int(s.final_value);
    S_OK
}
unsafe extern "win64" fn var_previous_int(this: *mut VariableObj, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let s = var(this);
    *out = s.int(s.previous);
    S_OK
}
unsafe extern "win64" fn var_storyboard(this: *mut VariableObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let sb = var(this).storyboard;
    add_ref_raw(sb);
    *out = sb;
    if sb.is_null() {
        S_FALSE
    } else {
        S_OK
    }
}
unsafe extern "win64" fn var_lower(this: *mut VariableObj, b: f64) -> i32 {
    var(this).lower = b;
    S_OK
}
unsafe extern "win64" fn var_upper(this: *mut VariableObj, b: f64) -> i32 {
    var(this).upper = b;
    S_OK
}
unsafe extern "win64" fn var_rounding(this: *mut VariableObj, mode: u32) -> i32 {
    var(this).rounding = mode;
    S_OK
}
unsafe extern "win64" fn var_set_tag(this: *mut VariableObj, obj: *mut c_void, id: u32) -> i32 {
    add_ref_raw(obj);
    let old = std::mem::replace(&mut var(this).tag, (obj, id));
    release_raw(old.0);
    S_OK
}
unsafe extern "win64" fn var_get_tag(
    this: *mut VariableObj,
    obj: *mut *mut c_void,
    id: *mut u32,
) -> i32 {
    let (o, i) = var(this).tag;
    if !obj.is_null() {
        add_ref_raw(o);
        *obj = o;
    }
    if !id.is_null() {
        *id = i;
    }
    if o.is_null() && i == 0 {
        0x8002_000Cu32 as i32 // UI_E_VALUE_NOT_SET
    } else {
        S_OK
    }
}
unsafe extern "win64" fn var_handler(this: *mut VariableObj, h: *mut c_void) -> i32 {
    add_ref_raw(h);
    let old = std::mem::replace(&mut var(this).handler, h);
    release_raw(old);
    S_OK
}
unsafe extern "win64" fn var_int_handler(this: *mut VariableObj, h: *mut c_void) -> i32 {
    add_ref_raw(h);
    let old = std::mem::replace(&mut var(this).int_handler, h);
    release_raw(old);
    S_OK
}

static VARIABLE_VT: VTable<17> = VTable([
    var_qi as *const (),
    add_ref::<Variable> as *const (),
    release::<Variable> as *const (),
    var_value as *const (),
    var_final as *const (),
    var_previous as *const (),
    var_int as *const (),
    var_final_int as *const (),
    var_previous_int as *const (),
    var_storyboard as *const (),
    var_lower as *const (),
    var_upper as *const (),
    var_rounding as *const (),
    var_set_tag as *const (),
    var_get_tag as *const (),
    var_handler as *const (),
    var_int_handler as *const (),
]);

unsafe fn as_variable(p: *mut c_void) -> Option<*mut VariableObj> {
    (!p.is_null() && *(p as *const *const ()) == VARIABLE_VT.ptr()).then_some(p.cast())
}

// ───────────────────────── Storyboards ─────────────────────────

/// Une transition placée : variable, courbe (valeurs initiales fixées à
/// Schedule), départ en secondes depuis le début du storyboard.
struct Placed {
    variable: *mut VariableObj,
    transition: *mut c_void,
    curve: Curve,
    start: f64,
}

struct SbState {
    placed: Vec<Placed>,
    keyframes: Vec<f64>,
    status: u32,
    start_time: f64,
    tag: (*mut c_void, u32),
    handler: *mut c_void,
}

struct Storyboard {
    manager: *mut ManagerObj,
    st: Mutex<SbState>,
}

impl Drop for Storyboard {
    fn drop(&mut self) {
        let st = self.st.get_mut().unwrap_or_else(|e| e.into_inner());
        unsafe {
            for p in st.placed.drain(..) {
                Com::release(p.variable);
                release_raw(p.transition);
            }
            release_raw(st.tag.0);
            release_raw(st.handler);
            Com::release(self.manager);
        }
    }
}

type StoryboardObj = Com<Storyboard>;

unsafe fn sb(this: *mut StoryboardObj) -> std::sync::MutexGuard<'static, SbState> {
    crate::dlls::lock(&(*this).inner.st)
}

impl SbState {
    /// Fin des transitions déjà posées sur `v` (0 sinon).
    fn end_of(&self, v: *mut VariableObj) -> f64 {
        self.placed
            .iter()
            .filter(|p| p.variable == v)
            .map(|p| p.start + p.curve.duration())
            .fold(0.0, f64::max)
    }
    fn keyframe_time(&self, k: usize) -> Option<f64> {
        if k == KEYFRAME_START {
            return Some(0.0);
        }
        self.keyframes.get(k.wrapping_sub(1)).copied()
    }
    fn total(&self) -> f64 {
        self.placed
            .iter()
            .map(|p| p.start + p.curve.duration())
            .fold(0.0, f64::max)
    }
}

unsafe fn place(
    this: *mut StoryboardObj,
    v: *mut c_void,
    t: *mut c_void,
    start: Option<f64>,
) -> i32 {
    let (Some(v), Some(kind)) = (as_variable(v), transition_kind(t)) else {
        return E_INVALIDARG;
    };
    let mut s = sb(this);
    if s.status != SB_BUILDING {
        return UI_E_STORYBOARD_ACTIVE;
    }
    let start = start.unwrap_or_else(|| s.end_of(v));
    Com::add_ref(v);
    add_ref_raw(t);
    s.placed.push(Placed {
        variable: v,
        transition: t,
        curve: Curve {
            kind,
            v0: 0.0,
            u0: 0.0,
        },
        start,
    });
    S_OK
}

unsafe extern "win64" fn sb_qi(
    this: *mut StoryboardObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_simple(this, riid, out, &[IID_STORYBOARD])
}
unsafe extern "win64" fn sb_add(this: *mut StoryboardObj, v: *mut c_void, t: *mut c_void) -> i32 {
    place(this, v, t, None)
}
unsafe extern "win64" fn sb_keyframe_offset(
    this: *mut StoryboardObj,
    base: usize,
    offset: f64,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut s = sb(this);
    let Some(t0) = s.keyframe_time(base) else {
        return E_INVALIDARG;
    };
    s.keyframes.push(t0 + offset.max(0.0));
    *out = s.keyframes.len();
    S_OK
}
unsafe extern "win64" fn sb_keyframe_after(
    this: *mut StoryboardObj,
    t: *mut c_void,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut s = sb(this);
    let Some(end) = s
        .placed
        .iter()
        .find(|p| p.transition == t)
        .map(|p| p.start + p.curve.duration())
    else {
        return E_INVALIDARG;
    };
    s.keyframes.push(end);
    *out = s.keyframes.len();
    S_OK
}
unsafe extern "win64" fn sb_add_at(
    this: *mut StoryboardObj,
    v: *mut c_void,
    t: *mut c_void,
    k: usize,
) -> i32 {
    let Some(start) = sb(this).keyframe_time(k) else {
        return E_INVALIDARG;
    };
    place(this, v, t, Some(start))
}
unsafe extern "win64" fn sb_add_between(
    this: *mut StoryboardObj,
    v: *mut c_void,
    t: *mut c_void,
    k1: usize,
    _k2: usize,
) -> i32 {
    sb_add_at(this, v, t, k1)
}
unsafe extern "win64" fn sb_repeat(_: *mut StoryboardObj, _: usize, _: usize, _: i32) -> i32 {
    // Les répétitions sont jouées une seule fois.
    S_OK
}
unsafe extern "win64" fn sb_hold(_: *mut StoryboardObj, _: *mut c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn sb_delay(_: *mut StoryboardObj, _: f64) -> i32 {
    S_OK
}

/// Schedule : fixe la valeur et la vitesse de départ de chaque transition,
/// puis confie le storyboard au gestionnaire.
unsafe extern "win64" fn sb_schedule(this: *mut StoryboardObj, time: f64, result: *mut u32) -> i32 {
    {
        let mut s = sb(this);
        if s.status != SB_BUILDING {
            if !result.is_null() {
                *result = SCHEDULING_ALREADY;
            }
            return S_OK;
        }
        s.placed.sort_by(|a, b| a.start.total_cmp(&b.start));
        let vars: Vec<*mut VariableObj> = {
            let mut v: Vec<_> = s.placed.iter().map(|p| p.variable).collect();
            v.dedup();
            v
        };
        for v in vars {
            let (mut val, mut vel) = {
                let st = var(v);
                (st.value, 0.0)
            };
            let mut fin = val;
            for p in s.placed.iter_mut().filter(|p| p.variable == v) {
                p.curve.v0 = val;
                p.curve.u0 = vel;
                let d = p.curve.duration();
                val = p.curve.value(d);
                vel = p.curve.velocity(d);
                fin = p.curve.final_value();
            }
            let mut st = var(v);
            st.final_value = st.clamp(fin);
            let old = std::mem::replace(&mut st.storyboard, this.cast());
            drop(st);
            if old.is_null() {
                // Lien non tenu : le storyboard vit tant qu'il est programmé.
            }
        }
        s.status = SB_SCHEDULED;
        s.start_time = time;
    }
    let m = (*this).inner.manager;
    Com::add_ref(this);
    let was_idle = {
        let mut ms = mgr(m);
        let idle = ms.scheduled.is_empty();
        ms.scheduled.push(this);
        idle
    };
    if was_idle {
        notify_manager(m, MANAGER_BUSY, MANAGER_IDLE);
    }
    if !result.is_null() {
        *result = SCHEDULING_SUCCEEDED;
    }
    let mut r = 0;
    manager_update(m, time, &mut r);
    S_OK
}
unsafe extern "win64" fn sb_conclude(this: *mut StoryboardObj) -> i32 {
    sb_finish(this, 0.0)
}
unsafe extern "win64" fn sb_finish(this: *mut StoryboardObj, _deadline: f64) -> i32 {
    // Le storyboard se termine tout de suite à ses valeurs finales.
    sb(this).start_time = f64::NEG_INFINITY;
    let m = (*this).inner.manager;
    let mut r = 0;
    manager_update(m, now(), &mut r);
    S_OK
}
unsafe extern "win64" fn sb_abandon(this: *mut StoryboardObj) -> i32 {
    let m = (*this).inner.manager;
    let removed = {
        let mut ms = mgr(m);
        let before = ms.scheduled.len();
        ms.scheduled.retain(|&p| p != this);
        before != ms.scheduled.len()
    };
    sb(this).status = 2; // CANCELLED
    if removed {
        Com::release(this);
    }
    S_OK
}
unsafe extern "win64" fn sb_set_tag(this: *mut StoryboardObj, obj: *mut c_void, id: u32) -> i32 {
    add_ref_raw(obj);
    let old = std::mem::replace(&mut sb(this).tag, (obj, id));
    release_raw(old.0);
    S_OK
}
unsafe extern "win64" fn sb_get_tag(
    this: *mut StoryboardObj,
    obj: *mut *mut c_void,
    id: *mut u32,
) -> i32 {
    let (o, i) = sb(this).tag;
    if !obj.is_null() {
        add_ref_raw(o);
        *obj = o;
    }
    if !id.is_null() {
        *id = i;
    }
    S_OK
}
unsafe extern "win64" fn sb_status(this: *mut StoryboardObj, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = sb(this).status;
    S_OK
}
unsafe extern "win64" fn sb_elapsed(this: *mut StoryboardObj, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let s = sb(this);
    *out = if s.status == SB_BUILDING {
        0.0
    } else {
        (now() - s.start_time).clamp(0.0, s.total())
    };
    S_OK
}
unsafe extern "win64" fn sb_handler(this: *mut StoryboardObj, h: *mut c_void) -> i32 {
    add_ref_raw(h);
    let old = std::mem::replace(&mut sb(this).handler, h);
    release_raw(old);
    S_OK
}

static STORYBOARD_VT: VTable<20> = VTable([
    sb_qi as *const (),
    add_ref::<Storyboard> as *const (),
    release::<Storyboard> as *const (),
    sb_add as *const (),
    sb_keyframe_offset as *const (),
    sb_keyframe_after as *const (),
    sb_add_at as *const (),
    sb_add_between as *const (),
    sb_repeat as *const (),
    sb_hold as *const (),
    sb_delay as *const (),
    sb_schedule as *const (),
    sb_conclude as *const (),
    sb_finish as *const (),
    sb_abandon as *const (),
    sb_set_tag as *const (),
    sb_get_tag as *const (),
    sb_status as *const (),
    sb_elapsed as *const (),
    sb_handler as *const (),
]);

// ───────────────────────── Gestionnaire ─────────────────────────

struct MgrState {
    scheduled: Vec<*mut StoryboardObj>,
    handler: *mut c_void,
    paused: bool,
    /// UI_ANIMATION_MODE : désactivé (0), système (1), activé (2).
    mode: u32,
}

#[repr(C)]
struct Manager {
    /// IUIAnimationTimerUpdateHandler, pour IUIAnimationTimer::SetTimerUpdateHandler.
    update: Iface,
    st: Mutex<MgrState>,
}

type ManagerObj = Com<Manager>;

unsafe fn mgr(m: *mut ManagerObj) -> std::sync::MutexGuard<'static, MgrState> {
    crate::dlls::lock(&(*m).inner.st)
}

unsafe fn notify_manager(m: *mut ManagerObj, new: u32, old: u32) {
    let h = mgr(m).handler;
    if !h.is_null() {
        add_ref_raw(h);
        type F = unsafe extern "win64" fn(*mut c_void, u32, u32) -> i32;
        let vt = *(h as *const *const usize);
        let f: F = std::mem::transmute(*vt.add(3));
        f(h, new, old);
        release_raw(h);
    }
}

/// Changement de valeur à annoncer une fois les verrous rendus.
struct Change {
    sb: *mut StoryboardObj,
    var: *mut VariableObj,
    new: f64,
    old: f64,
    new_int: i32,
    old_int: i32,
    handler: *mut c_void,
    int_handler: *mut c_void,
}

/// Avance tous les storyboards programmés jusqu'à `time`.
unsafe fn manager_update(m: *mut ManagerObj, time: f64, result: &mut u32) {
    let list: Vec<*mut StoryboardObj> = {
        let ms = mgr(m);
        if ms.paused {
            *result = 0;
            return;
        }
        ms.scheduled.clone()
    };
    let instant = mgr(m).mode == 0;
    let mut changes = Vec::new();
    let mut finished = Vec::new();
    let mut started = Vec::new();
    for &s in &list {
        let mut st = sb(s);
        let total = st.total();
        let elapsed = if instant { total } else { time - st.start_time };
        if st.status == SB_SCHEDULED && elapsed >= 0.0 {
            st.status = SB_PLAYING;
            started.push(s);
        }
        if st.status != SB_PLAYING {
            continue;
        }
        let mut vars: Vec<*mut VariableObj> = st.placed.iter().map(|p| p.variable).collect();
        vars.dedup();
        for v in vars {
            // La dernière transition déjà commencée donne la valeur.
            let cur = st
                .placed
                .iter()
                .rfind(|p| p.variable == v && p.start <= elapsed.max(0.0));
            let Some(p) = cur else { continue };
            let raw = p.curve.value(elapsed - p.start);
            let mut vs = var(v);
            let newv = vs.clamp(raw);
            if newv != vs.value {
                let old = vs.value;
                vs.previous = old;
                vs.value = newv;
                changes.push(Change {
                    sb: s,
                    var: v,
                    new: newv,
                    old,
                    new_int: vs.int(newv),
                    old_int: vs.int(old),
                    handler: vs.handler,
                    int_handler: vs.int_handler,
                });
                add_ref_raw(vs.handler);
                add_ref_raw(vs.int_handler);
            }
        }
        if elapsed >= total {
            st.status = SB_FINISHED;
            for p in &st.placed {
                let mut vs = var(p.variable);
                if vs.storyboard == s.cast() {
                    vs.storyboard = std::ptr::null_mut();
                }
            }
            finished.push(s);
        }
    }
    *result = (!changes.is_empty()) as u32;
    for &s in &started {
        sb_event(s, SB_PLAYING, SB_SCHEDULED);
    }
    for c in changes {
        type F = unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void, f64, f64) -> i32;
        type FI = unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void, i32, i32) -> i32;
        if !c.handler.is_null() {
            let vt = *(c.handler as *const *const usize);
            let f: F = std::mem::transmute(*vt.add(3));
            f(c.handler, c.sb.cast(), c.var.cast(), c.new, c.old);
            release_raw(c.handler);
        }
        if !c.int_handler.is_null() {
            if c.new_int != c.old_int {
                let vt = *(c.int_handler as *const *const usize);
                let f: FI = std::mem::transmute(*vt.add(3));
                f(
                    c.int_handler,
                    c.sb.cast(),
                    c.var.cast(),
                    c.new_int,
                    c.old_int,
                );
            }
            release_raw(c.int_handler);
        }
    }
    for &s in &list {
        sb_updated(s);
    }
    if !finished.is_empty() {
        let now_idle = {
            let mut ms = mgr(m);
            ms.scheduled.retain(|p| !finished.contains(p));
            ms.scheduled.is_empty()
        };
        for &s in &finished {
            sb_event(s, SB_FINISHED, SB_PLAYING);
            Com::release(s);
        }
        if now_idle {
            notify_manager(m, MANAGER_IDLE, MANAGER_BUSY);
        }
    }
}

unsafe fn sb_event(s: *mut StoryboardObj, new: u32, old: u32) {
    let h = sb(s).handler;
    if !h.is_null() {
        add_ref_raw(h);
        type F = unsafe extern "win64" fn(*mut c_void, *mut c_void, u32, u32) -> i32;
        let vt = *(h as *const *const usize);
        let f: F = std::mem::transmute(*vt.add(3));
        f(h, s.cast(), new, old);
        release_raw(h);
    }
}

unsafe fn sb_updated(s: *mut StoryboardObj) {
    let h = sb(s).handler;
    if !h.is_null() {
        add_ref_raw(h);
        type F = unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32;
        let vt = *(h as *const *const usize);
        let f: F = std::mem::transmute(*vt.add(4));
        f(h, s.cast());
        release_raw(h);
    }
}

unsafe extern "win64" fn mgr_qi(
    this: *mut ManagerObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() && riid.as_ref() == Some(&IID_TIMER_UPDATE) {
        Com::add_ref(this);
        *out = std::ptr::addr_of_mut!((*this).inner.update).cast();
        return S_OK;
    }
    qi_simple(this, riid, out, &[IID_MANAGER])
}
unsafe extern "win64" fn mgr_create_variable(
    this: *mut ManagerObj,
    initial: f64,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    Com::add_ref(this);
    let v = Variable {
        manager: this,
        st: Mutex::new(VarState {
            value: initial,
            previous: initial,
            final_value: initial,
            lower: f64::NEG_INFINITY,
            upper: f64::INFINITY,
            rounding: 0,
            tag: (std::ptr::null_mut(), 0),
            storyboard: std::ptr::null_mut(),
            handler: std::ptr::null_mut(),
            int_handler: std::ptr::null_mut(),
        }),
    };
    put(out, Com::create(VARIABLE_VT.ptr(), v))
}
unsafe fn new_storyboard(this: *mut ManagerObj) -> *mut StoryboardObj {
    Com::add_ref(this);
    Com::create(
        STORYBOARD_VT.ptr(),
        Storyboard {
            manager: this,
            st: Mutex::new(SbState {
                placed: Vec::new(),
                keyframes: Vec::new(),
                status: SB_BUILDING,
                start_time: 0.0,
                tag: (std::ptr::null_mut(), 0),
                handler: std::ptr::null_mut(),
            }),
        },
    )
}
unsafe extern "win64" fn mgr_schedule_transition(
    this: *mut ManagerObj,
    v: *mut c_void,
    t: *mut c_void,
    time: f64,
) -> i32 {
    let s = new_storyboard(this);
    let hr = place(s, v, t, None);
    if hr >= 0 {
        sb_schedule(s, time, std::ptr::null_mut());
    }
    Com::release(s);
    hr
}
unsafe extern "win64" fn mgr_create_storyboard(
    this: *mut ManagerObj,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put(out, new_storyboard(this))
}
unsafe extern "win64" fn mgr_finish_all(this: *mut ManagerObj, _deadline: f64) -> i32 {
    let list = mgr(this).scheduled.clone();
    for s in list {
        sb(s).start_time = f64::NEG_INFINITY;
    }
    let mut r = 0;
    manager_update(this, now(), &mut r);
    S_OK
}
unsafe extern "win64" fn mgr_abandon_all(this: *mut ManagerObj) -> i32 {
    let list = std::mem::take(&mut mgr(this).scheduled);
    let had = !list.is_empty();
    for s in list {
        sb(s).status = 2;
        Com::release(s);
    }
    if had {
        notify_manager(this, MANAGER_IDLE, MANAGER_BUSY);
    }
    S_OK
}
unsafe extern "win64" fn mgr_update(this: *mut ManagerObj, time: f64, result: *mut u32) -> i32 {
    let mut r = 0;
    manager_update(this, time, &mut r);
    if !result.is_null() {
        *result = r;
    }
    S_OK
}
unsafe extern "win64" fn mgr_from_tag(
    _: *mut ManagerObj,
    _: *mut c_void,
    _: u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    0x8002_000Cu32 as i32 // UI_E_VALUE_NOT_SET
}
unsafe extern "win64" fn mgr_status(this: *mut ManagerObj, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = if mgr(this).scheduled.is_empty() {
        MANAGER_IDLE
    } else {
        MANAGER_BUSY
    };
    S_OK
}
unsafe extern "win64" fn mgr_mode(this: *mut ManagerObj, mode: u32) -> i32 {
    mgr(this).mode = mode;
    S_OK
}
unsafe extern "win64" fn mgr_pause(this: *mut ManagerObj) -> i32 {
    mgr(this).paused = true;
    S_OK
}
unsafe extern "win64" fn mgr_resume(this: *mut ManagerObj) -> i32 {
    mgr(this).paused = false;
    S_OK
}
unsafe extern "win64" fn mgr_handler(this: *mut ManagerObj, h: *mut c_void) -> i32 {
    add_ref_raw(h);
    let old = std::mem::replace(&mut mgr(this).handler, h);
    release_raw(old);
    S_OK
}
unsafe extern "win64" fn mgr_comparison(_: *mut ManagerObj, _: *mut c_void) -> i32 {
    // Un seul storyboard à la fois par variable n'entre jamais en conflit
    // de priorité ici : la comparaison n'est jamais consultée.
    S_OK
}
unsafe extern "win64" fn mgr_delay(_: *mut ManagerObj, _: f64) -> i32 {
    S_OK
}
unsafe extern "win64" fn mgr_shutdown(this: *mut ManagerObj) -> i32 {
    mgr_abandon_all(this);
    let h = std::mem::replace(&mut mgr(this).handler, std::ptr::null_mut());
    release_raw(h);
    S_OK
}

static MANAGER_VT: VTable<22> = VTable([
    mgr_qi as *const (),
    add_ref::<Manager> as *const (),
    release::<Manager> as *const (),
    mgr_create_variable as *const (),
    mgr_schedule_transition as *const (),
    mgr_create_storyboard as *const (),
    mgr_finish_all as *const (),
    mgr_abandon_all as *const (),
    mgr_update as *const (),
    mgr_from_tag as *const (), // GetVariableFromTag
    mgr_from_tag as *const (), // GetStoryboardFromTag
    mgr_status as *const (),
    mgr_mode as *const (),
    mgr_pause as *const (),
    mgr_resume as *const (),
    mgr_handler as *const (),
    mgr_comparison as *const (), // Cancel
    mgr_comparison as *const (), // Trim
    mgr_comparison as *const (), // Compress
    mgr_comparison as *const (), // Conclude
    mgr_delay as *const (),
    mgr_shutdown as *const (),
]);

// IUIAnimationTimerUpdateHandler du gestionnaire.
unsafe fn upd_owner(this: *mut Iface) -> *mut ManagerObj {
    (*this).owner.cast()
}
unsafe extern "win64" fn upd_qi(this: *mut Iface, riid: *const Guid, out: *mut *mut c_void) -> i32 {
    mgr_qi(upd_owner(this), riid, out)
}
unsafe extern "win64" fn upd_add_ref(this: *mut Iface) -> u32 {
    Com::add_ref(upd_owner(this))
}
unsafe extern "win64" fn upd_release(this: *mut Iface) -> u32 {
    Com::release(upd_owner(this))
}
unsafe extern "win64" fn upd_on_update(this: *mut Iface, time: f64, result: *mut u32) -> i32 {
    mgr_update(upd_owner(this), time, result)
}
unsafe extern "win64" fn upd_client(_: *mut Iface, _: *mut c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn upd_clear(_: *mut Iface) -> i32 {
    S_OK
}

static UPDATE_VT: VTable<6> = VTable([
    upd_qi as *const (),
    upd_add_ref as *const (),
    upd_release as *const (),
    upd_on_update as *const (),
    upd_client as *const (),
    upd_clear as *const (),
]);

unsafe fn new_manager() -> *mut ManagerObj {
    let m = Com::create(
        MANAGER_VT.ptr(),
        Manager {
            update: Iface::new(UPDATE_VT.ptr()),
            st: Mutex::new(MgrState {
                scheduled: Vec::new(),
                handler: std::ptr::null_mut(),
                paused: false,
                mode: 1,
            }),
        },
    );
    (*m).inner.update.owner = m.cast();
    m
}

// ───────────────────────── Minuterie ─────────────────────────

struct Timer {
    handler: Mutex<*mut c_void>,
    enabled: Mutex<bool>,
}

impl Drop for Timer {
    fn drop(&mut self) {
        unsafe { release_raw(*self.handler.get_mut().unwrap_or_else(|e| e.into_inner())) };
    }
}

type TimerObj = Com<Timer>;

unsafe extern "win64" fn timer_qi(
    this: *mut TimerObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_simple(this, riid, out, &[IID_TIMER])
}
unsafe extern "win64" fn timer_set_update(this: *mut TimerObj, h: *mut c_void, _idle: u32) -> i32 {
    add_ref_raw(h);
    let old = std::mem::replace(&mut *crate::dlls::lock(&Com::get(this).handler), h);
    release_raw(old);
    S_OK
}
unsafe extern "win64" fn timer_set_events(_: *mut TimerObj, _: *mut c_void) -> i32 {
    S_OK
}
unsafe extern "win64" fn timer_enable(this: *mut TimerObj) -> i32 {
    let mut e = crate::dlls::lock(&Com::get(this).enabled);
    if *e {
        return S_FALSE;
    }
    *e = true;
    S_OK
}
unsafe extern "win64" fn timer_disable(this: *mut TimerObj) -> i32 {
    let mut e = crate::dlls::lock(&Com::get(this).enabled);
    if !*e {
        return S_FALSE;
    }
    *e = false;
    S_OK
}
unsafe extern "win64" fn timer_is_enabled(this: *mut TimerObj) -> i32 {
    if *crate::dlls::lock(&Com::get(this).enabled) {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn timer_time(_: *mut TimerObj, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = now();
    S_OK
}
unsafe extern "win64" fn timer_fps(_: *mut TimerObj, _: u32) -> i32 {
    S_OK
}

static TIMER_VT: VTable<10> = VTable([
    timer_qi as *const (),
    add_ref::<Timer> as *const (),
    release::<Timer> as *const (),
    timer_set_update as *const (),
    timer_set_events as *const (),
    timer_enable as *const (),
    timer_disable as *const (),
    timer_is_enabled as *const (),
    timer_time as *const (),
    timer_fps as *const (),
]);

// ───────────────────────── Création ─────────────────────────

/// Classes de UIAnimation.dll pour CoCreateInstance.
pub unsafe fn create_builtin(clsid: &Guid, iid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    let obj: *mut c_void = if *clsid == CLSID_MANAGER {
        new_manager().cast()
    } else if *clsid == CLSID_LIBRARY {
        Com::create(LIBRARY_VT.ptr(), Library).cast()
    } else if *clsid == CLSID_FACTORY {
        Com::create(FACTORY_VT.ptr(), Factory).cast()
    } else if *clsid == CLSID_TIMER {
        Com::create(
            TIMER_VT.ptr(),
            Timer {
                handler: Mutex::new(std::ptr::null_mut()),
                enabled: Mutex::new(false),
            },
        )
        .cast()
    } else {
        return None;
    };
    type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
    let vt = *(obj as *const *const usize);
    let qi: Qi = std::mem::transmute(*vt);
    let hr = qi(obj, iid, out);
    release_raw(obj);
    Some(hr)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<F: Copy>(obj: *mut c_void, slot: usize) -> F {
        let vt = *(obj as *const *const usize);
        std::mem::transmute_copy(&*vt.add(slot))
    }

    #[test]
    fn vtables_have_windows_sizes() {
        assert_eq!(MANAGER_VT.0.len(), 22);
        assert_eq!(VARIABLE_VT.0.len(), 17);
        assert_eq!(STORYBOARD_VT.0.len(), 20);
        assert_eq!(TIMER_VT.0.len(), 10);
        assert_eq!(LIBRARY_VT.0.len(), 15);
        assert_eq!(TRANSITION_VT.0.len(), 7);
    }

    #[test]
    fn linear_transition_moves_variable_over_time() {
        unsafe {
            let mut m = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&CLSID_MANAGER, &IID_MANAGER, &mut m),
                Some(S_OK)
            );
            let mut lib = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&CLSID_LIBRARY, &IID_LIBRARY, &mut lib),
                Some(S_OK)
            );
            let mut v = std::ptr::null_mut();
            assert_eq!(mgr_create_variable(m.cast(), 10.0, &mut v), S_OK);
            let mut t = std::ptr::null_mut();
            assert_eq!(lib_linear(lib.cast(), 2.0, 30.0, &mut t), S_OK);
            let mut s = std::ptr::null_mut();
            assert_eq!(mgr_create_storyboard(m.cast(), &mut s), S_OK);
            assert_eq!(sb_add(s.cast(), v, t), S_OK);
            let mut res = 0;
            assert_eq!(sb_schedule(s.cast(), 100.0, &mut res), S_OK);
            assert_eq!(res, SCHEDULING_SUCCEEDED);
            let mut r = 0;
            mgr_update(m.cast(), 101.0, &mut r);
            assert_eq!(r, 1);
            let get: unsafe extern "win64" fn(*mut c_void, *mut f64) -> i32 = call(v, 4);
            let mut fin = 0.0;
            get(v, &mut fin);
            assert_eq!(fin, 30.0);
            assert!((var(v.cast()).value - 20.0).abs() < 1e-9);
            mgr_update(m.cast(), 105.0, &mut r);
            assert_eq!(var(v.cast()).value, 30.0);
            let mut st = 0;
            sb_status(s.cast(), &mut st);
            assert_eq!(st, SB_FINISHED);
            assert!(mgr(m.cast()).scheduled.is_empty());
            for p in [t, s, v, lib, m] {
                release_raw(p);
            }
        }
    }

    #[test]
    fn accelerate_decelerate_reaches_target() {
        let c = Curve {
            kind: Kind::AccelDecel {
                d: 1.0,
                fin: 100.0,
                acc: 0.3,
                dec: 0.3,
            },
            v0: 0.0,
            u0: 0.0,
        };
        assert!(c.value(0.0).abs() < 1e-9);
        assert!((c.value(1.0) - 100.0).abs() < 1e-6);
        assert!((c.value(0.5) - 50.0).abs() < 1e-6);
    }

    // Interpolateur d'application minimal : va de la valeur initiale à 50
    // en 2 secondes, en ligne droite.
    struct Fake {
        v0: Cell<f64>,
    }
    unsafe extern "win64" fn f_noop(_: *mut Com<Fake>) -> u32 {
        1
    }
    unsafe extern "win64" fn f_start(this: *mut Com<Fake>, v0: f64, _: f64) -> i32 {
        Com::get(this).v0.set(v0);
        S_OK
    }
    unsafe extern "win64" fn f_dur(_: *mut Com<Fake>, out: *mut f64) -> i32 {
        *out = 2.0;
        S_OK
    }
    unsafe extern "win64" fn f_fin(_: *mut Com<Fake>, out: *mut f64) -> i32 {
        *out = 50.0;
        S_OK
    }
    unsafe extern "win64" fn f_value(this: *mut Com<Fake>, t: f64, out: *mut f64) -> i32 {
        let v0 = Com::get(this).v0.get();
        *out = v0 + (50.0 - v0) * t / 2.0;
        S_OK
    }
    static FAKE_VT: VTable<10> = VTable([
        std::ptr::null(),
        add_ref::<Fake> as *const (),
        release::<Fake> as *const (),
        f_start as *const (),
        std::ptr::null(),
        f_dur as *const (),
        f_fin as *const (),
        f_value as *const (),
        std::ptr::null(),
        f_noop as *const (),
    ]);

    #[test]
    fn factory_wraps_application_interpolator() {
        unsafe {
            assert_eq!(FACTORY_VT.0.len(), 4);
            let fake: *mut c_void = Com::create(FAKE_VT.ptr(), Fake { v0: Cell::new(0.0) }).cast();
            let mut f = std::ptr::null_mut();
            assert_eq!(
                create_builtin(&CLSID_FACTORY, &IID_FACTORY, &mut f),
                Some(S_OK)
            );
            let mut t = std::ptr::null_mut();
            assert_eq!(fac_create(f.cast(), fake, &mut t), S_OK);
            let mut d = 0.0;
            tr_set_initial(t.cast(), 10.0);
            assert_eq!(tr_duration(t.cast(), &mut d), S_OK);
            assert_eq!(d, 2.0);
            let c = *crate::dlls::lock(&Com::get(t as *mut TransitionObj).curve);
            assert!((c.value(1.0) - 30.0).abs() < 1e-9);
            assert_eq!(c.final_value(), 50.0);
            release_raw(t);
            release_raw(f);
            release_raw(fake);
        }
    }
}
