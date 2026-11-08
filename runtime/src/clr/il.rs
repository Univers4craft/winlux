//! Interpréteur CIL (Common Intermediate Language, ECMA-335 partition III).
//!
//! Machine à pile qui exécute le bytecode .NET instruction par instruction.
//! Chaque appel managé crée un `CallFrame` et s'exécute dans son propre
//! `run_loop`, qui rend la main à l'appelant au `ret` de la méthode.
//!
//! Mémoire : arguments et variables locales vivent dans une zone propre à
//! l'appel, qui ne bouge pas tant que la méthode s'exécute ; `ldloca` et
//! `ldarga` donnent donc de vraies adresses, lues et écrites par `ldind`,
//! `stind`, `ldfld`… Chaque valeur est rangée selon son type (`Kind`) : un
//! entier sur sa largeur, une référence sur 8 octets, un type valeur en
//! place. Sur la pile d'évaluation, un type valeur est une copie temporaire
//! dans le tas, recopiée à chaque rangement : la sémantique de copie est
//! respectée.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::bcl;
use super::delegates::FnTable;
use super::domain::AsmInfo;
use super::exceptions::{self, ClrException};
use super::gc;
use super::metadata::{self, Assembly, ElementType};
use super::opcodes::{self, OpCode};
use super::pinvoke;
use super::resolve::{Callee, Target};
use super::tables::Token;
use super::threads;
use super::types::{self, Kind, TypeId};

/// Profondeur d'appel maximale avant StackOverflowException.
const MAX_DEPTH: usize = 4000;

#[derive(Debug, Clone, Copy)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Ref(*mut u8),
    Null,
}

impl Value {
    pub fn as_i32(self) -> i32 {
        match self {
            Value::I32(v) => v,
            Value::I64(v) => v as i32,
            Value::F32(v) => v as i32,
            Value::F64(v) => v as i32,
            Value::Ref(p) => p as i32,
            Value::Null => 0,
        }
    }

    pub fn as_i64(self) -> i64 {
        match self {
            Value::I32(v) => v as i64,
            Value::I64(v) => v,
            Value::F32(v) => v as i64,
            Value::F64(v) => v as i64,
            Value::Ref(p) => p as i64,
            Value::Null => 0,
        }
    }

    pub fn as_f64(self) -> f64 {
        match self {
            Value::I32(v) => v as f64,
            Value::I64(v) => v as f64,
            Value::F32(v) => v as f64,
            Value::F64(v) => v,
            _ => 0.0,
        }
    }

    pub fn as_ref(self) -> *mut u8 {
        match self {
            Value::Ref(p) => p,
            Value::I64(v) => v as *mut u8,
            Value::I32(v) => v as isize as *mut u8,
            _ => std::ptr::null_mut(),
        }
    }

    pub fn is_truthy(self) -> bool {
        match self {
            Value::I32(v) => v != 0,
            Value::I64(v) => v != 0,
            Value::F32(v) => v != 0.0,
            Value::F64(v) => v != 0.0,
            Value::Ref(p) => !p.is_null(),
            Value::Null => false,
        }
    }

    fn is_float(self) -> bool {
        matches!(self, Value::F32(_) | Value::F64(_))
    }

    /// Entier 64 bits ou pointeur (sinon entier 32 bits).
    fn is_wide(self) -> bool {
        matches!(self, Value::I64(_) | Value::Ref(_))
    }
}

/// Lit une valeur de type `kind` rangée à `ptr`. Un type valeur est recopié
/// dans un objet temporaire du tas.
pub fn load(ptr: *const u8, kind: Kind) -> Value {
    unsafe {
        match kind {
            Kind::Bool | Kind::U1 => Value::I32(*ptr as i32),
            Kind::I1 => Value::I32(*(ptr as *const i8) as i32),
            Kind::I2 => Value::I32((ptr as *const i16).read_unaligned() as i32),
            Kind::U2 | Kind::Char => Value::I32((ptr as *const u16).read_unaligned() as i32),
            Kind::I4 | Kind::U4 => Value::I32((ptr as *const i32).read_unaligned()),
            Kind::I8 | Kind::U8 | Kind::I => Value::I64((ptr as *const i64).read_unaligned()),
            Kind::R4 => Value::F32((ptr as *const f32).read_unaligned()),
            Kind::R8 => Value::F64((ptr as *const f64).read_unaligned()),
            Kind::Ref => match (ptr as *const *mut u8).read_unaligned() {
                p if p.is_null() => Value::Null,
                p => Value::Ref(p),
            },
            Kind::Struct(id) => {
                let size = kind.elem_size();
                let copy = gc::with_gc(|gc| gc.alloc(id, size));
                std::ptr::copy_nonoverlapping(ptr, copy, size);
                Value::Ref(copy)
            }
        }
    }
}

/// Range `val` à `ptr` selon `kind`. Un type valeur est recopié depuis son
/// objet temporaire (une valeur nulle le remet à zéro).
pub fn store(ptr: *mut u8, kind: Kind, val: Value) {
    unsafe {
        match kind {
            Kind::Bool | Kind::I1 | Kind::U1 => *ptr = val.as_i32() as u8,
            Kind::I2 | Kind::U2 | Kind::Char => {
                (ptr as *mut u16).write_unaligned(val.as_i32() as u16)
            }
            Kind::I4 | Kind::U4 => (ptr as *mut i32).write_unaligned(val.as_i32()),
            Kind::I8 | Kind::U8 | Kind::I => (ptr as *mut i64).write_unaligned(val.as_i64()),
            Kind::R4 => (ptr as *mut f32).write_unaligned(val.as_f64() as f32),
            Kind::R8 => (ptr as *mut f64).write_unaligned(val.as_f64()),
            Kind::Ref => (ptr as *mut *mut u8).write_unaligned(match val {
                Value::Null => std::ptr::null_mut(),
                v => v.as_ref(),
            }),
            Kind::Struct(_) => {
                let size = kind.elem_size();
                let src = val.as_ref();
                if src.is_null() {
                    std::ptr::write_bytes(ptr, 0, size);
                } else if src != ptr {
                    std::ptr::copy(src, ptr, size);
                }
            }
        }
    }
}

/// Contexte générique d'une méthode : arguments de type de sa classe (`!0`)
/// et de la méthode elle-même (`!!0`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gen {
    pub t: Vec<TypeId>,
    pub m: Vec<TypeId>,
}

/// Zone mémoire d'un appel : emplacements de 8 octets, alignés, qui ne
/// bougent pas pendant toute la durée de l'appel.
struct Slots {
    words: Box<[u64]>,
    layout: Vec<(usize, Kind)>,
}

impl Slots {
    fn new(kinds: &[Kind]) -> Slots {
        let mut layout = Vec::with_capacity(kinds.len());
        let mut offset = 0;
        for &k in kinds {
            layout.push((offset, k));
            offset += k.slot_size();
        }
        Slots {
            words: vec![0u64; offset / 8].into_boxed_slice(),
            layout,
        }
    }

    fn addr(&mut self, idx: usize) -> Option<(*mut u8, Kind)> {
        let &(off, kind) = self.layout.get(idx)?;
        Some((
            unsafe { (self.words.as_mut_ptr() as *mut u8).add(off) },
            kind,
        ))
    }

    fn get(&mut self, idx: usize) -> Value {
        self.addr(idx).map_or(Value::Null, |(p, k)| load(p, k))
    }

    fn set(&mut self, idx: usize, v: Value) {
        if let Some((p, k)) = self.addr(idx) {
            store(p, k, v);
        }
    }
}

struct CallFrame {
    ip: usize,
    code: Rc<Vec<u8>>,
    locals: Slots,
    args: Slots,
    eval_stack: Vec<Value>,
    exception_clauses: Rc<Vec<metadata::ExceptionClause>>,
    /// Blocs finally/fault en cours, par début de gestionnaire, avec ce
    /// qu'il faut reprendre à leur `endfinally`.
    unwinding: Vec<(usize, Unwind)>,
    /// Exceptions des catch en cours, par début de gestionnaire (pour
    /// `rethrow`).
    caught: Vec<(usize, ClrException)>,
    gen: Rc<Gen>,
    /// Type du préfixe `constrained.` en attente du `callvirt` suivant.
    constrained: Option<TypeId>,
}

pub struct Interpreter {
    frames: Vec<CallFrame>,
    /// Assembly de la méthode en cours ; les autres attendent dans `parked`.
    pub assembly: Assembly,
    parked: Vec<Option<Assembly>>,
    pub(super) current: usize,
    pub(super) domain: Vec<AsmInfo>,
    /// État commun à tous les fils managés.
    pub(super) shared: Arc<Shared>,
    /// Valeurs en transit vers du code natif, gardées en vie par le
    /// ramasse-miettes.
    pub(super) pins: Vec<Value>,
    /// Instructions exécutées depuis le dernier passage de main.
    ticks: u32,
    /// Constructeurs statiques en cours sur ce fil : pas de passage de main
    /// au milieu (un autre fil verrait le type à moitié initialisé).
    cctor_depth: u32,
}

/// État partagé par les interpréteurs des différents fils.
#[derive(Default)]
pub struct Shared {
    /// Types dont le constructeur statique a été lancé.
    initialized: Mutex<HashSet<TypeId>>,
    /// Champs statiques des types de la bibliothèque intégrée.
    external_statics: Mutex<HashMap<(usize, u32), Box<[u64; 4]>>>,
    /// Pointeurs de méthode rendus par `ldftn` / `ldvirtftn`.
    pub(super) fns: Mutex<FnTable>,
}

