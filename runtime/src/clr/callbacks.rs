//! Rappels natifs : un délégué passé à une fonction native (`EnumWindows`,
//! `qsort`, procédure de fenêtre…) devient un pointeur de fonction Win64.
//! Chaque délégué reçoit l'une des 64 portes d'entrée compilées d'avance,
//! qui rappelle l'interpréteur en cours sur ce fil.

use std::cell::Cell;
use std::sync::Mutex;

use super::gc;
use super::il::{Interpreter, Value};
use super::metadata::{self, ElementType};
use super::pinvoke;
use super::types::{self, Kind};

/// Nombre de délégués qui peuvent être passés au code natif.
pub const CAPACITY: usize = 64;

struct Slot {
    delegate: usize,
    /// Types des paramètres d'`Invoke` (vide si inconnus : mots bruts).
    params: Vec<(Kind, bool)>,
    unicode: bool,
}

static SLOTS: Mutex<Vec<Slot>> = Mutex::new(Vec::new());

thread_local! {
    static CURRENT: Cell<*mut Interpreter> = const { Cell::new(std::ptr::null_mut()) };
}

/// Exécute `f` en laissant `interp` joignable par les rappels natifs.
pub fn with_interpreter<T>(interp: &mut Interpreter, f: impl FnOnce() -> T) -> T {
    let previous = CURRENT.with(|c| c.replace(interp as *mut Interpreter));
    let r = f();
    CURRENT.with(|c| c.set(previous));
    r
}

/// Rend `interp` joignable par les rappels natifs de ce fil ; renvoie le
/// précédent.
pub fn set_current(interp: *mut Interpreter) -> *mut Interpreter {
    CURRENT.with(|c| c.replace(interp))
}

/// Délégué derrière un pointeur de fonction rendu par `callback_for`.
pub fn delegate_for_pointer(p: usize) -> Option<usize> {
    let i = THUNKS.iter().position(|t| *t as usize == p)?;
    SLOTS.lock().unwrap().get(i).map(|s| s.delegate)
}

/// Délégués confiés au code natif, racines du ramasse-miettes.
pub fn roots() -> Vec<usize> {
    SLOTS.lock().unwrap().iter().map(|s| s.delegate).collect()
}

macro_rules! thunk {
    ($name:ident, $index:expr) => {
        extern "win64" fn $name(
            a0: usize,
            a1: usize,
            a2: usize,
            a3: usize,
            a4: usize,
            a5: usize,
            a6: usize,
            a7: usize,
        ) -> usize {
            dispatch($index, [a0, a1, a2, a3, a4, a5, a6, a7])
        }
    };
}

thunk!(t0, 0);
thunk!(t1, 1);
thunk!(t2, 2);
thunk!(t3, 3);
thunk!(t4, 4);
thunk!(t5, 5);
thunk!(t6, 6);
thunk!(t7, 7);
thunk!(t8, 8);
thunk!(t9, 9);
thunk!(t10, 10);
thunk!(t11, 11);
thunk!(t12, 12);
thunk!(t13, 13);
thunk!(t14, 14);
thunk!(t15, 15);
thunk!(t16, 16);
thunk!(t17, 17);
thunk!(t18, 18);
thunk!(t19, 19);
thunk!(t20, 20);
thunk!(t21, 21);
thunk!(t22, 22);
thunk!(t23, 23);
thunk!(t24, 24);
thunk!(t25, 25);
thunk!(t26, 26);
thunk!(t27, 27);
thunk!(t28, 28);
thunk!(t29, 29);
thunk!(t30, 30);
thunk!(t31, 31);
thunk!(t32, 32);
thunk!(t33, 33);
thunk!(t34, 34);
thunk!(t35, 35);
thunk!(t36, 36);
thunk!(t37, 37);
thunk!(t38, 38);
thunk!(t39, 39);
thunk!(t40, 40);
thunk!(t41, 41);
thunk!(t42, 42);
thunk!(t43, 43);
thunk!(t44, 44);
thunk!(t45, 45);
thunk!(t46, 46);
thunk!(t47, 47);
thunk!(t48, 48);
thunk!(t49, 49);
thunk!(t50, 50);
thunk!(t51, 51);
thunk!(t52, 52);
thunk!(t53, 53);
thunk!(t54, 54);
thunk!(t55, 55);
thunk!(t56, 56);
thunk!(t57, 57);
thunk!(t58, 58);
thunk!(t59, 59);
thunk!(t60, 60);
thunk!(t61, 61);
thunk!(t62, 62);
thunk!(t63, 63);

type Thunk = extern "win64" fn(usize, usize, usize, usize, usize, usize, usize, usize) -> usize;

static THUNKS: [Thunk; CAPACITY] = [
    t0, t1, t2, t3, t4, t5, t6, t7, t8, t9, t10, t11, t12, t13, t14, t15, t16, t17, t18, t19, t20,
    t21, t22, t23, t24, t25, t26, t27, t28, t29, t30, t31, t32, t33, t34, t35, t36, t37, t38, t39,
    t40, t41, t42, t43, t44, t45, t46, t47, t48, t49, t50, t51, t52, t53, t54, t55, t56, t57, t58,
    t59, t60, t61, t62, t63,
];

