//! Interpréteur CIL (Common Intermediate Language).
//!
//! Machine à pile qui exécute le bytecode .NET instruction par instruction.
//! Chaque appel managé crée un `CallFrame` et s'exécute dans son propre
//! `run_loop`, qui rend la main à l'appelant au `ret` de la méthode.

use super::bcl;
use super::exceptions::{self, ClrException, Handler};
use super::gc;
use super::metadata::{self, Assembly, ElementType};
use super::opcodes::{self, OpCode};
use super::pinvoke;
use super::tables::{CodedIndex, TableId, Token};
use super::types;

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

    fn default_for(et: &ElementType) -> Value {
        match et {
            ElementType::Boolean
            | ElementType::I1
            | ElementType::U1
            | ElementType::I2
            | ElementType::U2
            | ElementType::I4
            | ElementType::U4
            | ElementType::Char => Value::I32(0),
            ElementType::I8 | ElementType::U8 => Value::I64(0),
            ElementType::R4 => Value::F32(0.0),
            ElementType::R8 => Value::F64(0.0),
            ElementType::IntPtr | ElementType::UIntPtr => Value::I64(0),
            _ => Value::Null,
        }
    }
}

/// Lit un emplacement de 8 octets (champ d'objet ou statique) selon son type.
fn load_slot(ptr: *const u8, et: &ElementType) -> Value {
    unsafe {
        match et {
            ElementType::R4 => Value::F32(*(ptr as *const f32)),
            ElementType::R8 => Value::F64(*(ptr as *const f64)),
            ElementType::I8 | ElementType::U8 | ElementType::IntPtr | ElementType::UIntPtr => {
                Value::I64(*(ptr as *const i64))
            }
            ElementType::Boolean
            | ElementType::Char
            | ElementType::I1
            | ElementType::U1
            | ElementType::I2
            | ElementType::U2
            | ElementType::I4
            | ElementType::U4 => Value::I32(*(ptr as *const i32)),
            _ => match *(ptr as *const *mut u8) {
                p if p.is_null() => Value::Null,
                p => Value::Ref(p),
            },
        }
    }
}

fn store_slot(ptr: *mut u8, val: Value) {
    unsafe {
        match val {
            Value::F32(v) => *(ptr as *mut f32) = v,
            Value::F64(v) => *(ptr as *mut f64) = v,
            Value::Ref(p) => *(ptr as *mut *mut u8) = p,
            Value::Null => *(ptr as *mut u64) = 0,
            v => *(ptr as *mut i64) = v.as_i64(),
        }
    }
}

fn check_index(arr: *mut u8, idx: usize) -> Result<(), ClrException> {
    if arr.is_null() {
        return Err(ClrException::null_reference());
    }
    if idx >= unsafe { *(arr as *const u64) } as usize {
        return Err(ClrException::index_out_of_range());
    }
    Ok(())
}

struct CallFrame {
    ip: usize,
    code: Vec<u8>,
    locals: Vec<Value>,
    args: Vec<Value>,
    eval_stack: Vec<Value>,
    exception_clauses: Vec<metadata::ExceptionClause>,
    /// Exception à relancer à la fin du finally en cours.
    pending: Option<ClrException>,
    /// Destination d'un `leave` en attente de la fin d'un finally.
    leave_target: Option<usize>,
    /// Exception traitée par le catch en cours (pour `rethrow`).
    caught: Option<ClrException>,
}

pub struct Interpreter {
    frames: Vec<CallFrame>,
    pub assembly: Assembly,
}

pub enum ExecResult {
    Return(Value),
    Exception(ClrException),
}

/// Méthode désignée par un jeton d'appel.
enum Target {
    Managed(u32),
    External {
        namespace: String,
        class: String,
        name: String,
    },
}

enum StepResult {
    Continue,
    Branch(usize),
    Return(Value),
    Throw(ClrException),
}

impl Interpreter {
    pub fn new(assembly: Assembly) -> Self {
        Interpreter {
            frames: Vec::new(),
            assembly,
        }
    }

    pub fn execute_method(&mut self, method_row: u32, args: Vec<Value>) -> ExecResult {
        let md = self.assembly.get_method_def(method_row);
        let sig = metadata::decode_method_sig(self.assembly.get_blob(md.signature));

        if md.is_pinvoke() {
            let token = Token::new(TableId::MethodDef, method_row);
            let Some(imp) = self.assembly.find_impl_map(token) else {
                return ExecResult::Exception(ClrException::new(
                    "System",
                    "EntryPointNotFoundException",
                    "P/Invoke sans ImplMap",
                ));
            };
            let module = self
                .assembly
                .get_string(self.assembly.get_module_ref(imp.import_scope).name);
            let import = pinvoke::Import {
                module,
                name: self.assembly.get_string(imp.import_name),
                unicode: imp.mapping_flags & 0x0004 != 0,
            };
            return match pinvoke::call(&import, &args, &sig.param_types, &sig.ret_type) {
                Ok(v) => ExecResult::Return(v),
                Err((kind, msg)) => ExecResult::Exception(ClrException::new("System", kind, &msg)),
            };
        }

        if md.rva == 0 {
            let td = self
                .assembly
                .get_type_def(self.find_declaring_type(method_row));
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
            return ExecResult::Exception(ClrException::new(
                "System",
                "BadImageFormatException",
                &format!("corps de méthode illisible (RVA {:#x})", md.rva),
            ));
        };
        let locals = if body.local_var_sig_token != 0 {
            let sig = self
                .assembly
                .get_standalone_sig(body.local_var_sig_token & 0x00FF_FFFF);
            metadata::decode_locals_sig(self.assembly.get_blob(sig.signature))
                .iter()
                .map(Value::default_for)
                .collect()
        } else {
            Vec::new()
        };

        self.frames.push(CallFrame {
            ip: 0,
            code: body.code,
            locals,
            args,
            eval_stack: Vec::with_capacity(body.max_stack as usize),
            exception_clauses: body.exception_clauses,
            pending: None,
            leave_target: None,
            caught: None,
        });
        self.run_loop()
    }