pub enum ExecResult {
    Return(Value),
    Exception(ClrException),
}

/// Suite d'un bloc finally à son `endfinally`.
#[derive(Clone, Copy)]
enum Unwind {
    /// Exception à relancer.
    Exception(ClrException),
    /// Destination d'un `leave`.
    Leave(usize),
}

enum StepResult {
    Continue,
    /// Fin d'un bloc filtre : vrai si le gestionnaire est retenu.
    EndFilter(bool),
    Branch(usize),
    Return(Value),
    Throw(ClrException),
}

fn exception(name: &str, message: &str) -> ClrException {
    ClrException::new("System", name, message)
}

fn overflow() -> ClrException {
    exception(
        "OverflowException",
        "Arithmetic operation resulted in an overflow.",
    )
}

fn invalid_cast() -> ClrException {
    exception("InvalidCastException", "Specified cast is not valid.")
}

impl Interpreter {
    pub fn new(assemblies: Vec<Assembly>, domain: Vec<AsmInfo>) -> Self {
        let mut parked: Vec<Option<Assembly>> = assemblies.into_iter().map(Some).collect();
        let assembly = parked[0].take().expect("assembly du programme");
        Interpreter {
            frames: Vec::new(),
            assembly,
            parked,
            current: 0,
            domain,
            shared: Arc::new(Shared::default()),
            pins: Vec::new(),
            ticks: 0,
            cctor_depth: 0,
        }
    }

    /// Interpréteur d'un nouveau fil : mêmes assemblies et même état
    /// partagé, pile d'appels vide.
    pub(super) fn fork(&self) -> Interpreter {
        let mut parked: Vec<Option<Assembly>> = (0..self.parked.len())
            .map(|i| Some(self.assembly_at(i).clone()))
            .collect();
        let assembly = parked[0].take().expect("assembly du programme");
        Interpreter {
            frames: Vec::new(),
            assembly,
            parked,
            current: 0,
            domain: self.domain.clone(),
            shared: self.shared.clone(),
            pins: Vec::new(),
            ticks: 0,
            cctor_depth: 0,
        }
    }

    /// Rend `index` assembly courante ; renvoie la précédente.
    pub(super) fn enter(&mut self, index: usize) -> usize {
        let previous = self.current;
        if index != previous {
            let next = self.parked[index].take().expect("assembly déjà active");
            let old = std::mem::replace(&mut self.assembly, next);
            self.parked[previous] = Some(old);
            self.current = index;
        }
        previous
    }

    pub(super) fn assembly_at(&self, index: usize) -> &Assembly {
        if index == self.current {
            &self.assembly
        } else {
            self.parked[index].as_ref().expect("assembly du domaine")
        }
    }

    /// Contexte générique de la méthode en cours.
    pub(super) fn current_gen(&self) -> Gen {
        self.frames
            .last()
            .map(|f| (*f.gen).clone())
            .unwrap_or_default()
    }

    /// Exécute le point d'entrée (ou une méthode non générique) de
    /// l'assembly courante.
    pub fn execute_method(&mut self, method_row: u32, args: Vec<Value>) -> ExecResult {
        self.execute_gen(method_row, args, Gen::default())
    }

    /// Exécute une méthode d'une assembly quelconque du domaine.
    fn execute_in(
        &mut self,
        asm: usize,
        method_row: u32,
        args: Vec<Value>,
        gen: Gen,
    ) -> ExecResult {
        let previous = self.enter(asm);
        let result = self.execute_gen(method_row, args, gen);
        self.enter(previous);
        result
    }

    /// Type (instancié) qui déclare une méthode de l'assembly courante.
    fn owner_type(&self, method_row: u32, gen: &Gen) -> Option<TypeId> {
        let info = &self.domain[self.current];
        let owner = info
            .method_owner
            .get(method_row as usize)
            .copied()
            .unwrap_or(0);
        let id = info.type_ids.get(owner as usize).copied().flatten()?;
        Some(if gen.t.is_empty() {
            id
        } else {
            self.instantiate(id, gen.t.clone())
        })
    }

    /// Lance une seule fois le constructeur statique d'un type, avant son
    /// premier usage (ECMA-335 §II.10.5.3.1).
    fn ensure_initialized(&mut self, id: TypeId) -> Result<(), ClrException> {
        if !self.shared.initialized.lock().unwrap().insert(id) {
            return Ok(());
        }
        let Some(((asm, row), args)) = types::with_types(|reg| {
            reg.get(id)
                .and_then(|t| Some((t.def?, t.type_args.clone())))
        }) else {
            return Ok(());
        };
        let a = self.assembly_at(asm);
        let cctor = a.type_methods(row).find(|&m| {
            let md = a.get_method_def(m);
            md.is_cctor() && md.rva != 0 && a.get_string(md.name) == ".cctor"
        });
        match cctor {
            Some(m) => {
                self.cctor_depth += 1;
                let r = self.execute_in(
                    asm,
                    m,
                    Vec::new(),
                    Gen {
                        t: args,
                        m: Vec::new(),
                    },
                );
                self.cctor_depth -= 1;
                match r {
                    ExecResult::Exception(e) => Err(e),
                    ExecResult::Return(_) => Ok(()),
                }
            }
            None => Ok(()),
        }
    }

    fn execute_gen(&mut self, method_row: u32, args: Vec<Value>, gen: Gen) -> ExecResult {
        let owner = self.owner_type(method_row, &gen);
        if let Some(owner) = owner {
            if let Err(e) = self.ensure_initialized(owner) {
                return ExecResult::Exception(e);
            }
        }
        let md = self.assembly.get_method_def(method_row);
        let sig = metadata::decode_method_sig(self.assembly.get_blob(md.signature));

        if md.is_pinvoke() {
            return match self.call_pinvoke(method_row, args, &sig, &gen) {
                Ok(v) => ExecResult::Return(v),
                Err(e) => ExecResult::Exception(e),
            };
        }

        if md.rva == 0 {
            let owner_row = self.domain[self.current]
                .method_owner
                .get(method_row as usize)
                .copied()
                .unwrap_or(0);
            let td = self.assembly.get_type_def(owner_row);
            let namespace = self.assembly.get_string(td.namespace).to_string();
            let class = self.assembly.get_string(td.name).to_string();
            let name = self.assembly.get_string(md.name).to_string();
            return match self.call_native(&namespace, &class, &name, args, &sig.param_types) {
                Ok(v) => ExecResult::Return(v),
                Err(e) => ExecResult::Exception(e),
            };
        }

        if self.frames.len() >= MAX_DEPTH {
            return ExecResult::Exception(ClrException::stack_overflow());
        }
        let Some(body) = self.assembly.method_body(md.rva) else {
            return ExecResult::Exception(exception(
                "BadImageFormatException",
                &format!("corps de méthode illisible (RVA {:#x})", md.rva),
            ));
        };

        let mut arg_kinds = Vec::with_capacity(sig.param_types.len() + 1);
        if sig.has_this() {
            let value_this = owner
                .is_some_and(|o| types::with_types(|reg| reg.get(o).is_some_and(|t| t.is_value)));
            arg_kinds.push(if value_this { Kind::I } else { Kind::Ref });
        }
        for p in &sig.param_types {
            arg_kinds.push(self.kind_of_sig(self.current, p, &gen));
        }
        let mut arg_slots = Slots::new(&arg_kinds);
        for (i, v) in args.into_iter().enumerate().take(arg_kinds.len()) {
            arg_slots.set(i, v);
        }

        let local_kinds: Vec<Kind> = if body.local_var_sig_token != 0 {
            let sig = self
                .assembly
                .get_standalone_sig(body.local_var_sig_token & 0x00FF_FFFF);
            metadata::decode_locals_sig(self.assembly.get_blob(sig.signature))
                .iter()
                .map(|et| self.kind_of_sig(self.current, et, &gen))
                .collect()
        } else {
            Vec::new()
        };

        self.frames.push(CallFrame {
            ip: 0,
            code: Rc::new(body.code),
            locals: Slots::new(&local_kinds),
            args: arg_slots,
            eval_stack: Vec::with_capacity(body.max_stack as usize),
            exception_clauses: Rc::new(body.exception_clauses),
            unwinding: Vec::new(),
            caught: Vec::new(),
            gen: Rc::new(gen),
            constrained: None,
        });
        self.run_loop()
    }

    /// Appelle `callee`. Avec `virtual_call`, la redéfinition du type réel de
    /// `this` est choisie.
    pub(super) fn invoke(
        &mut self,
        token: u32,
        callee: Callee,
        args: Vec<Value>,
        virtual_call: bool,
    ) -> Result<Value, ClrException> {
        let Callee {
            mut target,
            mut gen,
        } = callee;
        if virtual_call {
            match args.first() {
                Some(Value::Ref(this)) => self.devirtualize(token, *this, &mut target, &mut gen),
                Some(Value::Null) => return Err(ClrException::null_reference()),
                _ => {}
            }
        }
        match target {
            Target::Managed(asm, row) => match self.execute_in(asm, row, args, gen) {
                ExecResult::Return(v) => Ok(v),
                ExecResult::Exception(e) => Err(e),
            },
            Target::External {
                namespace,
                class,
                name,
            } => {
                let params = self.get_method_sig(token).param_types;
                self.call_native(&namespace, &class, &name, args, &params)
            }
        }
    }