impl Interpreter {
    /// Pointeur de fonction natif qui appelle le délégué `d`.
    pub(super) fn callback_for(&mut self, d: *mut u8, unicode: bool) -> Option<usize> {
        let slots = SLOTS.lock().unwrap();
        if let Some(i) = slots.iter().position(|s| s.delegate == d as usize) {
            return Some(THUNKS[i] as usize);
        }
        if slots.len() >= CAPACITY {
            return None;
        }
        drop(slots);
        let params = self.invoke_params(d);
        let mut slots = SLOTS.lock().unwrap();
        slots.push(Slot {
            delegate: d as usize,
            params,
            unicode,
        });
        Some(THUNKS[slots.len() - 1] as usize)
    }

    /// Paramètres de la méthode `Invoke` du type du délégué `d` : genre de
    /// valeur et « est une chaîne ».
    fn invoke_params(&mut self, d: *mut u8) -> Vec<(Kind, bool)> {
        let Some(id) = gc::type_of(d) else {
            return Vec::new();
        };
        let Some(((asm, row), args)) = types::with_types(|reg| {
            reg.get(id)
                .and_then(|t| Some((t.def?, t.type_args.clone())))
        }) else {
            return Vec::new();
        };
        let a = self.assembly_at(asm);
        let Some(invoke) = a
            .type_methods(row)
            .find(|&m| a.get_string(a.get_method_def(m).name) == "Invoke")
        else {
            return Vec::new();
        };
        let sig = metadata::decode_method_sig(a.get_blob(a.get_method_def(invoke).signature));
        let gen = super::il::Gen {
            t: args,
            m: Vec::new(),
        };
        sig.param_types
            .iter()
            .map(|et| {
                (
                    self.kind_of_sig(asm, et, &gen),
                    matches!(et, ElementType::String),
                )
            })
            .collect()
    }
}

fn dispatch(index: usize, raw: [usize; 8]) -> usize {
    let interp = CURRENT.with(|c| c.get());
    let (delegate, params, unicode) = {
        let slots = SLOTS.lock().unwrap();
        let Some(s) = slots.get(index) else {
            return 0;
        };
        (s.delegate, s.params.clone(), s.unicode)
    };
    if interp.is_null() {
        return dispatch_foreign(delegate, &params, unicode, raw);
    }
    // SAFETY : l'interpréteur attend le retour de l'appel natif qui nous
    // rappelle (`with_interpreter`) et ne touche pas à son état d'ici là.
    let interp = unsafe { &mut *interp };
    run(interp, delegate, &params, unicode, raw)
}

/// Rappel arrivé sur un fil que le code natif a créé lui-même
/// (`CreateThread`, ServiceMain de `StartServiceCtrlDispatcher`, #2383) :
/// le fil entre dans le runtime avec son propre interpréteur, le temps du
/// rappel, comme un fil managé (`threads::launch`).
fn dispatch_foreign(
    delegate: usize,
    params: &[(Kind, bool)],
    unicode: bool,
    raw: [usize; 8],
) -> usize {
    super::threads::gil_acquire();
    let Some(model) = super::threads::any_interpreter() else {
        super::threads::gil_release();
        crate::log_warn!("clr-interop", "rappel natif sans interpréteur vivant");
        return 0;
    };
    if crate::process::teb() == 0 {
        crate::process::init_thread();
    }
    // SAFETY : le verrou global est pris ; l'interpréteur modèle n'est que
    // lu (assemblies, domaine), son fil attend ce verrou ou un appel natif.
    let mut own = Box::new(unsafe { &*model }.fork());
    let p: *mut Interpreter = &mut *own;
    super::threads::register(p);
    let previous = set_current(p);
    let result = run(&mut own, delegate, params, unicode, raw);
    set_current(previous);
    super::threads::unregister(p);
    drop(own);
    super::threads::gil_release();
    result
}

fn run(
    interp: &mut Interpreter,
    delegate: usize,
    params: &[(Kind, bool)],
    unicode: bool,
    raw: [usize; 8],
) -> usize {
    let count = if params.is_empty() {
        4
    } else {
        params.len().min(8)
    };
    let args: Vec<Value> = (0..count)
        .map(|i| {
            let word = raw[i];
            match params.get(i) {
                Some((_, true)) => {
                    if word == 0 {
                        Value::Null
                    } else {
                        super::bcl::alloc_managed_string(&pinvoke::read_native_string(
                            word as u64,
                            unicode,
                        ))
                    }
                }
                Some((Kind::Ref, false)) => {
                    if word == 0 {
                        Value::Null
                    } else {
                        Value::Ref(word as *mut u8)
                    }
                }
                Some((kind, false))
                    if kind.elem_size() <= 4 && !matches!(kind, Kind::Struct(_)) =>
                {
                    super::il::load(&word as *const usize as *const u8, *kind)
                }
                _ => Value::I64(word as i64),
            }
        })
        .collect();
    // Le fil a rendu le verrou global pendant l'appel natif : il le
    // reprend le temps du rappel.
    super::threads::gil_acquire();
    let result = interp.invoke_delegate(delegate as *mut u8, args);
    super::threads::gil_release();
    match result {
        Ok(v) => v.as_i64() as usize,
        Err(e) => {
            crate::log_warn!("clr-interop", "exception dans un rappel natif : {e}");
            0
        }
    }
}