    /// Appelle la méthode désignée par `token`. Avec `virtual_call`, la
    /// redéfinition du type réel de `this` est choisie.
    fn invoke(
        &mut self,
        token: u32,
        args: Vec<Value>,
        virtual_call: bool,
    ) -> Result<Value, ClrException> {
        let mut target = self.resolve_method(token);
        if virtual_call {
            if let Some(Value::Ref(this)) = args.first() {
                if let Some(type_id) = gc::type_of(*this) {
                    let (name, argc) = self.method_name_and_arity(token);
                    let row = types::with_types(|reg| reg.find_override(type_id, &name, argc));
                    if let Some(row) = row {
                        target = Target::Managed(Token(row).rid());
                    }
                }
            } else if matches!(args.first(), Some(Value::Null)) {
                return Err(ClrException::null_reference());
            }
        }
        match target {
            Target::Managed(row) => match self.execute_method(row, args) {
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

    fn resolve_method(&self, token: u32) -> Target {
        let row = Token(token).rid();
        match Token(token).table() {
            t if t == TableId::MethodDef as u8 => Target::Managed(row),
            t if t == TableId::MethodSpec as u8 => {
                let spec = self.assembly.get_method_spec(row);
                let (table, inner) = CodedIndex::MethodDefOrRef.decode(spec.method);
                let table = table.map_or(0, |t| t as u32);
                self.resolve_method((table << 24) | inner)
            }
            t if t == TableId::MemberRef as u8 => {
                let mr = self.assembly.get_member_ref(row);
                let name = self.assembly.get_string(mr.name).to_string();
                let (parent, prow) = CodedIndex::MemberRefParent.decode(mr.class);
                let (namespace, class) = match parent {
                    Some(TableId::TypeRef) => {
                        let tr = self.assembly.get_type_ref(prow);
                        (tr.namespace, tr.name)
                    }
                    Some(TableId::TypeDef) => {
                        let found = self.assembly.type_methods(prow).find(|&m| {
                            self.assembly
                                .get_string(self.assembly.get_method_def(m).name)
                                == name
                        });
                        if let Some(m) = found {
                            return Target::Managed(m);
                        }
                        let td = self.assembly.get_type_def(prow);
                        (td.namespace, td.name)
                    }
                    Some(TableId::TypeSpec) => {
                        let (ns, cls) = self.type_spec_name(prow);
                        return Target::External {
                            namespace: ns,
                            class: cls,
                            name,
                        };
                    }
                    _ => (0, 0),
                };
                Target::External {
                    namespace: self.assembly.get_string(namespace).to_string(),
                    class: self.assembly.get_string(class).to_string(),
                    name,
                }
            }
            _ => Target::External {
                namespace: String::new(),
                class: String::new(),
                name: format!("jeton {token:#010x}"),
            },
        }
    }

    /// Nom du type générique d'une instanciation (`List`1` pour `List<int>`).
    fn type_spec_name(&self, row: u32) -> (String, String) {
        let blob = self
            .assembly
            .get_blob(self.assembly.get_type_spec(row).signature);
        let mut off = 0;
        let coded = match metadata::decode_type(blob, &mut off) {
            ElementType::GenericInst(inner, _) => match *inner {
                ElementType::Class(c) | ElementType::ValueType(c) => c,
                _ => 0,
            },
            ElementType::Class(c) | ElementType::ValueType(c) => c,
            _ => 0,
        };
        let (table, r) = CodedIndex::TypeDefOrRef.decode(coded);
        let (ns, name) = match table {
            Some(TableId::TypeRef) => {
                let tr = self.assembly.get_type_ref(r);
                (tr.namespace, tr.name)
            }
            Some(TableId::TypeDef) => {
                let td = self.assembly.get_type_def(r);
                (td.namespace, td.name)
            }
            _ => (0, 0),
        };
        (
            self.assembly.get_string(ns).to_string(),
            self.assembly.get_string(name).to_string(),
        )
    }

    fn method_name_and_arity(&self, token: u32) -> (String, usize) {
        let row = Token(token).rid();
        let name = match Token(token).table() {
            t if t == TableId::MethodDef as u8 => self.assembly.get_method_def(row).name,
            t if t == TableId::MemberRef as u8 => self.assembly.get_member_ref(row).name,
            _ => 0,
        };
        (
            self.assembly.get_string(name).to_string(),
            self.get_method_sig(token).param_types.len(),
        )
    }

    /// Méthode implémentée par le runtime (BCL intégrée).
    fn call_native(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: Vec<Value>,
        params: &[ElementType],
    ) -> Result<Value, ClrException> {
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
            ("System", "GC", "Collect") => {
                self.collect_garbage();
                return Ok(Value::Null);
            }
            _ => {}
        }
        let mut args = args;
        let first_param = args.len() - params.len().min(args.len());
        for (i, param) in params.iter().enumerate() {
            if matches!(param, ElementType::Object) {
                args[first_param + i] = self.managed_to_string(args[first_param + i])?;
            }
        }
        bcl::dispatch_internal_call(namespace, class, name, &args, params).ok_or_else(|| {
            ClrException::new(
                "System",
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
        match types::with_types(|reg| reg.find_override(type_id, "ToString", 0)) {
            Some(token) => match self.execute_method(Token(token).rid(), vec![v]) {
                ExecResult::Return(s) => Ok(s),
                ExecResult::Exception(e) => Err(e),
            },
            None => Ok(v),
        }
    }

    fn find_declaring_type(&self, method_row: u32) -> u32 {
        (1..=self.assembly.row_count(TableId::TypeDef))
            .find(|&i| self.assembly.type_methods(i).contains(&method_row))
            .unwrap_or(0)
    }

    /// Exécute la méthode au sommet de la pile d'appels jusqu'à son retour.
    fn run_loop(&mut self) -> ExecResult {
        loop {
            if gc::with_gc(|gc| gc.should_collect()) {
                self.collect_garbage();
            }
            let frame = self.frames.last().unwrap();
            if frame.ip >= frame.code.len() {
                let ret = self.frames.pop().unwrap().eval_stack.pop();
                return ExecResult::Return(ret.unwrap_or(Value::Null));
            }
            let Some(instr) = opcodes::decode(&frame.code, frame.ip) else {
                let ip = frame.ip;
                self.frames.pop();
                return ExecResult::Exception(ClrException::new(
                    "System",
                    "InvalidProgramException",
                    &format!("opcode invalide à IL_{ip:04x}"),
                ));
            };
            let next_ip = instr.offset + instr.size;

            match self.step(instr.op, instr.offset, next_ip) {
                StepResult::Continue => {
                    let f = self.frames.last_mut().unwrap();
                    if f.ip == instr.offset {
                        f.ip = next_ip;
                    }
                }
                StepResult::Branch(target) => self.frames.last_mut().unwrap().ip = target,
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
        let clauses = std::mem::take(&mut self.frames.last_mut().unwrap().exception_clauses);
        let handler = exceptions::find_handler(&clauses, ip, |class| self.catches(class, &exc));
        let f = self.frames.last_mut().unwrap();
        f.exception_clauses = clauses;
        f.eval_stack.clear();
        match handler {
            Some(Handler::Catch(h)) => {
                f.eval_stack.push(Value::Ref(exc.obj_ptr));
                f.caught = Some(exc);
                f.ip = h;
                None
            }
            Some(Handler::Finally(h)) => {
                f.pending = Some(exc);
                f.leave_target = None;
                f.ip = h;
                None
            }
            None => Some(exc),
        }
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

    /// Type désigné par un jeton TypeDef / TypeRef (les types de la BCL sont
    /// créés à la demande).
    fn resolve_type_token(&self, token: u32) -> Option<types::TypeId> {
        let row = Token(token).rid();
        let (ns, name) = match Token(token).table() {
            t if t == TableId::TypeDef as u8 => {
                let td = self.assembly.get_type_def(row);
                (td.namespace, td.name)
            }
            t if t == TableId::TypeRef as u8 => {
                let tr = self.assembly.get_type_ref(row);
                (tr.namespace, tr.name)
            }
            _ => return None,
        };
        let ns = self.assembly.get_string(ns);
        let name = self.assembly.get_string(name);
        Some(types::with_types(|reg| reg.ensure_external(ns, name)))
    }

    fn is_interface_token(&self, token: u32) -> bool {
        Token(token).table() == TableId::TypeDef as u8
            && self
                .assembly
                .get_type_def(Token(token).rid())
                .is_interface()
    }

    /// Collecte en prenant pour racines tout ce que voient les méthodes en
    /// cours et les champs statiques.
    fn collect_garbage(&mut self) {
        let mut roots = Vec::new();
        for f in &self.frames {
            let values = f.locals.iter().chain(&f.args).chain(&f.eval_stack);
            roots.extend(values.map(|v| v.as_i64() as usize));
            let held = f.pending.iter().chain(&f.caught);
            roots.extend(held.map(|e| e.obj_ptr as usize));
        }
        types::with_types(|reg| {
            for data in reg.static_data.values() {
                roots.extend(
                    data.chunks_exact(8)
                        .map(|c| usize::from_le_bytes(c.try_into().unwrap())),
                );
            }
        });
        gc::with_gc(|gc| gc.collect(&roots));
    }

    fn step(&mut self, op: OpCode, offset: usize, next_ip: usize) -> StepResult {
        let f = self.frames.last_mut().unwrap();
        let code = &f.code;

        macro_rules! push {
            ($v:expr) => {
                f.eval_stack.push($v)
            };
        }
        macro_rules! pop {
            () => {
                f.eval_stack.pop().unwrap_or(Value::Null)
            };
        }

        match op {
            OpCode::Nop | OpCode::Break => StepResult::Continue,

            // --- Chargement d'arguments ---
            OpCode::Ldarg_0 => {
                push!(f.args.first().copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldarg_1 => {
                push!(f.args.get(1).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldarg_2 => {
                push!(f.args.get(2).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldarg_3 => {
                push!(f.args.get(3).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldarg_S => {
                let idx = opcodes::read_u8(code, offset + 1) as usize;
                push!(f.args.get(idx).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldarg => {
                let idx = opcodes::read_u8(code, offset + 2) as usize
                    | ((opcodes::read_u8(code, offset + 3) as usize) << 8);
                push!(f.args.get(idx).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Starg_S => {
                let idx = opcodes::read_u8(code, offset + 1) as usize;
                let v = pop!();
                if idx < f.args.len() {
                    f.args[idx] = v;
                }
                StepResult::Continue
            }

            // --- Variables locales ---
            OpCode::Ldloc_0 => {
                push!(f.locals.first().copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldloc_1 => {
                push!(f.locals.get(1).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldloc_2 => {
                push!(f.locals.get(2).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldloc_3 => {
                push!(f.locals.get(3).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Ldloc_S => {
                let idx = opcodes::read_u8(code, offset + 1) as usize;
                push!(f.locals.get(idx).copied().unwrap_or(Value::Null));
                StepResult::Continue
            }
            OpCode::Stloc_0 => {
                let v = pop!();
                if !f.locals.is_empty() {
                    f.locals[0] = v;
                }
                StepResult::Continue
            }
            OpCode::Stloc_1 => {
                let v = pop!();
                if f.locals.len() > 1 {
                    f.locals[1] = v;
                }
                StepResult::Continue
            }
            OpCode::Stloc_2 => {
                let v = pop!();
                if f.locals.len() > 2 {
                    f.locals[2] = v;
                }
                StepResult::Continue
            }
            OpCode::Stloc_3 => {
                let v = pop!();
                if f.locals.len() > 3 {
                    f.locals[3] = v;
                }
                StepResult::Continue
            }
            OpCode::Stloc_S => {
                let idx = opcodes::read_u8(code, offset + 1) as usize;
                let v = pop!();
                if idx < f.locals.len() {
                    f.locals[idx] = v;
                }
                StepResult::Continue
            }

            // --- Constantes ---
            OpCode::Ldnull => {
                push!(Value::Null);
                StepResult::Continue
            }
            OpCode::Ldc_I4_M1 => {
                push!(Value::I32(-1));
                StepResult::Continue
            }
            OpCode::Ldc_I4_0 => {
                push!(Value::I32(0));
                StepResult::Continue
            }
            OpCode::Ldc_I4_1 => {
                push!(Value::I32(1));
                StepResult::Continue
            }
            OpCode::Ldc_I4_2 => {
                push!(Value::I32(2));
                StepResult::Continue
            }
            OpCode::Ldc_I4_3 => {
                push!(Value::I32(3));
                StepResult::Continue
            }
            OpCode::Ldc_I4_4 => {
                push!(Value::I32(4));
                StepResult::Continue
            }
            OpCode::Ldc_I4_5 => {
                push!(Value::I32(5));
                StepResult::Continue
            }
            OpCode::Ldc_I4_6 => {
                push!(Value::I32(6));
                StepResult::Continue
            }
            OpCode::Ldc_I4_7 => {
                push!(Value::I32(7));
                StepResult::Continue
            }
            OpCode::Ldc_I4_8 => {
                push!(Value::I32(8));
                StepResult::Continue
            }
            OpCode::Ldc_I4_S => {
                push!(Value::I32(opcodes::read_i8(code, offset + 1) as i32));
                StepResult::Continue
            }
            OpCode::Ldc_I4 => {
                push!(Value::I32(opcodes::read_i32(code, offset + 1)));
                StepResult::Continue
            }
            OpCode::Ldc_I8 => {
                push!(Value::I64(opcodes::read_i64(code, offset + 1)));
                StepResult::Continue
            }
            OpCode::Ldc_R4 => {
                push!(Value::F32(opcodes::read_f32(code, offset + 1)));
                StepResult::Continue
            }
            OpCode::Ldc_R8 => {
                push!(Value::F64(opcodes::read_f64(code, offset + 1)));
                StepResult::Continue
            }

            // --- Pile ---
            OpCode::Dup => {
                let v = f.eval_stack.last().copied().unwrap_or(Value::Null);
                push!(v);
                StepResult::Continue
            }
            OpCode::Pop => {
                pop!();
                StepResult::Continue
            }

            // --- Arithmétique ---
            OpCode::Add => {
                let b = pop!();
                let a = pop!();
                push!(arith_add(a, b));
                StepResult::Continue
            }
            OpCode::Sub => {
                let b = pop!();
                let a = pop!();
                push!(arith_sub(a, b));
                StepResult::Continue
            }
            OpCode::Mul => {
                let b = pop!();
                let a = pop!();
                push!(arith_mul(a, b));
                StepResult::Continue
            }
            OpCode::Div => {
                let b = pop!();
                let a = pop!();
                if matches!(b, Value::I32(0) | Value::I64(0)) {
                    return StepResult::Throw(ClrException::divide_by_zero());
                }
                push!(arith_div(a, b));
                StepResult::Continue
            }
            OpCode::Div_Un => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(
                    (a.as_i32() as u32).wrapping_div(b.as_i32().max(1) as u32) as i32
                ));
                StepResult::Continue
            }
            OpCode::Rem => {
                let b = pop!();
                let a = pop!();
                if matches!(b, Value::I32(0) | Value::I64(0)) {
                    return StepResult::Throw(ClrException::divide_by_zero());
                }
                push!(arith_rem(a, b));
                StepResult::Continue
            }
            OpCode::Rem_Un => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(
                    (a.as_i32() as u32).wrapping_rem(b.as_i32().max(1) as u32) as i32
                ));
                StepResult::Continue
            }
            OpCode::Neg => {
                let v = pop!();
                push!(match v {
                    Value::I32(x) => Value::I32(-x),
                    Value::I64(x) => Value::I64(-x),
                    Value::F64(x) => Value::F64(-x),
                    _ => Value::I32(0),
                });
                StepResult::Continue
            }
            OpCode::And => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(a.as_i32() & b.as_i32()));
                StepResult::Continue
            }
            OpCode::Or => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(a.as_i32() | b.as_i32()));
                StepResult::Continue
            }
            OpCode::Xor => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(a.as_i32() ^ b.as_i32()));
                StepResult::Continue
            }
            OpCode::Not => {
                let v = pop!();
                push!(Value::I32(!v.as_i32()));
                StepResult::Continue
            }
            OpCode::Shl => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(a.as_i32().wrapping_shl(b.as_i32() as u32)));
                StepResult::Continue
            }
            OpCode::Shr => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(a.as_i32().wrapping_shr(b.as_i32() as u32)));
                StepResult::Continue
            }
            OpCode::Shr_Un => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(
                    (a.as_i32() as u32).wrapping_shr(b.as_i32() as u32) as i32
                ));
                StepResult::Continue
            }

            // --- Comparaisons ---
            OpCode::Ceq => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(if a.as_i64() == b.as_i64() { 1 } else { 0 }));
                StepResult::Continue
            }
            OpCode::Cgt => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(if a.as_i64() > b.as_i64() { 1 } else { 0 }));
                StepResult::Continue
            }
            OpCode::Cgt_Un => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(if (a.as_i64() as u64) > (b.as_i64() as u64) {
                    1
                } else {
                    0
                }));
                StepResult::Continue
            }
            OpCode::Clt => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(if a.as_i64() < b.as_i64() { 1 } else { 0 }));
                StepResult::Continue
            }
            OpCode::Clt_Un => {
                let b = pop!();
                let a = pop!();
                push!(Value::I32(if (a.as_i64() as u64) < (b.as_i64() as u64) {
                    1
                } else {
                    0
                }));
                StepResult::Continue
            }

            // --- Branchements courts ---
            OpCode::Br_S => {
                let target = (next_ip as i64 + opcodes::read_i8(code, offset + 1) as i64) as usize;
                StepResult::Branch(target)
            }
            OpCode::Brfalse_S => {
                let v = pop!();
                if !v.is_truthy() {
                    StepResult::Branch(
                        (next_ip as i64 + opcodes::read_i8(code, offset + 1) as i64) as usize,
                    )
                } else {
                    f.ip = next_ip;
                    StepResult::Continue
                }
            }
            OpCode::Brtrue_S => {
                let v = pop!();
                if v.is_truthy() {
                    StepResult::Branch(
                        (next_ip as i64 + opcodes::read_i8(code, offset + 1) as i64) as usize,
                    )
                } else {
                    f.ip = next_ip;
                    StepResult::Continue
                }
            }
            OpCode::Beq_S => {
                let b = pop!();
                let a = pop!();
                branch_cond_s(a.as_i64() == b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bge_S => {
                let b = pop!();
                let a = pop!();
                branch_cond_s(a.as_i64() >= b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bgt_S => {
                let b = pop!();
                let a = pop!();
                branch_cond_s(a.as_i64() > b.as_i64(), code, offset, next_ip)
            }
            OpCode::Ble_S => {
                let b = pop!();
                let a = pop!();
                branch_cond_s(a.as_i64() <= b.as_i64(), code, offset, next_ip)
            }
            OpCode::Blt_S => {
                let b = pop!();
                let a = pop!();
                branch_cond_s(a.as_i64() < b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bne_Un_S => {
                let b = pop!();
                let a = pop!();
                branch_cond_s(a.as_i64() != b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bge_Un_S | OpCode::Bgt_Un_S | OpCode::Ble_Un_S | OpCode::Blt_Un_S => {
                let b = pop!();
                let a = pop!();
                let au = a.as_i64() as u64;
                let bu = b.as_i64() as u64;
                let cond = match op {
                    OpCode::Bge_Un_S => au >= bu,
                    OpCode::Bgt_Un_S => au > bu,
                    OpCode::Ble_Un_S => au <= bu,
                    _ => au < bu,
                };
                branch_cond_s(cond, code, offset, next_ip)
            }

            // --- Branchements longs ---
            OpCode::Br => {
                let target = (next_ip as i64 + opcodes::read_i32(code, offset + 1) as i64) as usize;
                StepResult::Branch(target)
            }
            OpCode::Brfalse => {
                let v = pop!();
                if !v.is_truthy() {
                    StepResult::Branch(
                        (next_ip as i64 + opcodes::read_i32(code, offset + 1) as i64) as usize,
                    )
                } else {
                    f.ip = next_ip;
                    StepResult::Continue
                }
            }
            OpCode::Brtrue => {
                let v = pop!();
                if v.is_truthy() {
                    StepResult::Branch(
                        (next_ip as i64 + opcodes::read_i32(code, offset + 1) as i64) as usize,
                    )
                } else {
                    f.ip = next_ip;
                    StepResult::Continue
                }
            }
            OpCode::Beq => {
                let b = pop!();
                let a = pop!();
                branch_cond(a.as_i64() == b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bge => {
                let b = pop!();
                let a = pop!();
                branch_cond(a.as_i64() >= b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bgt => {
                let b = pop!();
                let a = pop!();
                branch_cond(a.as_i64() > b.as_i64(), code, offset, next_ip)
            }
            OpCode::Ble => {
                let b = pop!();
                let a = pop!();
                branch_cond(a.as_i64() <= b.as_i64(), code, offset, next_ip)
            }
            OpCode::Blt => {
                let b = pop!();
                let a = pop!();
                branch_cond(a.as_i64() < b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bne_Un => {
                let b = pop!();
                let a = pop!();
                branch_cond(a.as_i64() != b.as_i64(), code, offset, next_ip)
            }
            OpCode::Bge_Un | OpCode::Bgt_Un | OpCode::Ble_Un | OpCode::Blt_Un => {
                let b = pop!();
                let a = pop!();
                let au = a.as_i64() as u64;
                let bu = b.as_i64() as u64;
                let cond = match op {
                    OpCode::Bge_Un => au >= bu,
                    OpCode::Bgt_Un => au > bu,
                    OpCode::Ble_Un => au <= bu,
                    _ => au < bu,
                };
                branch_cond(cond, code, offset, next_ip)
            }

            OpCode::Switch => {
                let n = opcodes::read_u32(code, offset + 1) as usize;
                let v = pop!().as_i32() as u32 as usize;
                let base = next_ip;
                if v < n {
                    let target_off = opcodes::read_i32(code, offset + 5 + v * 4);
                    StepResult::Branch((base as i64 + target_off as i64) as usize)
                } else {
                    f.ip = next_ip;
                    StepResult::Continue
                }
            }

            // --- Conversions ---
            OpCode::Conv_I1 => {
                let v = pop!();
                push!(Value::I32(v.as_i32() as i8 as i32));
                StepResult::Continue
            }
            OpCode::Conv_I2 => {
                let v = pop!();
                push!(Value::I32(v.as_i32() as i16 as i32));
                StepResult::Continue
            }
            OpCode::Conv_I4 => {
                let v = pop!();
                push!(Value::I32(v.as_i32()));
                StepResult::Continue
            }
            OpCode::Conv_I8 => {
                let v = pop!();
                push!(Value::I64(v.as_i64()));
                StepResult::Continue
            }
            OpCode::Conv_R4 => {
                let v = pop!();
                push!(Value::F32(v.as_f64() as f32));
                StepResult::Continue
            }
            OpCode::Conv_R8 => {
                let v = pop!();
                push!(Value::F64(v.as_f64()));
                StepResult::Continue
            }
            OpCode::Conv_U1 => {
                let v = pop!();
                push!(Value::I32(v.as_i32() as u8 as i32));
                StepResult::Continue
            }
            OpCode::Conv_U2 => {
                let v = pop!();
                push!(Value::I32(v.as_i32() as u16 as i32));
                StepResult::Continue
            }
            OpCode::Conv_U4 => {
                let v = pop!();
                push!(Value::I32(v.as_i32()));
                StepResult::Continue
            }
            OpCode::Conv_U8 => {
                let v = pop!();
                push!(Value::I64(v.as_i64()));
                StepResult::Continue
            }
            OpCode::Conv_I => {
                let v = pop!();
                push!(Value::I64(v.as_i64()));
                StepResult::Continue
            }
            OpCode::Conv_U => {
                let v = pop!();
                push!(Value::I64(v.as_i64()));
                StepResult::Continue
            }
            OpCode::Conv_R_Un => {
                let v = pop!();
                push!(Value::F64(v.as_i64() as u64 as f64));
                StepResult::Continue
            }
            OpCode::Conv_Ovf_I1
            | OpCode::Conv_Ovf_I2
            | OpCode::Conv_Ovf_I4
            | OpCode::Conv_Ovf_I8
            | OpCode::Conv_Ovf_U1
            | OpCode::Conv_Ovf_U2
            | OpCode::Conv_Ovf_U4
            | OpCode::Conv_Ovf_U8
            | OpCode::Conv_Ovf_I
            | OpCode::Conv_Ovf_U
            | OpCode::Conv_Ovf_I1_Un
            | OpCode::Conv_Ovf_I2_Un
            | OpCode::Conv_Ovf_I4_Un
            | OpCode::Conv_Ovf_I8_Un
            | OpCode::Conv_Ovf_U1_Un
            | OpCode::Conv_Ovf_U2_Un
            | OpCode::Conv_Ovf_U4_Un
            | OpCode::Conv_Ovf_U8_Un
            | OpCode::Conv_Ovf_I_Un
            | OpCode::Conv_Ovf_U_Un => {
                // Simplifié : pas de vérification de débordement
                let v = pop!();
                push!(Value::I64(v.as_i64()));
                StepResult::Continue
            }
            OpCode::Add_Ovf | OpCode::Add_Ovf_Un => {
                let b = pop!();
                let a = pop!();
                push!(arith_add(a, b));
                StepResult::Continue
            }
            OpCode::Sub_Ovf | OpCode::Sub_Ovf_Un => {
                let b = pop!();
                let a = pop!();
                push!(arith_sub(a, b));
                StepResult::Continue
            }
            OpCode::Mul_Ovf | OpCode::Mul_Ovf_Un => {
                let b = pop!();
                let a = pop!();
                push!(arith_mul(a, b));
                StepResult::Continue
            }

            // --- Chaînes ---
            OpCode::Ldstr => {
                let token = opcodes::read_u32(code, offset + 1);
                let us_idx = token & 0x00FFFFFF;
                let s = self.assembly.get_user_string(us_idx);
                f.eval_stack.push(bcl::alloc_managed_string(&s));
                StepResult::Continue
            }

            // --- Appels ---
            OpCode::Call | OpCode::Callvirt => {
                let token = opcodes::read_u32(code, offset + 1);
                f.ip = next_ip;
                let sig = self.get_method_sig(token);
                let f = self.frames.last_mut().unwrap();
                let split = f
                    .eval_stack
                    .len()
                    .saturating_sub(sig.param_types.len() + sig.has_this() as usize);
                let args = f.eval_stack.split_off(split);
                match self.invoke(token, args, op == OpCode::Callvirt) {
                    Ok(v) => {
                        if !matches!(sig.ret_type, ElementType::Void) {
                            self.frames.last_mut().unwrap().eval_stack.push(v);
                        }
                        StepResult::Continue
                    }
                    Err(e) => StepResult::Throw(e),
                }
            }

            OpCode::Newobj => {
                let token = opcodes::read_u32(code, offset + 1);
                f.ip = next_ip;
                let sig = self.get_method_sig(token);
                let (type_id, size) = self.constructed_type(token);
                let f = self.frames.last_mut().unwrap();
                let split = f.eval_stack.len().saturating_sub(sig.param_types.len());
                let mut args = f.eval_stack.split_off(split);
                let obj = gc::with_gc(|gc| gc.alloc(type_id, size));
                args.insert(0, Value::Ref(obj));
                match self.invoke(token, args, false) {
                    Ok(_) => {
                        self.frames
                            .last_mut()
                            .unwrap()
                            .eval_stack
                            .push(Value::Ref(obj));
                        StepResult::Continue
                    }
                    Err(e) => StepResult::Throw(e),
                }
            }

            OpCode::Ret => {
                let ret_val = f.eval_stack.pop().unwrap_or(Value::Null);
                StepResult::Return(ret_val)
            }

            // --- Champs ---
            OpCode::Ldfld | OpCode::Ldflda => {
                let token = opcodes::read_u32(code, offset + 1);
                let obj = pop!().as_ref();
                if obj.is_null() {
                    return StepResult::Throw(ClrException::null_reference());
                }
                let (slot, et) = self.instance_field(token);
                let ptr = unsafe { obj.add(slot) };
                let val = if op == OpCode::Ldflda {
                    Value::Ref(ptr)
                } else {
                    load_slot(ptr, &et)
                };
                self.frames.last_mut().unwrap().eval_stack.push(val);
                StepResult::Continue
            }
            OpCode::Stfld => {
                let token = opcodes::read_u32(code, offset + 1);
                let val = pop!();
                let obj = pop!().as_ref();
                if obj.is_null() {
                    return StepResult::Throw(ClrException::null_reference());
                }
                let (slot, _) = self.instance_field(token);
                store_slot(unsafe { obj.add(slot) }, val);
                StepResult::Continue
            }
            OpCode::Ldsfld | OpCode::Ldsflda => {
                let token = opcodes::read_u32(code, offset + 1);
                let (ptr, et) = self.static_field(token);
                let val = if op == OpCode::Ldsflda {
                    Value::Ref(ptr)
                } else {
                    load_slot(ptr, &et)
                };
                self.frames.last_mut().unwrap().eval_stack.push(val);
                StepResult::Continue
            }
            OpCode::Stsfld => {
                let token = opcodes::read_u32(code, offset + 1);
                let val = pop!();
                let (ptr, _) = self.static_field(token);
                store_slot(ptr, val);
                StepResult::Continue
            }

            // --- Tableaux ---
            OpCode::Newarr => {
                let _type_token = opcodes::read_u32(code, offset + 1);
                let length = pop!().as_i32() as usize;
                let ptr = gc::with_gc(|gc| gc.alloc_array(types::TYPE_ARRAY, 8, length));
                push!(Value::Ref(ptr));
                StepResult::Continue
            }
            OpCode::Ldlen => {
                let arr = pop!().as_ref();
                let len = if !arr.is_null() {
                    unsafe { *(arr as *const u64) as i32 }
                } else {
                    0
                };
                push!(Value::I32(len));
                StepResult::Continue
            }
            OpCode::Ldelem_I4 | OpCode::Ldelem_U4 => {
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                let val = if !arr.is_null() {
                    unsafe { *(arr.add(8 + idx * 4) as *const i32) }
                } else {
                    0
                };
                push!(Value::I32(val));
                StepResult::Continue
            }
            OpCode::Ldelem_I8 => {
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                let val = if !arr.is_null() {
                    unsafe { *(arr.add(8 + idx * 8) as *const i64) }
                } else {
                    0
                };
                push!(Value::I64(val));
                StepResult::Continue
            }
            OpCode::Ldelem_Ref => {
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                let val = if !arr.is_null() {
                    unsafe { Value::Ref(*(arr.add(8 + idx * 8) as *const *mut u8)) }
                } else {
                    Value::Null
                };
                push!(val);
                StepResult::Continue
            }
            OpCode::Ldelem_I1 | OpCode::Ldelem_U1 => {
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                let val = if !arr.is_null() {
                    unsafe { *arr.add(8 + idx) as i32 }
                } else {
                    0
                };
                push!(Value::I32(val));
                StepResult::Continue
            }
            OpCode::Ldelem_I2 | OpCode::Ldelem_U2 => {
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                let val = if !arr.is_null() {
                    unsafe { *(arr.add(8 + idx * 2) as *const i16) as i32 }
                } else {
                    0
                };
                push!(Value::I32(val));
                StepResult::Continue
            }
            OpCode::Stelem_I4 => {
                let val = pop!().as_i32();
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                if !arr.is_null() {
                    unsafe {
                        *(arr.add(8 + idx * 4) as *mut i32) = val;
                    }
                }
                StepResult::Continue
            }
            OpCode::Stelem_I8 => {
                let val = pop!().as_i64();
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                if !arr.is_null() {
                    unsafe {
                        *(arr.add(8 + idx * 8) as *mut i64) = val;
                    }
                }
                StepResult::Continue
            }
            OpCode::Stelem_Ref => {
                let val = pop!().as_ref();
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                if !arr.is_null() {
                    unsafe {
                        *(arr.add(8 + idx * 8) as *mut *mut u8) = val;
                    }
                }
                StepResult::Continue
            }
            OpCode::Stelem_I1 => {
                let val = pop!().as_i32() as u8;
                let idx = pop!().as_i32() as usize;
                let arr = pop!().as_ref();
                if let Err(e) = check_index(arr, idx) {
                    return StepResult::Throw(e);
                }
                if !arr.is_null() {
                    unsafe {
                        *arr.add(8 + idx) = val;
                    }
                }
                StepResult::Continue
            }

            // --- Indirections ---
            OpCode::Ldind_I4 | OpCode::Ldind_U4 => {
                let ptr = pop!().as_ref();
                let val = if !ptr.is_null() {
                    unsafe { *(ptr as *const i32) }
                } else {
                    0
                };
                push!(Value::I32(val));
                StepResult::Continue
            }
            OpCode::Ldind_I8 => {
                let ptr = pop!().as_ref();
                let val = if !ptr.is_null() {
                    unsafe { *(ptr as *const i64) }
                } else {
                    0
                };
                push!(Value::I64(val));
                StepResult::Continue
            }
            OpCode::Ldind_I | OpCode::Ldind_Ref => {
                let ptr = pop!().as_ref();
                let val = if !ptr.is_null() {
                    unsafe { Value::Ref(*(ptr as *const *mut u8)) }
                } else {
                    Value::Null
                };
                push!(val);
                StepResult::Continue
            }
            OpCode::Stind_I4 => {
                let val = pop!().as_i32();
                let ptr = pop!().as_ref();
                if !ptr.is_null() {
                    unsafe {
                        *(ptr as *mut i32) = val;
                    }
                }
                StepResult::Continue
            }
            OpCode::Stind_I8 => {
                let val = pop!().as_i64();
                let ptr = pop!().as_ref();
                if !ptr.is_null() {
                    unsafe {
                        *(ptr as *mut i64) = val;
                    }
                }
                StepResult::Continue
            }
            OpCode::Stind_Ref | OpCode::Stind_I => {
                let val = pop!().as_ref();
                let ptr = pop!().as_ref();
                if !ptr.is_null() {
                    unsafe {
                        *(ptr as *mut *mut u8) = val;
                    }
                }
                StepResult::Continue
            }
            OpCode::Ldind_I1
            | OpCode::Ldind_U1
            | OpCode::Ldind_I2
            | OpCode::Ldind_U2
            | OpCode::Ldind_R4
            | OpCode::Ldind_R8
            | OpCode::Stind_I1
            | OpCode::Stind_I2
            | OpCode::Stind_R4
            | OpCode::Stind_R8 => {
                // Simplifié
                StepResult::Continue
            }

            // --- Objets ---
            OpCode::Castclass | OpCode::Isinst => {
                let token = opcodes::read_u32(code, offset + 1);
                let obj = pop!();
                let fits = match obj {
                    Value::Ref(p) => self.is_instance(p, token),
                    _ => true,
                };
                let f = self.frames.last_mut().unwrap();
                if fits {
                    f.eval_stack.push(obj);
                } else if op == OpCode::Isinst {
                    f.eval_stack.push(Value::Null);
                } else {
                    return StepResult::Throw(ClrException::new(
                        "System",
                        "InvalidCastException",
                        "Specified cast is not valid.",
                    ));
                }
                StepResult::Continue
            }
            OpCode::Box => {
                let token = opcodes::read_u32(code, offset + 1);
                let val = pop!();
                // Les structures sont déjà représentées par une référence.
                let boxed = if let Value::Ref(_) = val {
                    val
                } else {
                    let type_id = self.resolve_type_token(token).unwrap_or(types::TYPE_OBJECT);
                    let p = gc::with_gc(|gc| gc.alloc(type_id, 8));
                    store_slot(p, val);
                    Value::Ref(p)
                };
                self.frames.last_mut().unwrap().eval_stack.push(boxed);
                StepResult::Continue
            }
            OpCode::Unbox | OpCode::Unbox_Any => {
                let obj = pop!().as_ref();
                let val = match gc::type_of(obj) {
                    None => return StepResult::Throw(ClrException::null_reference()),
                    Some(id) if (types::TYPE_INT32..=types::TYPE_INTPTR).contains(&id) => {
                        if op == OpCode::Unbox {
                            Value::Ref(obj)
                        } else {
                            bcl::unbox(obj, id)
                        }
                    }
                    Some(_) => Value::Ref(obj),
                };
                self.frames.last_mut().unwrap().eval_stack.push(val);
                StepResult::Continue
            }

            // --- Exceptions ---
            OpCode::Throw => match pop!() {
                Value::Ref(p) if gc::type_of(p).is_some() => {
                    StepResult::Throw(ClrException { obj_ptr: p })
                }
                _ => StepResult::Throw(ClrException::null_reference()),
            },
            OpCode::Rethrow => match f.caught {
                Some(exc) => StepResult::Throw(exc),
                None => StepResult::Throw(ClrException::null_reference()),
            },
            OpCode::Leave | OpCode::Leave_S => {
                let target = if op == OpCode::Leave_S {
                    (next_ip as i64 + opcodes::read_i8(code, offset + 1) as i64) as usize
                } else {
                    (next_ip as i64 + opcodes::read_i32(code, offset + 1) as i64) as usize
                };
                f.eval_stack.clear();
                f.caught = None;
                match exceptions::find_leave_target(&f.exception_clauses, offset, target) {
                    Some(handler) => {
                        f.leave_target = Some(target);
                        StepResult::Branch(handler)
                    }
                    None => StepResult::Branch(target),
                }
            }
            OpCode::Endfinally => {
                if let Some(exc) = f.pending.take() {
                    return StepResult::Throw(exc);
                }
                match f.leave_target.take() {
                    Some(target) => {
                        match exceptions::find_leave_target(&f.exception_clauses, offset, target) {
                            Some(handler) => {
                                f.leave_target = Some(target);
                                StepResult::Branch(handler)
                            }
                            None => StepResult::Branch(target),
                        }
                    }
                    None => StepResult::Continue,
                }
            }

            // --- Tokens ---
            OpCode::Ldtoken => {
                let _token = opcodes::read_u32(code, offset + 1);
                push!(Value::I64(0));
                StepResult::Continue
            }

            // --- Divers ---
            OpCode::Initobj => {
                let _token = opcodes::read_u32(code, offset + 2);
                let ptr = pop!().as_ref();
                if !ptr.is_null() {
                    unsafe {
                        std::ptr::write_bytes(ptr, 0, 16);
                    }
                }
                StepResult::Continue
            }
            OpCode::Sizeof => {
                let _token = opcodes::read_u32(code, offset + 2);
                push!(Value::I32(8));
                StepResult::Continue
            }
            OpCode::Localloc => {
                let size = pop!().as_i32() as usize;
                let ptr = gc::with_gc(|gc| gc.alloc(0, size));
                push!(Value::Ref(ptr));
                StepResult::Continue
            }

            // Préfixes (ignorés)
            OpCode::Volatile
            | OpCode::Unaligned
            | OpCode::Tail
            | OpCode::Readonly
            | OpCode::Constrained => StepResult::Continue,

            OpCode::Ldloc
            | OpCode::Ldloca
            | OpCode::Stloc
            | OpCode::Starg
            | OpCode::Ldarga
            | OpCode::Ldarga_S
            | OpCode::Ldloca_S => {
                // Formes longues et adresses
                let idx = match op {
                    OpCode::Ldloc | OpCode::Stloc | OpCode::Ldloca => {
                        opcodes::read_u8(code, offset + 2) as usize
                            | ((opcodes::read_u8(code, offset + 3) as usize) << 8)
                    }
                    _ => opcodes::read_u8(code, offset + 1) as usize,
                };
                match op {
                    OpCode::Ldloc => {
                        push!(f.locals.get(idx).copied().unwrap_or(Value::Null));
                    }
                    OpCode::Stloc => {
                        let v = pop!();
                        if idx < f.locals.len() {
                            f.locals[idx] = v;
                        }
                    }
                    OpCode::Ldloca | OpCode::Ldloca_S => {
                        if idx < f.locals.len() {
                            push!(Value::Ref(&mut f.locals[idx] as *mut Value as *mut u8));
                        } else {
                            push!(Value::Null);
                        }
                    }
                    OpCode::Ldarga | OpCode::Ldarga_S => {
                        if idx < f.args.len() {
                            push!(Value::Ref(&mut f.args[idx] as *mut Value as *mut u8));
                        } else {
                            push!(Value::Null);
                        }
                    }
                    OpCode::Starg => {
                        let v = pop!();
                        if idx < f.args.len() {
                            f.args[idx] = v;
                        }
                    }
                    _ => {}
                }
                StepResult::Continue
            }

            _ => {
                crate::log_warn!("clr-il", "opcode non implémenté : {:?} à IP={}", op, offset);
                StepResult::Continue
            }
        }
    }

    fn get_method_sig(&self, token: u32) -> metadata::MethodSig {
        let row = Token(token).rid();
        let blob = match Token(token).table() {
            t if t == TableId::MethodDef as u8 => self.assembly.get_method_def(row).signature,
            t if t == TableId::MemberRef as u8 => self.assembly.get_member_ref(row).signature,
            t if t == TableId::MethodSpec as u8 => {
                let spec = self.assembly.get_method_spec(row);
                let (table, inner) = CodedIndex::MethodDefOrRef.decode(spec.method);
                return self.get_method_sig((table.map_or(0, |t| t as u32) << 24) | inner);
            }
            _ => 0,
        };
        metadata::decode_method_sig(self.assembly.get_blob(blob))
    }

    /// Type et taille de l'objet que construit le constructeur `token`.
    fn constructed_type(&self, token: u32) -> (types::TypeId, usize) {
        let (ns, name) = match self.resolve_method(token) {
            Target::Managed(row) => {
                let td = self.assembly.get_type_def(self.find_declaring_type(row));
                (
                    self.assembly.get_string(td.namespace).to_string(),
                    self.assembly.get_string(td.name).to_string(),
                )
            }
            Target::External {
                namespace, class, ..
            } => (namespace, class),
        };
        types::with_types(|reg| {
            let id = reg.ensure_external(&ns, &name);
            let size = reg
                .get(id)
                .map_or(types::EXTERNAL_OBJECT_SIZE, |t| t.instance_size);
            (id, size.max(16))
        })
    }

    fn is_instance(&self, obj: *mut u8, token: u32) -> bool {
        let Some(actual) = gc::type_of(obj) else {
            return true;
        };
        // Les interfaces implémentées ne sont pas suivies : on accepte.
        if self.is_interface_token(token) {
            return true;
        }
        match self.resolve_type_token(token) {
            Some(target) => types::with_types(|reg| {
                let known_class = reg.get(target).is_some_and(|t| !t.methods.is_empty())
                    || reg.is_subclass_of(target, types::TYPE_EXCEPTION)
                    || target < types::TYPE_EXCEPTION;
                !known_class || reg.is_subclass_of(actual, target)
            }),
            None => true,
        }
    }

    fn field_type(&self, field_row: u32) -> ElementType {
        let blob = self
            .assembly
            .get_blob(self.assembly.get_field(field_row).signature);
        let mut off = 1;
        metadata::decode_type(blob, &mut off)
    }

    fn owner_type_id(&self, field_row: u32) -> Option<types::TypeId> {
        let owner = (1..=self.assembly.row_count(TableId::TypeDef))
            .find(|&i| self.assembly.type_fields(i).contains(&field_row))?;
        let td = self.assembly.get_type_def(owner);
        let ns = self.assembly.get_string(td.namespace);
        let name = self.assembly.get_string(td.name);
        types::with_types(|reg| reg.find_by_name(ns, name))
    }

    /// Décalage et type d'un champ d'instance. Les champs des types de la BCL
    /// (MemberRef) ne sont pas modélisés : on utilise l'emplacement réservé.
    fn instance_field(&self, token: u32) -> (usize, ElementType) {
        let row = Token(token).rid();
        if Token(token).table() != TableId::Field as u8 {
            return (exceptions::MESSAGE_SLOT, ElementType::Object);
        }
        let offset = self.owner_type_id(row).and_then(|id| {
            types::with_types(|reg| {
                reg.get(id)?
                    .fields
                    .iter()
                    .find(|f| f.metadata_row == row)
                    .map(|f| f.offset)
            })
        });
        (offset.unwrap_or(0), self.field_type(row))
    }

    fn static_field(&self, token: u32) -> (*mut u8, ElementType) {
        let row = Token(token).rid();
        let owner = self.owner_type_id(row).unwrap_or(0);
        let ptr = types::with_types(|reg| reg.static_field_ptr(owner, row as usize * 8));
        (ptr, self.field_type(row))
    }
}

fn branch_cond_s(cond: bool, code: &[u8], offset: usize, next_ip: usize) -> StepResult {
    if cond {
        StepResult::Branch((next_ip as i64 + opcodes::read_i8(code, offset + 1) as i64) as usize)
    } else {
        StepResult::Continue
    }
}

fn branch_cond(cond: bool, code: &[u8], offset: usize, next_ip: usize) -> StepResult {
    if cond {
        StepResult::Branch((next_ip as i64 + opcodes::read_i32(code, offset + 1) as i64) as usize)
    } else {
        StepResult::Continue
    }
}

fn arith_add(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) => Value::I32(x.wrapping_add(y)),
        (Value::I64(x), Value::I64(y)) => Value::I64(x.wrapping_add(y)),
        (Value::F64(x), Value::F64(y)) => Value::F64(x + y),
        (Value::F32(x), Value::F32(y)) => Value::F32(x + y),
        _ => Value::I64(a.as_i64().wrapping_add(b.as_i64())),
    }
}

fn arith_sub(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) => Value::I32(x.wrapping_sub(y)),
        (Value::I64(x), Value::I64(y)) => Value::I64(x.wrapping_sub(y)),
        (Value::F64(x), Value::F64(y)) => Value::F64(x - y),
        _ => Value::I64(a.as_i64().wrapping_sub(b.as_i64())),
    }
}

fn arith_mul(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) => Value::I32(x.wrapping_mul(y)),
        (Value::I64(x), Value::I64(y)) => Value::I64(x.wrapping_mul(y)),
        (Value::F64(x), Value::F64(y)) => Value::F64(x * y),
        _ => Value::I64(a.as_i64().wrapping_mul(b.as_i64())),
    }
}

fn arith_div(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) if y != 0 => Value::I32(x.wrapping_div(y)),
        (Value::I64(x), Value::I64(y)) if y != 0 => Value::I64(x.wrapping_div(y)),
        (Value::F64(x), Value::F64(y)) => Value::F64(x / y),
        _ => Value::I32(0),
    }
}

fn arith_rem(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) if y != 0 => Value::I32(x.wrapping_rem(y)),
        (Value::I64(x), Value::I64(y)) if y != 0 => Value::I64(x.wrapping_rem(y)),
        (Value::F64(x), Value::F64(y)) => Value::F64(x % y),
        _ => Value::I32(0),
    }
}