    /// Remplace la méthode appelée par la redéfinition du type réel de
    /// `this`.
    fn devirtualize(&self, token: u32, this: *mut u8, target: &mut Target, gen: &mut Gen) {
        let Some(type_id) = gc::type_of(this) else {
            return;
        };
        let name = self.method_name(token);
        let argc = self.get_method_sig(token).param_types.len();
        let found = types::with_types(|reg| {
            let (asm, tok, decl) = reg.find_override(type_id, &name, argc)?;
            Some((
                asm,
                tok,
                reg.get(decl)
                    .map(|t| t.type_args.clone())
                    .unwrap_or_default(),
            ))
        });
        if let Some((asm, tok, type_args)) = found {
            *target = Target::Managed(asm, Token(tok).rid());
            gen.t = type_args;
        }
    }

    /// Méthode implémentée par le runtime (bibliothèque intégrée).
    fn call_native(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: Vec<Value>,
        params: &[ElementType],
    ) -> Result<Value, ClrException> {
        if let Some(r) = self.delegate_call(namespace, class, name, &args, params) {
            return r;
        }
        if let Some(r) = self.thread_call(namespace, class, name, &args, params) {
            return r;
        }
        match (namespace, class, name) {
            (_, _, ".ctor") => {
                // Constructeurs de la BCL (Object, Exception…) : seul le message
                // éventuel d'une exception est conservé.
                if let (Some(Value::Ref(this)), Some(Value::Ref(msg))) = (args.first(), args.get(1))
                {
                    if gc::type_of(*msg) == Some(types::TYPE_STRING) && gc::type_of(*this).is_some()
                    {
                        unsafe {
                            *(this.add(exceptions::MESSAGE_SLOT) as *mut *mut u8) = *msg;
                        }
                        // Exception(string, Exception) : exception interne.
                        if let (true, Some(Value::Ref(inner))) =
                            (class.ends_with("Exception"), args.get(2))
                        {
                            unsafe {
                                *(this.add(exceptions::INNER_SLOT) as *mut *mut u8) = *inner;
                            }
                        }
                    }
                }
                return Ok(Value::Null);
            }
            (_, c, "get_Message") if c.ends_with("Exception") => {
                let this = args.first().map_or(std::ptr::null_mut(), |v| v.as_ref());
                if this.is_null() {
                    return Err(ClrException::null_reference());
                }
                let msg = ClrException { obj_ptr: this }.message();
                return Ok(bcl::alloc_managed_string(&msg));
            }
            (_, c, "get_InnerException") if c.ends_with("Exception") => {
                let this = args.first().map_or(std::ptr::null_mut(), |v| v.as_ref());
                if this.is_null() {
                    return Err(ClrException::null_reference());
                }
                let inner = unsafe { *(this.add(exceptions::INNER_SLOT) as *const *mut u8) };
                return Ok(if gc::type_of(inner).is_some() {
                    Value::Ref(inner)
                } else {
                    Value::Null
                });
            }
            (
                "System.Runtime.InteropServices",
                "Marshal",
                "GetLastWin32Error" | "GetLastPInvokeError",
            ) => {
                return Ok(Value::I32(pinvoke::LAST_ERROR.with(|e| e.get()) as i32));
            }
            ("System", "GC", "Collect") => {
                self.collect_garbage();
                return Ok(Value::Null);
            }
            _ => {}
        }
        let pinned = self.pins.len();
        self.pins.extend(args.iter().copied());
        let mut args = args;
        let first_param = args.len() - params.len().min(args.len());
        for (i, param) in params.iter().enumerate() {
            if matches!(param, ElementType::Object) {
                match self.managed_to_string(args[first_param + i]) {
                    Ok(v) => {
                        args[first_param + i] = v;
                        self.pins.push(v);
                    }
                    Err(e) => {
                        self.pins.truncate(pinned);
                        return Err(e);
                    }
                }
            }
        }
        let result = bcl::dispatch_internal_call(namespace, class, name, &args, params);
        self.pins.truncate(pinned);
        result.ok_or_else(|| {
            exception(
                "MissingMethodException",
                &format!("Method not found: {namespace}.{class}::{name}"),
            )
        })
    }

    /// Remplace un objet par le résultat de sa redéfinition managée de
    /// `ToString()`, pour que la BCL native l'affiche comme .NET.
    fn managed_to_string(&mut self, v: Value) -> Result<Value, ClrException> {
        let Value::Ref(p) = v else { return Ok(v) };
        let Some(type_id) = gc::type_of(p) else {
            return Ok(v);
        };
        let found = types::with_types(|reg| {
            let (asm, tok, decl) = reg.find_override(type_id, "ToString", 0)?;
            Some((
                asm,
                tok,
                reg.get(decl)
                    .map(|t| t.type_args.clone())
                    .unwrap_or_default(),
            ))
        });
        match found {
            Some((asm, token, t)) => {
                match self.execute_in(asm, Token(token).rid(), vec![v], Gen { t, m: Vec::new() }) {
                    ExecResult::Return(s) => Ok(s),
                    ExecResult::Exception(e) => Err(e),
                }
            }
            None => Ok(v),
        }
    }

    /// Exécute la méthode au sommet de la pile d'appels jusqu'à son retour.
    fn run_loop(&mut self) -> ExecResult {
        loop {
            if gc::with_gc(|gc| gc.should_collect()) {
                self.collect_garbage();
            }
            // Point sûr : entre deux instructions, la main passe aux autres
            // fils managés de temps en temps.
            self.ticks = self.ticks.wrapping_add(1);
            if self.ticks.is_multiple_of(512) && self.cctor_depth == 0 {
                threads::gil_yield();
            }
            let frame = self.frames.last().unwrap();
            if frame.ip >= frame.code.len() {
                let ret = self.frames.pop().unwrap().eval_stack.pop();
                return ExecResult::Return(ret.unwrap_or(Value::Null));
            }
            let Some(instr) = opcodes::decode(&frame.code, frame.ip) else {
                let ip = frame.ip;
                self.frames.pop();
                return ExecResult::Exception(exception(
                    "InvalidProgramException",
                    &format!("opcode invalide à IL_{ip:04x}"),
                ));
            };
            let next_ip = instr.offset + instr.size;

            let result = match self.step(instr.op, instr.offset, next_ip) {
                Ok(r) => r,
                Err(e) => StepResult::Throw(e),
            };
            match result {
                StepResult::Continue => {
                    let f = self.frames.last_mut().unwrap();
                    if f.ip == instr.offset {
                        f.ip = next_ip;
                    }
                }
                StepResult::Branch(target) => self.frames.last_mut().unwrap().ip = target,
                StepResult::EndFilter(_) => {
                    let exc = exception("InvalidProgramException", "endfilter hors d'un filtre");
                    if let Some(unhandled) = self.raise(exc, instr.offset) {
                        self.frames.pop();
                        return ExecResult::Exception(unhandled);
                    }
                }
                StepResult::Return(v) => {
                    self.frames.pop();
                    return ExecResult::Return(v);
                }
                StepResult::Throw(exc) => {
                    if let Some(unhandled) = self.raise(exc, instr.offset) {
                        self.frames.pop();
                        return ExecResult::Exception(unhandled);
                    }
                }
            }
        }
    }

    /// Cherche un gestionnaire dans la méthode courante ; renvoie l'exception
    /// si elle doit remonter à l'appelant.
    fn raise(&mut self, exc: ClrException, ip: usize) -> Option<ClrException> {
        let clauses = self.frames.last().unwrap().exception_clauses.clone();
        {
            let f = self.frames.last_mut().unwrap();
            f.eval_stack.clear();
            f.constrained = None;
        }
        // Passe unique : les filtres sont évalués pendant la remontée, dans
        // l'ordre des clauses (ECMA-335 §I.12.4.2.5 en décrit deux ; seule
        // la place des finally internes par rapport aux filtres externes
        // diffère).
        for clause in clauses.iter().filter(|c| exceptions::try_covers(c, ip)) {
            let handler = clause.handler_offset as usize;
            let accepted = if exceptions::is_finally(clause) {
                true
            } else if exceptions::is_filter(clause) {
                self.run_filter(clause.class_token_or_filter as usize, exc)
            } else {
                self.catches(clause.class_token_or_filter, &exc)
            };
            if !accepted {
                continue;
            }
            let f = self.frames.last_mut().unwrap();
            // Les gestionnaires que l'exception quitte sont abandonnés.
            let left = |h: usize| {
                clauses.iter().any(|c| {
                    c.handler_offset as usize == h
                        && exceptions::handler_covers(c, ip)
                        && !exceptions::handler_covers(c, handler)
                })
            };
            f.caught.retain(|(h, _)| !left(*h));
            f.unwinding.retain(|(h, _)| !left(*h));
            f.eval_stack.clear();
            if exceptions::is_finally(clause) {
                f.unwinding.push((handler, Unwind::Exception(exc)));
            } else {
                f.eval_stack.push(Value::Ref(exc.obj_ptr));
                f.caught.push((handler, exc));
            }
            f.ip = handler;
            return None;
        }
        Some(exc)
    }

    /// Exécute un bloc filtre jusqu'à son `endfilter`. Une exception levée
    /// dans le filtre vaut un refus (ECMA-335 §III.3.34).
    fn run_filter(&mut self, offset: usize, exc: ClrException) -> bool {
        let depth = self.frames.len();
        let saved_ip = self.frames[depth - 1].ip;
        self.pins.push(Value::Ref(exc.obj_ptr));
        {
            let f = &mut self.frames[depth - 1];
            f.ip = offset;
            f.eval_stack.clear();
            f.eval_stack.push(Value::Ref(exc.obj_ptr));
        }
        let accepted = loop {
            if gc::with_gc(|gc| gc.should_collect()) {
                self.collect_garbage();
            }
            let f = &self.frames[depth - 1];
            let Some(instr) = opcodes::decode(&f.code, f.ip) else {
                break false;
            };
            let next_ip = instr.offset + instr.size;
            match self.step(instr.op, instr.offset, next_ip) {
                Ok(StepResult::Continue) => {
                    let f = &mut self.frames[depth - 1];
                    if f.ip == instr.offset {
                        f.ip = next_ip;
                    }
                }
                Ok(StepResult::Branch(target)) => self.frames[depth - 1].ip = target,
                Ok(StepResult::EndFilter(v)) => break v,
                Ok(StepResult::Return(_) | StepResult::Throw(_)) | Err(_) => break false,
            }
        };
        self.pins.pop();
        let f = &mut self.frames[depth - 1];
        f.eval_stack.clear();
        f.ip = saved_ip;
        accepted
    }

    fn catches(&self, class_token: u32, exc: &ClrException) -> bool {
        let Some(thrown) = gc::type_of(exc.obj_ptr) else {
            return true;
        };
        match self.resolve_type_token(class_token) {
            Some(target) => types::with_types(|reg| reg.is_subclass_of(thrown, target)),
            None => true,
        }
    }

    /// Le type `target` d'un `isinst`/`castclass` accepte-t-il `obj` ?
    fn is_instance(&self, obj: *mut u8, target: TypeId) -> bool {
        let Some(actual) = gc::type_of(obj) else {
            return true;
        };
        types::with_types(|reg| {
            let Some(t) = reg.get(target) else {
                return true;
            };
            // Les interfaces implémentées ne sont pas encore suivies, ni la
            // hiérarchie des types de la bibliothèque intégrée : on accepte.
            if t.is_interface {
                return true;
            }
            let known = t.def.is_some()
                || target < types::TYPE_EXCEPTION
                || reg.is_subclass_of(target, types::TYPE_EXCEPTION);
            !known || reg.is_subclass_of(actual, target)
        })
    }

    /// Collecte en prenant pour racines tout ce que voient les méthodes en
    /// cours et les champs statiques.
    pub(super) fn collect_garbage(&mut self) {
        let mut roots: Vec<usize> = Vec::new();
        self.own_roots(&mut roots);
        // Les autres fils sont arrêtés à un point sûr (ils n'ont pas la
        // main) : leurs piles sont stables.
        let me = self as *const Interpreter as usize;
        threads::for_each_interpreter(|other| {
            if other as *const Interpreter as usize != me {
                other.own_roots(&mut roots);
            }
        });
        roots.extend(threads::roots());
        roots.extend(super::callbacks::roots());
        for data in self.shared.external_statics.lock().unwrap().values() {
            roots.extend(data.iter().map(|&w| w as usize));
        }
        types::with_types(|reg| {
            for data in reg.static_data.values() {
                roots.extend(data.iter().map(|&w| w as usize));
            }
        });
        gc::with_gc(|gc| gc.collect(&roots));
    }

    /// Racines propres à ce fil : piles d'appels et valeurs épinglées.
    fn own_roots(&self, roots: &mut Vec<usize>) {
        for f in &self.frames {
            roots.extend(f.locals.words.iter().map(|&w| w as usize));
            roots.extend(f.args.words.iter().map(|&w| w as usize));
            roots.extend(f.eval_stack.iter().map(|v| v.as_i64() as usize));
            roots.extend(f.caught.iter().map(|(_, e)| e.obj_ptr as usize));
            roots.extend(f.unwinding.iter().filter_map(|(_, u)| match u {
                Unwind::Exception(e) => Some(e.obj_ptr as usize),
                Unwind::Leave(_) => None,
            }));
        }
        roots.extend(self.pins.iter().map(|v| v.as_i64() as usize));
    }

    /// Adresse d'un champ statique, après le constructeur statique de son type.
    fn static_field(&mut self, token: u32) -> Result<(*mut u8, Kind), ClrException> {
        let Some(field) = self.resolve_field(token) else {
            // Champ statique de la bibliothèque intégrée : emplacement propre
            // au jeton.
            let mut statics = self.shared.external_statics.lock().unwrap();
            let slot = statics
                .entry((self.current, token))
                .or_insert_with(|| Box::new([0u64; 4]));
            return Ok((slot.as_mut_ptr() as *mut u8, Kind::Ref));
        };
        self.ensure_initialized(field.owner)?;
        let ptr = types::with_types(|reg| {
            reg.static_field_ptr(field.owner, field.info.offset, field.info.kind.slot_size())
        });
        Ok((ptr, field.info.kind))
    }

    /// Adresse d'un champ d'instance dans l'objet ou la structure `base`.
    fn instance_field(&self, token: u32, base: *mut u8) -> (*mut u8, Kind) {
        match self.resolve_field(token) {
            Some(f) => (unsafe { base.add(f.info.offset) }, f.info.kind),
            // Champ d'un type de la bibliothèque intégrée : seul l'emplacement
            // réservé (message d'exception) est modélisé.
            None => (unsafe { base.add(exceptions::MESSAGE_SLOT) }, Kind::Ref),
        }
    }

    /// Façon de ranger une valeur du type désigné par un jeton.
    fn kind_of_token(&self, token: u32) -> (TypeId, Kind) {
        let id = self.resolve_type_token(token).unwrap_or(types::TYPE_OBJECT);
        (id, self.kind_of_type(id))
    }

    /// Objet temporaire qui porte une copie d'une valeur de type valeur.
    pub(super) fn new_struct(&self, id: TypeId) -> *mut u8 {
        self.ensure_layout(id);
        let size = types::with_types(|reg| reg.get(id).map_or(8, |t| t.instance_size));
        gc::with_gc(|gc| gc.alloc(id, size))
    }

    fn step(
        &mut self,
        op: OpCode,
        offset: usize,
        next_ip: usize,
    ) -> Result<StepResult, ClrException> {
        let code = self.frames.last().unwrap().code.clone();
        let code = &code[..];

        macro_rules! push {
            ($v:expr) => {{
                let v = $v;
                self.frames.last_mut().unwrap().eval_stack.push(v)
            }};
        }
        macro_rules! pop {
            () => {
                self.frames
                    .last_mut()
                    .unwrap()
                    .eval_stack
                    .pop()
                    .unwrap_or(Value::Null)
            };
        }
        macro_rules! frame {
            () => {
                self.frames.last_mut().unwrap()
            };
        }
        let u8_arg = || opcodes::read_u8(code, offset + 1) as usize;
        let u16_arg = || {
            opcodes::read_u8(code, offset + 2) as usize
                | ((opcodes::read_u8(code, offset + 3) as usize) << 8)
        };
        let tok1 = || opcodes::read_u32(code, offset + 1);
        let tok2 = || opcodes::read_u32(code, offset + 2);
        let br_s = || (next_ip as i64 + opcodes::read_i8(code, offset + 1) as i64) as usize;
        let br_l = || (next_ip as i64 + opcodes::read_i32(code, offset + 1) as i64) as usize;

        use OpCode as O;
        let r = match op {
            O::Nop | O::Break => StepResult::Continue,

            // --- Arguments et variables locales ---
            O::Ldarg_0 | O::Ldarg_1 | O::Ldarg_2 | O::Ldarg_3 => {
                let idx = op as usize - O::Ldarg_0 as usize;
                let v = frame!().args.get(idx);
                push!(v);
                StepResult::Continue
            }
            O::Ldarg_S | O::Ldarg => {
                let idx = if op == O::Ldarg_S {
                    u8_arg()
                } else {
                    u16_arg()
                };
                let v = frame!().args.get(idx);
                push!(v);
                StepResult::Continue
            }
            O::Starg_S | O::Starg => {
                let idx = if op == O::Starg_S {
                    u8_arg()
                } else {
                    u16_arg()
                };
                let v = pop!();
                frame!().args.set(idx, v);
                StepResult::Continue
            }
            O::Ldarga_S | O::Ldarga => {
                let idx = if op == O::Ldarga_S {
                    u8_arg()
                } else {
                    u16_arg()
                };
                let p = frame!()
                    .args
                    .addr(idx)
                    .map_or(Value::Null, |(p, _)| Value::Ref(p));
                push!(p);
                StepResult::Continue
            }
            O::Ldloc_0 | O::Ldloc_1 | O::Ldloc_2 | O::Ldloc_3 => {
                let idx = op as usize - O::Ldloc_0 as usize;
                let v = frame!().locals.get(idx);
                push!(v);
                StepResult::Continue
            }
            O::Ldloc_S | O::Ldloc => {
                let idx = if op == O::Ldloc_S {
                    u8_arg()
                } else {
                    u16_arg()
                };
                let v = frame!().locals.get(idx);
                push!(v);
                StepResult::Continue
            }
            O::Stloc_0 | O::Stloc_1 | O::Stloc_2 | O::Stloc_3 => {
                let idx = op as usize - O::Stloc_0 as usize;
                let v = pop!();
                frame!().locals.set(idx, v);
                StepResult::Continue
            }
            O::Stloc_S | O::Stloc => {
                let idx = if op == O::Stloc_S {
                    u8_arg()
                } else {
                    u16_arg()
                };
                let v = pop!();
                frame!().locals.set(idx, v);
                StepResult::Continue
            }
            O::Ldloca_S | O::Ldloca => {
                let idx = if op == O::Ldloca_S {
                    u8_arg()
                } else {
                    u16_arg()
                };
                let p = frame!()
                    .locals
                    .addr(idx)
                    .map_or(Value::Null, |(p, _)| Value::Ref(p));
                push!(p);
                StepResult::Continue
            }

            // --- Constantes ---
            O::Ldnull => {
                push!(Value::Null);
                StepResult::Continue
            }
            O::Ldc_I4_M1
            | O::Ldc_I4_0
            | O::Ldc_I4_1
            | O::Ldc_I4_2
            | O::Ldc_I4_3
            | O::Ldc_I4_4
            | O::Ldc_I4_5
            | O::Ldc_I4_6
            | O::Ldc_I4_7
            | O::Ldc_I4_8 => {
                push!(Value::I32(op as i32 - O::Ldc_I4_0 as i32));
                StepResult::Continue
            }
            O::Ldc_I4_S => {
                push!(Value::I32(opcodes::read_i8(code, offset + 1) as i32));
                StepResult::Continue
            }
            O::Ldc_I4 => {
                push!(Value::I32(opcodes::read_i32(code, offset + 1)));
                StepResult::Continue
            }
            O::Ldc_I8 => {
                push!(Value::I64(opcodes::read_i64(code, offset + 1)));
                StepResult::Continue
            }
            O::Ldc_R4 => {
                push!(Value::F32(opcodes::read_f32(code, offset + 1)));
                StepResult::Continue
            }
            O::Ldc_R8 => {
                push!(Value::F64(opcodes::read_f64(code, offset + 1)));
                StepResult::Continue
            }

            // --- Pile ---
            O::Dup => {
                let v = frame!().eval_stack.last().copied().unwrap_or(Value::Null);
                push!(v);
                StepResult::Continue
            }
            O::Pop => {
                pop!();
                StepResult::Continue
            }

            // --- Arithmétique et logique ---
            O::Add
            | O::Sub
            | O::Mul
            | O::Div
            | O::Div_Un
            | O::Rem
            | O::Rem_Un
            | O::And
            | O::Or
            | O::Xor
            | O::Add_Ovf
            | O::Add_Ovf_Un
            | O::Sub_Ovf
            | O::Sub_Ovf_Un
            | O::Mul_Ovf
            | O::Mul_Ovf_Un => {
                let b = pop!();
                let a = pop!();
                push!(binary(op, a, b)?);
                StepResult::Continue
            }
            O::Shl | O::Shr | O::Shr_Un => {
                let n = pop!().as_i32() as u32;
                let a = pop!();
                push!(match (a.is_wide(), op) {
                    (true, O::Shl) => Value::I64(a.as_i64().wrapping_shl(n)),
                    (true, O::Shr) => Value::I64(a.as_i64().wrapping_shr(n)),
                    (true, _) => Value::I64((a.as_i64() as u64).wrapping_shr(n) as i64),
                    (false, O::Shl) => Value::I32(a.as_i32().wrapping_shl(n)),
                    (false, O::Shr) => Value::I32(a.as_i32().wrapping_shr(n)),
                    (false, _) => Value::I32((a.as_i32() as u32).wrapping_shr(n) as i32),
                });
                StepResult::Continue
            }
            O::Neg => {
                let v = pop!();
                push!(match v {
                    Value::I32(x) => Value::I32(x.wrapping_neg()),
                    Value::F32(x) => Value::F32(-x),
                    Value::F64(x) => Value::F64(-x),
                    v => Value::I64(v.as_i64().wrapping_neg()),
                });
                StepResult::Continue
            }
            O::Not => {
                let v = pop!();
                push!(match v {
                    Value::I32(x) => Value::I32(!x),
                    v => Value::I64(!v.as_i64()),
                });
                StepResult::Continue
            }

            // --- Comparaisons ---
            O::Ceq | O::Cgt | O::Cgt_Un | O::Clt | O::Clt_Un => {
                let b = pop!();
                let a = pop!();
                let r = match op {
                    O::Ceq => cmp_signed(a, b) == Some(Ordering::Equal),
                    O::Cgt => cmp_signed(a, b) == Some(Ordering::Greater),
                    O::Clt => cmp_signed(a, b) == Some(Ordering::Less),
                    O::Cgt_Un => matches!(cmp_unsigned(a, b), None | Some(Ordering::Greater)),
                    _ => matches!(cmp_unsigned(a, b), None | Some(Ordering::Less)),
                };
                push!(Value::I32(r as i32));
                StepResult::Continue
            }

            // --- Branchements ---
            O::Br_S => StepResult::Branch(br_s()),
            O::Br => StepResult::Branch(br_l()),
            O::Brfalse_S | O::Brtrue_S | O::Brfalse | O::Brtrue => {
                let v = pop!().is_truthy();
                let jump = matches!(op, O::Brtrue_S | O::Brtrue) == v;
                let target = if matches!(op, O::Brfalse_S | O::Brtrue_S) {
                    br_s()
                } else {
                    br_l()
                };
                if jump {
                    StepResult::Branch(target)
                } else {
                    StepResult::Continue
                }
            }
            O::Beq_S
            | O::Bge_S
            | O::Bgt_S
            | O::Ble_S
            | O::Blt_S
            | O::Bne_Un_S
            | O::Bge_Un_S
            | O::Bgt_Un_S
            | O::Ble_Un_S
            | O::Blt_Un_S
            | O::Beq
            | O::Bge
            | O::Bgt
            | O::Ble
            | O::Blt
            | O::Bne_Un
            | O::Bge_Un
            | O::Bgt_Un
            | O::Ble_Un
            | O::Blt_Un => {
                let b = pop!();
                let a = pop!();
                let short = (O::Beq_S as u16..=O::Blt_Un_S as u16).contains(&(op as u16));
                use Ordering::*;
                let s = cmp_signed(a, b);
                let u = cmp_unsigned(a, b);
                let cond = match op {
                    O::Beq_S | O::Beq => s == Some(Equal),
                    O::Bge_S | O::Bge => matches!(s, Some(Greater | Equal)),
                    O::Bgt_S | O::Bgt => s == Some(Greater),
                    O::Ble_S | O::Ble => matches!(s, Some(Less | Equal)),
                    O::Blt_S | O::Blt => s == Some(Less),
                    O::Bne_Un_S | O::Bne_Un => u != Some(Equal),
                    O::Bge_Un_S | O::Bge_Un => matches!(u, None | Some(Greater | Equal)),
                    O::Bgt_Un_S | O::Bgt_Un => matches!(u, None | Some(Greater)),
                    O::Ble_Un_S | O::Ble_Un => matches!(u, None | Some(Less | Equal)),
                    _ => matches!(u, None | Some(Less)),
                };
                if cond {
                    StepResult::Branch(if short { br_s() } else { br_l() })
                } else {
                    StepResult::Continue
                }
            }
            O::Switch => {
                let n = opcodes::read_u32(code, offset + 1) as usize;
                let v = pop!().as_i32() as u32 as usize;
                if v < n {
                    let target_off = opcodes::read_i32(code, offset + 5 + v * 4);
                    StepResult::Branch((next_ip as i64 + target_off as i64) as usize)
                } else {
                    StepResult::Continue
                }
            }

            // --- Conversions ---
            O::Conv_I1
            | O::Conv_I2
            | O::Conv_I4
            | O::Conv_I8
            | O::Conv_U1
            | O::Conv_U2
            | O::Conv_U4
            | O::Conv_U8
            | O::Conv_I
            | O::Conv_U
            | O::Conv_R4
            | O::Conv_R8
            | O::Conv_R_Un => {
                let v = pop!();
                push!(convert(op, v));
                StepResult::Continue
            }
            O::Conv_Ovf_I1
            | O::Conv_Ovf_I2
            | O::Conv_Ovf_I4
            | O::Conv_Ovf_I8
            | O::Conv_Ovf_U1
            | O::Conv_Ovf_U2
            | O::Conv_Ovf_U4
            | O::Conv_Ovf_U8
            | O::Conv_Ovf_I
            | O::Conv_Ovf_U
            | O::Conv_Ovf_I1_Un
            | O::Conv_Ovf_I2_Un
            | O::Conv_Ovf_I4_Un
            | O::Conv_Ovf_I8_Un
            | O::Conv_Ovf_U1_Un
            | O::Conv_Ovf_U2_Un
            | O::Conv_Ovf_U4_Un
            | O::Conv_Ovf_U8_Un
            | O::Conv_Ovf_I_Un
            | O::Conv_Ovf_U_Un => {
                let v = pop!();
                push!(convert_checked(op, v)?);
                StepResult::Continue
            }
            O::Ckfinite => {
                let v = pop!();
                if !v.as_f64().is_finite() {
                    return Err(exception("ArithmeticException", "Number is not finite."));
                }
                push!(v);
                StepResult::Continue
            }

            // --- Chaînes ---
            O::Ldstr => {
                let s = self.assembly.get_user_string(tok1() & 0x00FF_FFFF);
                push!(bcl::alloc_managed_string(&s));
                StepResult::Continue
            }

            // --- Appels ---
            O::Call | O::Callvirt => {
                let token = tok1();
                frame!().ip = next_ip;
                let sig = self.get_method_sig(token);
                let f = frame!();
                let constrained = f.constrained.take();
                let split = f
                    .eval_stack
                    .len()
                    .saturating_sub(sig.param_types.len() + sig.has_this() as usize);
                let mut args = f.eval_stack.split_off(split);
                let mut callee = self.resolve_callee(token);
                let mut virtual_call = op == O::Callvirt;
                if let (Some(ct), Some(this)) = (constrained, args.first().copied()) {
                    let kind = self.kind_of_type(ct);
                    let ptr = this.as_ref();
                    if kind == Kind::Ref {
                        args[0] = load(ptr, Kind::Ref);
                    } else {
                        let name = self.method_name(token);
                        let argc = sig.param_types.len();
                        let own = types::with_types(|reg| {
                            let t = reg.get(ct)?;
                            let (asm, tok) = reg.find_own_method(ct, &name, argc)?;
                            Some((asm, tok, t.type_args.clone()))
                        });
                        if let Some((asm, tok, t)) = own {
                            // Méthode du type valeur lui-même : `this` reste
                            // l'adresse de la valeur.
                            callee = Callee {
                                target: Target::Managed(asm, Token(tok).rid()),
                                gen: Gen { t, m: callee.gen.m },
                            };
                            virtual_call = false;
                        } else if matches!(kind, Kind::Struct(_)) {
                            let boxed = self.new_struct(ct);
                            store(boxed, kind, load(ptr, kind));
                            args[0] = Value::Ref(boxed);
                        } else {
                            args[0] = load(ptr, kind);
                            virtual_call = false;
                        }
                    }
                }
                let v = self.invoke(token, callee, args, virtual_call)?;
                if !matches!(sig.ret_type, ElementType::Void) {
                    push!(v);
                }
                StepResult::Continue
            }
            O::Newobj => {
                let token = tok1();
                frame!().ip = next_ip;
                let sig = self.get_method_sig(token);
                let callee = self.resolve_callee(token);
                let type_id = match &callee.target {
                    Target::Managed(asm, row) => {
                        let info = &self.domain[*asm];
                        let owner = info.method_owner.get(*row as usize).copied().unwrap_or(0);
                        let id = info
                            .type_ids
                            .get(owner as usize)
                            .copied()
                            .flatten()
                            .unwrap_or(types::TYPE_OBJECT);
                        self.instantiate(id, callee.gen.t.clone())
                    }
                    Target::External {
                        namespace, class, ..
                    } => types::with_types(|reg| reg.ensure_external(namespace, class)),
                };
                let f = frame!();
                let split = f.eval_stack.len().saturating_sub(sig.param_types.len());
                let mut args = f.eval_stack.split_off(split);
                let kind = self.kind_of_type(type_id);
                let obj = if kind == Kind::Ref {
                    self.ensure_layout(type_id);
                    let size = types::with_types(|reg| {
                        reg.get(type_id)
                            .map_or(types::EXTERNAL_OBJECT_SIZE, |t| t.instance_size)
                    });
                    gc::with_gc(|gc| gc.alloc(type_id, size.max(16)))
                } else {
                    self.new_struct(type_id)
                };
                args.insert(0, Value::Ref(obj));
                self.pins.push(Value::Ref(obj));
                let result = self.invoke(token, callee, args, false);
                self.pins.pop();
                result?;
                push!(match kind {
                    Kind::Ref | Kind::Struct(_) => Value::Ref(obj),
                    k => load(obj, k),
                });
                StepResult::Continue
            }
            O::Ret => StepResult::Return(pop!()),
            O::Jmp | O::Calli => {
                return Err(exception(
                    "InvalidProgramException",
                    &format!("{op:?} n'est pas encore pris en charge"),
                ));
            }

            // --- Champs ---
            O::Ldfld | O::Ldflda => {
                let token = tok1();
                let obj = pop!().as_ref();
                if obj.is_null() {
                    return Err(ClrException::null_reference());
                }
                let (ptr, kind) = self.instance_field(token, obj);
                push!(if op == O::Ldflda {
                    Value::Ref(ptr)
                } else {
                    load(ptr, kind)
                });
                StepResult::Continue
            }
            O::Stfld => {
                let token = tok1();
                let val = pop!();
                let obj = pop!().as_ref();
                if obj.is_null() {
                    return Err(ClrException::null_reference());
                }
                let (ptr, kind) = self.instance_field(token, obj);
                store(ptr, kind, val);
                StepResult::Continue
            }
            O::Ldsfld | O::Ldsflda => {
                let (ptr, kind) = self.static_field(tok1())?;
                push!(if op == O::Ldsflda {
                    Value::Ref(ptr)
                } else {
                    load(ptr, kind)
                });
                StepResult::Continue
            }
            O::Stsfld => {
                let val = pop!();
                let (ptr, kind) = self.static_field(tok1())?;
                store(ptr, kind, val);
                StepResult::Continue
            }

            // --- Tableaux ---
            O::Newarr => {
                let (elem, kind) = self.kind_of_token(tok1());
                let length = pop!().as_i64();
                if length < 0 {
                    return Err(overflow());
                }
                let elem = if kind == Kind::Ref { 0 } else { elem };
                let ptr =
                    gc::with_gc(|gc| gc.alloc_array_of(elem, kind.elem_size(), length as usize));
                push!(Value::Ref(ptr));
                StepResult::Continue
            }
            O::Ldlen => {
                let arr = pop!().as_ref();
                if arr.is_null() {
                    return Err(ClrException::null_reference());
                }
                push!(Value::I32(unsafe { *(arr as *const u64) } as i32));
                StepResult::Continue
            }
            O::Ldelem_I1
            | O::Ldelem_U1
            | O::Ldelem_I2
            | O::Ldelem_U2
            | O::Ldelem_I4
            | O::Ldelem_U4
            | O::Ldelem_I8
            | O::Ldelem_I
            | O::Ldelem_R4
            | O::Ldelem_R8
            | O::Ldelem_Ref
            | O::Ldelem
            | O::Ldelema => {
                let kind = match op {
                    O::Ldelem_I1 => Kind::I1,
                    O::Ldelem_U1 => Kind::U1,
                    O::Ldelem_I2 => Kind::I2,
                    O::Ldelem_U2 => Kind::U2,
                    O::Ldelem_I4 => Kind::I4,
                    O::Ldelem_U4 => Kind::U4,
                    O::Ldelem_I8 => Kind::I8,
                    O::Ldelem_I => Kind::I,
                    O::Ldelem_R4 => Kind::R4,
                    O::Ldelem_R8 => Kind::R8,
                    O::Ldelem_Ref => Kind::Ref,
                    _ => self.kind_of_token(tok1()).1,
                };
                let idx = pop!().as_i64();
                let arr = pop!().as_ref();
                let ptr = element(arr, idx, kind)?;
                push!(if op == O::Ldelema {
                    Value::Ref(ptr)
                } else {
                    load(ptr, kind)
                });
                StepResult::Continue
            }
            O::Stelem_I
            | O::Stelem_I1
            | O::Stelem_I2
            | O::Stelem_I4
            | O::Stelem_I8
            | O::Stelem_R4
            | O::Stelem_R8
            | O::Stelem_Ref
            | O::Stelem => {
                let kind = match op {
                    O::Stelem_I => Kind::I,
                    O::Stelem_I1 => Kind::I1,
                    O::Stelem_I2 => Kind::I2,
                    O::Stelem_I4 => Kind::I4,
                    O::Stelem_I8 => Kind::I8,
                    O::Stelem_R4 => Kind::R4,
                    O::Stelem_R8 => Kind::R8,
                    O::Stelem_Ref => Kind::Ref,
                    _ => self.kind_of_token(tok1()).1,
                };
                let val = pop!();
                let idx = pop!().as_i64();
                let arr = pop!().as_ref();
                let ptr = element(arr, idx, kind)?;
                store(ptr, kind, val);
                StepResult::Continue
            }

            // --- Indirections ---
            O::Ldind_I1
            | O::Ldind_U1
            | O::Ldind_I2
            | O::Ldind_U2
            | O::Ldind_I4
            | O::Ldind_U4
            | O::Ldind_I8
            | O::Ldind_I
            | O::Ldind_R4
            | O::Ldind_R8
            | O::Ldind_Ref
            | O::Ldobj => {
                let kind = match op {
                    O::Ldind_I1 => Kind::I1,
                    O::Ldind_U1 => Kind::U1,
                    O::Ldind_I2 => Kind::I2,
                    O::Ldind_U2 => Kind::U2,
                    O::Ldind_I4 => Kind::I4,
                    O::Ldind_U4 => Kind::U4,
                    O::Ldind_I8 => Kind::I8,
                    O::Ldind_I => Kind::I,
                    O::Ldind_R4 => Kind::R4,
                    O::Ldind_R8 => Kind::R8,
                    O::Ldind_Ref => Kind::Ref,
                    _ => self.kind_of_token(tok1()).1,
                };
                let ptr = pop!().as_ref();
                if ptr.is_null() {
                    return Err(ClrException::null_reference());
                }
                push!(load(ptr, kind));
                StepResult::Continue
            }
            O::Stind_I1
            | O::Stind_I2
            | O::Stind_I4
            | O::Stind_I8
            | O::Stind_I
            | O::Stind_R4
            | O::Stind_R8
            | O::Stind_Ref
            | O::Stobj => {
                let kind = match op {
                    O::Stind_I1 => Kind::I1,
                    O::Stind_I2 => Kind::I2,
                    O::Stind_I4 => Kind::I4,
                    O::Stind_I8 => Kind::I8,
                    O::Stind_I => Kind::I,
                    O::Stind_R4 => Kind::R4,
                    O::Stind_R8 => Kind::R8,
                    O::Stind_Ref => Kind::Ref,
                    _ => self.kind_of_token(tok1()).1,
                };
                let val = pop!();
                let ptr = pop!().as_ref();
                if ptr.is_null() {
                    return Err(ClrException::null_reference());
                }
                store(ptr, kind, val);
                StepResult::Continue
            }
            O::Cpobj => {
                let (_, kind) = self.kind_of_token(tok1());
                let src = pop!().as_ref();
                let dst = pop!().as_ref();
                if src.is_null() || dst.is_null() {
                    return Err(ClrException::null_reference());
                }
                unsafe { std::ptr::copy(src, dst, kind.elem_size()) };
                StepResult::Continue
            }
            O::Initobj => {
                let (_, kind) = self.kind_of_token(tok2());
                let ptr = pop!().as_ref();
                if ptr.is_null() {
                    return Err(ClrException::null_reference());
                }
                unsafe { std::ptr::write_bytes(ptr, 0, kind.elem_size()) };
                StepResult::Continue
            }
            O::Sizeof => {
                let (_, kind) = self.kind_of_token(tok2());
                push!(Value::I32(kind.elem_size() as i32));
                StepResult::Continue
            }
            O::Cpblk | O::Initblk => {
                let size = pop!().as_i64() as usize;
                let second = pop!();
                let dst = pop!().as_ref();
                if dst.is_null() {
                    return Err(ClrException::null_reference());
                }
                unsafe {
                    if op == O::Cpblk {
                        std::ptr::copy(second.as_ref(), dst, size);
                    } else {
                        std::ptr::write_bytes(dst, second.as_i32() as u8, size);
                    }
                }
                StepResult::Continue
            }
            O::Localloc => {
                let size = pop!().as_i64() as usize;
                let ptr = gc::with_gc(|gc| gc.alloc(0, size));
                push!(Value::Ref(ptr));
                StepResult::Continue
            }

            // --- Objets ---
            O::Castclass | O::Isinst => {
                let (target, _) = self.kind_of_token(tok1());
                let obj = pop!();
                let fits = match obj {
                    Value::Ref(p) => self.is_instance(p, target),
                    _ => true,
                };
                if fits {
                    push!(obj);
                } else if op == O::Isinst {
                    push!(Value::Null);
                } else {
                    return Err(invalid_cast());
                }
                StepResult::Continue
            }
            O::Box => {
                let (id, kind) = self.kind_of_token(tok1());
                let val = pop!();
                let boxed = match kind {
                    Kind::Ref => val,
                    Kind::Struct(_) => {
                        let b = self.new_struct(id);
                        store(b, kind, val);
                        Value::Ref(b)
                    }
                    k => {
                        let b = gc::with_gc(|gc| gc.alloc(id, 8));
                        store(b, k, val);
                        Value::Ref(b)
                    }
                };
                push!(boxed);
                StepResult::Continue
            }
            O::Unbox | O::Unbox_Any => {
                let (id, kind) = self.kind_of_token(tok1());
                let obj = pop!();
                if kind == Kind::Ref {
                    // unbox.any d'un type référence : castclass.
                    if let Value::Ref(p) = obj {
                        if !self.is_instance(p, id) {
                            return Err(invalid_cast());
                        }
                    }
                    push!(obj);
                    return Ok(StepResult::Continue);
                }
                let p = obj.as_ref();
                let Some(actual) = gc::type_of(p) else {
                    return Err(ClrException::null_reference());
                };
                if actual != id && self.kind_of_type(actual) != kind {
                    return Err(invalid_cast());
                }
                push!(if op == O::Unbox {
                    Value::Ref(p)
                } else {
                    load(p, kind)
                });
                StepResult::Continue
            }

            // --- Exceptions ---
            O::Throw => match pop!() {
                Value::Ref(p) if gc::type_of(p).is_some() => {
                    StepResult::Throw(ClrException { obj_ptr: p })
                }
                _ => StepResult::Throw(ClrException::null_reference()),
            },
            O::Rethrow => {
                let f = frame!();
                let handler = exceptions::enclosing_handler(
                    &f.exception_clauses,
                    offset,
                    exceptions::is_catch,
                )
                .map(|c| c.handler_offset as usize);
                let exc = f
                    .caught
                    .iter()
                    .rev()
                    .find(|(h, _)| Some(*h) == handler)
                    .map(|(_, e)| *e);
                match exc {
                    Some(exc) => StepResult::Throw(exc),
                    None => {
                        return Err(exception(
                            "InvalidProgramException",
                            "rethrow hors d'un gestionnaire catch",
                        ))
                    }
                }
            }
            O::Leave | O::Leave_S => {
                let target = if op == O::Leave_S { br_s() } else { br_l() };
                let f = frame!();
                f.eval_stack.clear();
                let clauses = f.exception_clauses.clone();
                // Sortie des catch qui contiennent le `leave` sans contenir
                // sa destination.
                f.caught.retain(|(h, _)| {
                    !clauses.iter().any(|c| {
                        c.handler_offset as usize == *h
                            && exceptions::handler_covers(c, offset)
                            && !exceptions::handler_covers(c, target)
                    })
                });
                match exceptions::find_leave_target(&clauses, offset, target) {
                    Some(handler) => {
                        f.unwinding.push((handler, Unwind::Leave(target)));
                        StepResult::Branch(handler)
                    }
                    None => StepResult::Branch(target),
                }
            }
            O::Endfinally => {
                let f = frame!();
                let clauses = f.exception_clauses.clone();
                let Some(handler) =
                    exceptions::enclosing_handler(&clauses, offset, exceptions::is_finally)
                        .map(|c| c.handler_offset as usize)
                else {
                    return Err(exception(
                        "InvalidProgramException",
                        "endfinally hors d'un bloc finally",
                    ));
                };
                let entry = f
                    .unwinding
                    .iter()
                    .rposition(|(h, _)| *h == handler)
                    .map(|i| f.unwinding.remove(i).1);
                match entry {
                    Some(Unwind::Exception(exc)) => StepResult::Throw(exc),
                    Some(Unwind::Leave(target)) => {
                        match exceptions::find_leave_target(&clauses, offset, target) {
                            Some(next) => {
                                f.unwinding.push((next, Unwind::Leave(target)));
                                StepResult::Branch(next)
                            }
                            None => StepResult::Branch(target),
                        }
                    }
                    None => StepResult::Continue,
                }
            }
            O::Endfilter => StepResult::EndFilter(pop!().as_i32() != 0),

            // --- Jetons ---
            O::Ldtoken => {
                let token = tok1();
                let handle = match self.resolve_type_token(token) {
                    Some(id) => id as i64,
                    None => token as i64,
                };
                push!(Value::I64(handle));
                StepResult::Continue
            }

            // --- Pointeurs de méthode ---
            O::Ldftn => {
                let token = tok2();
                let callee = self.resolve_callee(token);
                push!(Value::I64(self.fn_handle(token, callee)));
                StepResult::Continue
            }
            O::Ldvirtftn => {
                let token = tok2();
                let Callee {
                    mut target,
                    mut gen,
                } = self.resolve_callee(token);
                match pop!() {
                    Value::Ref(this) => self.devirtualize(token, this, &mut target, &mut gen),
                    Value::Null => return Err(ClrException::null_reference()),
                    _ => {}
                }
                push!(Value::I64(self.fn_handle(token, Callee { target, gen })));
                StepResult::Continue
            }

            // --- Préfixes ---
            O::Constrained => {
                let ct = self.resolve_type_token(tok2());
                frame!().constrained = ct;
                StepResult::Continue
            }
            O::Volatile | O::Unaligned | O::Tail | O::Readonly => StepResult::Continue,

            _ => {
                crate::log_warn!("clr-il", "opcode non implémenté : {:?} à IP={}", op, offset);
                return Err(exception(
                    "InvalidProgramException",
                    &format!("{op:?} n'est pas encore pris en charge"),
                ));
            }
        };
        Ok(r)
    }
}

/// Adresse d'un élément de tableau, après vérification de l'indice.
fn element(arr: *mut u8, idx: i64, kind: Kind) -> Result<*mut u8, ClrException> {
    if arr.is_null() {
        return Err(ClrException::null_reference());
    }
    let len = unsafe { *(arr as *const u64) } as i64;
    if idx < 0 || idx >= len {
        return Err(ClrException::index_out_of_range());
    }
    Ok(unsafe { arr.add(8 + idx as usize * kind.elem_size()) })
}

/// Comparaison signée (ordre partiel pour les flottants : NaN n'est pas
/// ordonné).
fn cmp_signed(a: Value, b: Value) -> Option<Ordering> {
    if a.is_float() || b.is_float() {
        return a.as_f64().partial_cmp(&b.as_f64());
    }
    Some(a.as_i64().cmp(&b.as_i64()))
}

/// Comparaison non signée (ou non ordonnée pour les flottants).
fn cmp_unsigned(a: Value, b: Value) -> Option<Ordering> {
    if a.is_float() || b.is_float() {
        return a.as_f64().partial_cmp(&b.as_f64());
    }
    if a.is_wide() || b.is_wide() {
        Some((a.as_i64() as u64).cmp(&(b.as_i64() as u64)))
    } else {
        Some((a.as_i32() as u32).cmp(&(b.as_i32() as u32)))
    }
}

/// Opération binaire (ECMA-335 §III.1.5) : flottant si l'un des opérandes
/// l'est, 64 bits si l'un l'est (ou est un pointeur), sinon 32 bits.
fn binary(op: OpCode, a: Value, b: Value) -> Result<Value, ClrException> {
    use OpCode as O;
    if a.is_float() || b.is_float() {
        let (x, y) = (a.as_f64(), b.as_f64());
        let r = match op {
            O::Add | O::Add_Ovf | O::Add_Ovf_Un => x + y,
            O::Sub | O::Sub_Ovf | O::Sub_Ovf_Un => x - y,
            O::Mul | O::Mul_Ovf | O::Mul_Ovf_Un => x * y,
            O::Div | O::Div_Un => x / y,
            O::Rem | O::Rem_Un => x % y,
            _ => {
                return Err(exception(
                    "InvalidProgramException",
                    "opération logique sur un flottant",
                ))
            }
        };
        return Ok(if matches!((a, b), (Value::F32(_), Value::F32(_))) {
            Value::F32(r as f32)
        } else {
            Value::F64(r)
        });
    }
    let divide_by_zero = || ClrException::divide_by_zero();
    let arith = || {
        exception(
            "ArithmeticException",
            "Arithmetic operation resulted in an overflow.",
        )
    };
    if a.is_wide() || b.is_wide() {
        let (x, y) = (a.as_i64(), b.as_i64());
        let (ux, uy) = (x as u64, y as u64);
        let r = match op {
            O::Add => x.wrapping_add(y),
            O::Sub => x.wrapping_sub(y),
            O::Mul => x.wrapping_mul(y),
            O::Div => {
                if y == 0 {
                    return Err(divide_by_zero());
                }
                x.checked_div(y).ok_or_else(arith)?
            }
            O::Rem => {
                if y == 0 {
                    return Err(divide_by_zero());
                }
                x.checked_rem(y).ok_or_else(arith)?
            }
            O::Div_Un => ux.checked_div(uy).ok_or_else(divide_by_zero)? as i64,
            O::Rem_Un => ux.checked_rem(uy).ok_or_else(divide_by_zero)? as i64,
            O::And => x & y,
            O::Or => x | y,
            O::Xor => x ^ y,
            O::Add_Ovf => x.checked_add(y).ok_or_else(overflow)?,
            O::Sub_Ovf => x.checked_sub(y).ok_or_else(overflow)?,
            O::Mul_Ovf => x.checked_mul(y).ok_or_else(overflow)?,
            O::Add_Ovf_Un => ux.checked_add(uy).ok_or_else(overflow)? as i64,
            O::Sub_Ovf_Un => ux.checked_sub(uy).ok_or_else(overflow)? as i64,
            _ => ux.checked_mul(uy).ok_or_else(overflow)? as i64,
        };
        return Ok(Value::I64(r));
    }
    let (x, y) = (a.as_i32(), b.as_i32());
    let (ux, uy) = (x as u32, y as u32);
    let r = match op {
        O::Add => x.wrapping_add(y),
        O::Sub => x.wrapping_sub(y),
        O::Mul => x.wrapping_mul(y),
        O::Div => {
            if y == 0 {
                return Err(divide_by_zero());
            }
            x.checked_div(y).ok_or_else(arith)?
        }
        O::Rem => {
            if y == 0 {
                return Err(divide_by_zero());
            }
            x.checked_rem(y).ok_or_else(arith)?
        }
        O::Div_Un => ux.checked_div(uy).ok_or_else(divide_by_zero)? as i32,
        O::Rem_Un => ux.checked_rem(uy).ok_or_else(divide_by_zero)? as i32,
        O::And => x & y,
        O::Or => x | y,
        O::Xor => x ^ y,
        O::Add_Ovf => x.checked_add(y).ok_or_else(overflow)?,
        O::Sub_Ovf => x.checked_sub(y).ok_or_else(overflow)?,
        O::Mul_Ovf => x.checked_mul(y).ok_or_else(overflow)?,
        O::Add_Ovf_Un => ux.checked_add(uy).ok_or_else(overflow)? as i32,
        O::Sub_Ovf_Un => ux.checked_sub(uy).ok_or_else(overflow)? as i32,
        _ => ux.checked_mul(uy).ok_or_else(overflow)? as i32,
    };
    Ok(Value::I32(r))
}

/// Entier porté par une valeur, flottant tronqué vers zéro.
fn integer(v: Value) -> i64 {
    match v {
        Value::F32(f) => f as i64,
        Value::F64(f) => f as i64,
        v => v.as_i64(),
    }
}

/// Conversions sans contrôle (`conv.*`) : les `u` étendent par des zéros,
/// les `i` par le signe.
fn convert(op: OpCode, v: Value) -> Value {
    use OpCode as O;
    let unsigned_source = |v: Value| -> u64 {
        match v {
            Value::I32(x) => x as u32 as u64,
            Value::F32(f) => f as u64,
            Value::F64(f) => f as u64,
            v => v.as_i64() as u64,
        }
    };
    match op {
        O::Conv_I1 => Value::I32(integer(v) as i8 as i32),
        O::Conv_I2 => Value::I32(integer(v) as i16 as i32),
        O::Conv_I4 => Value::I32(integer(v) as i32),
        O::Conv_U1 => Value::I32(integer(v) as u8 as i32),
        O::Conv_U2 => Value::I32(integer(v) as u16 as i32),
        O::Conv_U4 => Value::I32(unsigned_source(v) as u32 as i32),
        O::Conv_I8 | O::Conv_I => Value::I64(integer(v)),
        O::Conv_U8 | O::Conv_U => Value::I64(unsigned_source(v) as i64),
        O::Conv_R4 => Value::F32(v.as_f64() as f32),
        O::Conv_R8 => Value::F64(v.as_f64()),
        _ => Value::F64(match v {
            Value::I32(x) => x as u32 as f64,
            v if v.is_float() => v.as_f64(),
            v => v.as_i64() as u64 as f64,
        }),
    }
}

/// Conversions avec contrôle de débordement (`conv.ovf.*`).
fn convert_checked(op: OpCode, v: Value) -> Result<Value, ClrException> {
    use OpCode as O;
    let unsigned_source = matches!(
        op,
        O::Conv_Ovf_I1_Un
            | O::Conv_Ovf_I2_Un
            | O::Conv_Ovf_I4_Un
            | O::Conv_Ovf_I8_Un
            | O::Conv_Ovf_U1_Un
            | O::Conv_Ovf_U2_Un
            | O::Conv_Ovf_U4_Un
            | O::Conv_Ovf_U8_Un
            | O::Conv_Ovf_I_Un
            | O::Conv_Ovf_U_Un
    );
    let x: i128 = match v {
        Value::F32(_) | Value::F64(_) => {
            let f = v.as_f64().trunc();
            if !f.is_finite() || f.abs() > 1e20 {
                return Err(overflow());
            }
            f as i128
        }
        Value::I32(i) if unsigned_source => i as u32 as i128,
        v if unsigned_source => v.as_i64() as u64 as i128,
        v => v.as_i64() as i128,
    };
    let (lo, hi): (i128, i128) = match op {
        O::Conv_Ovf_I1 | O::Conv_Ovf_I1_Un => (i8::MIN as i128, i8::MAX as i128),
        O::Conv_Ovf_I2 | O::Conv_Ovf_I2_Un => (i16::MIN as i128, i16::MAX as i128),
        O::Conv_Ovf_I4 | O::Conv_Ovf_I4_Un => (i32::MIN as i128, i32::MAX as i128),
        O::Conv_Ovf_U1 | O::Conv_Ovf_U1_Un => (0, u8::MAX as i128),
        O::Conv_Ovf_U2 | O::Conv_Ovf_U2_Un => (0, u16::MAX as i128),
        O::Conv_Ovf_U4 | O::Conv_Ovf_U4_Un => (0, u32::MAX as i128),
        O::Conv_Ovf_U8 | O::Conv_Ovf_U8_Un | O::Conv_Ovf_U | O::Conv_Ovf_U_Un => {
            (0, u64::MAX as i128)
        }
        _ => (i64::MIN as i128, i64::MAX as i128),
    };
    if x < lo || x > hi {
        return Err(overflow());
    }
    Ok(match op {
        O::Conv_Ovf_I8
        | O::Conv_Ovf_I8_Un
        | O::Conv_Ovf_U8
        | O::Conv_Ovf_U8_Un
        | O::Conv_Ovf_I
        | O::Conv_Ovf_I_Un
        | O::Conv_Ovf_U
        | O::Conv_Ovf_U_Un => Value::I64(x as i64),
        _ => Value::I32(x as i64 as i32),
    })
}
