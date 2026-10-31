//! Interpréteur CIL (Common Intermediate Language).
//!
//! Machine à pile qui exécute le bytecode .NET instruction par instruction.

use super::gc;
use super::metadata::{self, Assembly, ElementType, MethodBody};
use super::opcodes::{self, OpCode};
use super::tables::{TableId, Token};
use super::types::{self, TypeId, TypeRegistry};

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
            Value::Null => std::ptr::null_mut(),
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

struct CallFrame {
    method_token: u32,
    ip: usize,
    code: Vec<u8>,
    locals: Vec<Value>,
    args: Vec<Value>,
    eval_stack: Vec<Value>,
    exception_clauses: Vec<metadata::ExceptionClause>,
}

pub struct Interpreter {
    frames: Vec<CallFrame>,
    pub assembly: Assembly,
    pub exit_code: i32,
}

pub enum ExecResult {
    Return(Value),
    Exit(i32),
    Exception(String),
    PInvoke {
        module: String,
        name: String,
        args: Vec<Value>,
    },
    InternalCall {
        namespace: String,
        class: String,
        method: String,
        args: Vec<Value>,
    },
}

impl Interpreter {
    pub fn new(assembly: Assembly) -> Self {
        Interpreter {
            frames: Vec::new(),
            assembly,
            exit_code: 0,
        }
    }

    pub fn execute_method(&mut self, method_row: u32, args: Vec<Value>) -> ExecResult {
        let md = self.assembly.get_method_def(method_row);
        let name = self.assembly.get_string(md.name).to_string();

        if md.is_pinvoke() {
            let token = Token::new(TableId::MethodDef, method_row);
            if let Some(imp) = self.assembly.find_impl_map(token) {
                let mod_row = self.assembly.get_module_ref(imp.import_scope);
                let module = self.assembly.get_string(mod_row.name).to_string();
                let import_name = self.assembly.get_string(imp.import_name).to_string();
                return ExecResult::PInvoke {
                    module,
                    name: import_name,
                    args,
                };
            }
        }

        if md.rva == 0 {
            let type_row = self.find_declaring_type(method_row);
            let td = self.assembly.get_type_def(type_row);
            let ns = self.assembly.get_string(td.namespace).to_string();
            let cls = self.assembly.get_string(td.name).to_string();
            return ExecResult::InternalCall {
                namespace: ns,
                class: cls,
                method: name,
                args,
            };
        }

        let body = match self.assembly.method_body(md.rva) {
            Some(b) => b,
            None => {
                return ExecResult::Exception(format!(
                    "corps de méthode introuvable pour {name} (RVA {:#x})",
                    md.rva
                ))
            }
        };

        let sig_blob = self.assembly.get_blob(md.signature);
        let _sig = metadata::decode_method_sig(sig_blob);

        let locals = if body.local_var_sig_token != 0 {
            let sig_row = (body.local_var_sig_token & 0x00FFFFFF) as u32;
            let sig = self.assembly.get_standalone_sig(sig_row);
            let blob = self.assembly.get_blob(sig.signature);
            let local_types = metadata::decode_locals_sig(blob);
            local_types.iter().map(|t| Value::default_for(t)).collect()
        } else {
            Vec::new()
        };

        let frame = CallFrame {
            method_token: Token::new(TableId::MethodDef, method_row).0,
            ip: 0,
            code: body.code.clone(),
            locals,
            args,
            eval_stack: Vec::with_capacity(body.max_stack as usize),
            exception_clauses: body.exception_clauses.clone(),
        };
        self.frames.push(frame);
        self.run_loop()
    }

    pub fn execute_token(&mut self, token: u32, args: Vec<Value>) -> ExecResult {
        let table = (token >> 24) as u8;
        let row = token & 0x00FFFFFF;
        match table {
            0x06 => self.execute_method(row, args),
            0x0A => {
                let mr = self.assembly.get_member_ref(row);
                let name = self.assembly.get_string(mr.name).to_string();
                let (parent_table, parent_row) =
                    super::tables::CodedIndex::MemberRefParent.decode(mr.class);
                let (ns, cls) = match parent_table {
                    Some(TableId::TypeRef) => {
                        let tr = self.assembly.get_type_ref(parent_row);
                        (
                            self.assembly.get_string(tr.namespace).to_string(),
                            self.assembly.get_string(tr.name).to_string(),
                        )
                    }
                    Some(TableId::TypeDef) => {
                        let td = self.assembly.get_type_def(parent_row);
                        (
                            self.assembly.get_string(td.namespace).to_string(),
                            self.assembly.get_string(td.name).to_string(),
                        )
                    }
                    _ => (String::new(), String::new()),
                };
                ExecResult::InternalCall {
                    namespace: ns,
                    class: cls,
                    method: name,
                    args,
                }
            }
            _ => ExecResult::Exception(format!("token non supporté : {token:#x}")),
        }
    }

    fn find_declaring_type(&self, method_row: u32) -> u32 {
        let count = self.assembly.row_count(TableId::TypeDef);
        for i in 1..=count {
            let range = self.assembly.type_methods(i);
            if method_row >= range.start && method_row < range.end {
                return i;
            }
        }
        0
    }

    fn run_loop(&mut self) -> ExecResult {
        loop {
            let depth = self.frames.len();
            if depth == 0 {
                return ExecResult::Return(Value::I32(self.exit_code));
            }
            let frame = &self.frames[depth - 1];
            if frame.ip >= frame.code.len() {
                let ret = self
                    .frames
                    .pop()
                    .unwrap()
                    .eval_stack
                    .pop()
                    .unwrap_or(Value::Null);
                if self.frames.is_empty() {
                    return ExecResult::Return(ret);
                }
                self.frames.last_mut().unwrap().eval_stack.push(ret);
                continue;
            }

            let instr =
                match opcodes::decode(&self.frames[depth - 1].code, self.frames[depth - 1].ip) {
                    Some(i) => i,
                    None => {
                        return ExecResult::Exception(format!(
                            "opcode invalide à IP={}",
                            self.frames[depth - 1].ip
                        ))
                    }
                };
            let next_ip = instr.offset + instr.size;

            match self.step(instr.op, instr.offset, next_ip) {
                StepResult::Continue => {
                    let d = self.frames.len();
                    if d > 0 && self.frames[d - 1].ip == instr.offset {
                        self.frames[d - 1].ip = next_ip;
                    }
                }
                StepResult::Branch(target) => {
                    self.frames.last_mut().unwrap().ip = target;
                }
                StepResult::Call(result) => match result {
                    ExecResult::Return(v) => {
                        self.frames.last_mut().unwrap().eval_stack.push(v);
                        self.frames.last_mut().unwrap().ip = next_ip;
                    }
                    other => return other,
                },
                StepResult::Return(v) => {
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return ExecResult::Return(v);
                    }
                    self.frames.last_mut().unwrap().eval_stack.push(v);
                }
                StepResult::Err(msg) => return ExecResult::Exception(msg),
                StepResult::Delegate(result) => return result,
            }
        }
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
                push!(f.args.get(0).copied().unwrap_or(Value::Null));
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
                push!(f.locals.get(0).copied().unwrap_or(Value::Null));
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
                    return StepResult::Err("System.DivideByZeroException".into());
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
                let ptr = gc::with_gc(|gc| {
                    let bytes = s.encode_utf16().collect::<Vec<u16>>();
                    let data_size = 8 + bytes.len() * 2;
                    let p = gc.alloc(types::TYPE_STRING, data_size);
                    unsafe {
                        *(p as *mut u64) = bytes.len() as u64;
                        let dest = p.add(8) as *mut u16;
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dest, bytes.len());
                    }
                    p
                });
                f.eval_stack.push(Value::Ref(ptr));
                StepResult::Continue
            }

            // --- Appels ---
            OpCode::Call | OpCode::Callvirt => {
                let token = opcodes::read_u32(code, offset + 1);
                f.ip = next_ip;
                let sig = self.get_method_sig(token);
                let argc = sig.param_types.len()
                    + if sig.calling_convention & 0x20 != 0 {
                        1
                    } else {
                        0
                    };
                let has_this = sig.calling_convention & 0x20 != 0;
                let f = self.frames.last_mut().unwrap();
                let mut args = Vec::with_capacity(argc);
                for _ in 0..sig.param_types.len() {
                    args.push(f.eval_stack.pop().unwrap_or(Value::Null));
                }
                args.reverse();
                if has_this {
                    let this = f.eval_stack.pop().unwrap_or(Value::Null);
                    args.insert(0, this);
                }
                let result = self.execute_token(token, args);
                return StepResult::Call(result);
            }

            OpCode::Newobj => {
                let token = opcodes::read_u32(code, offset + 1);
                f.ip = next_ip;
                let sig = self.get_method_sig(token);
                let f = self.frames.last_mut().unwrap();
                let mut args = Vec::new();
                for _ in 0..sig.param_types.len() {
                    args.push(f.eval_stack.pop().unwrap_or(Value::Null));
                }
                args.reverse();
                let obj_ptr = gc::with_gc(|gc| gc.alloc(types::TYPE_OBJECT, 64));
                args.insert(0, Value::Ref(obj_ptr));
                let result = self.execute_token(token, args);
                match result {
                    ExecResult::Return(_) | ExecResult::InternalCall { .. } => {
                        self.frames
                            .last_mut()
                            .unwrap()
                            .eval_stack
                            .push(Value::Ref(obj_ptr));
                        return StepResult::Call(ExecResult::Return(Value::Ref(obj_ptr)));
                    }
                    other => return StepResult::Delegate(other),
                }
            }

            OpCode::Ret => {
                let ret_val = f.eval_stack.pop().unwrap_or(Value::Null);
                StepResult::Return(ret_val)
            }

            // --- Champs ---
            OpCode::Ldfld => {
                let token = opcodes::read_u32(code, offset + 1);
                let obj = pop!().as_ref();
                drop(f);
                let field_offset = self.resolve_field_offset(token);
                let val = if !obj.is_null() {
                    unsafe { Value::I64(*(obj.add(field_offset) as *const i64)) }
                } else {
                    Value::Null
                };
                self.frames.last_mut().unwrap().eval_stack.push(val);
                StepResult::Continue
            }
            OpCode::Stfld => {
                let token = opcodes::read_u32(code, offset + 1);
                let val = pop!();
                let obj = pop!().as_ref();
                drop(f);
                let field_offset = self.resolve_field_offset(token);
                if !obj.is_null() {
                    unsafe {
                        *(obj.add(field_offset) as *mut i64) = val.as_i64();
                    }
                }
                StepResult::Continue
            }
            OpCode::Ldsfld => {
                let token = opcodes::read_u32(code, offset + 1);
                drop(f);
                let val = self.load_static_field(token);
                self.frames.last_mut().unwrap().eval_stack.push(val);
                StepResult::Continue
            }
            OpCode::Stsfld => {
                let token = opcodes::read_u32(code, offset + 1);
                let val = pop!();
                drop(f);
                self.store_static_field(token, val);
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
                let _token = opcodes::read_u32(code, offset + 1);
                // Simplifié : pas de vérification de type
                StepResult::Continue
            }
            OpCode::Box => {
                let _token = opcodes::read_u32(code, offset + 1);
                let val = pop!();
                let ptr = gc::with_gc(|gc| {
                    let p = gc.alloc(types::TYPE_OBJECT, 16);
                    unsafe {
                        *(p as *mut i64) = val.as_i64();
                    }
                    p
                });
                push!(Value::Ref(ptr));
                StepResult::Continue
            }
            OpCode::Unbox | OpCode::Unbox_Any => {
                let _token = opcodes::read_u32(code, offset + 1);
                let obj = pop!().as_ref();
                let val = if !obj.is_null() {
                    unsafe { Value::I64(*(obj as *const i64)) }
                } else {
                    Value::I32(0)
                };
                push!(val);
                StepResult::Continue
            }

            // --- Exceptions ---
            OpCode::Throw => {
                let _exc = pop!();
                StepResult::Err("exception .NET lancée".into())
            }
            OpCode::Leave | OpCode::Leave_S => {
                let target = if op == OpCode::Leave_S {
                    (next_ip as i64 + opcodes::read_i8(code, offset + 1) as i64) as usize
                } else {
                    (next_ip as i64 + opcodes::read_i32(code, offset + 1) as i64) as usize
                };
                f.eval_stack.clear();
                StepResult::Branch(target)
            }
            OpCode::Endfinally => StepResult::Continue,

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
        let table = (token >> 24) as u8;
        let row = token & 0x00FFFFFF;
        let sig_blob_idx = match table {
            0x06 => self.assembly.get_method_def(row).signature,
            0x0A => self.assembly.get_member_ref(row).signature,
            _ => 0,
        };
        let blob = self.assembly.get_blob(sig_blob_idx);
        metadata::decode_method_sig(blob)
    }

    fn resolve_field_offset(&self, token: u32) -> usize {
        let table = (token >> 24) as u8;
        let row = token & 0x00FFFFFF;
        match table {
            0x04 => (row as usize).saturating_sub(1) * 8,
            0x0A => {
                let mr = self.assembly.get_member_ref(row);
                let name = self.assembly.get_string(mr.name);
                types::with_types(|reg| {
                    let (parent_table, parent_row) =
                        super::tables::CodedIndex::MemberRefParent.decode(mr.class);
                    let (ns, cls) = match parent_table {
                        Some(TableId::TypeRef) => {
                            let tr = self.assembly.get_type_ref(parent_row);
                            (
                                self.assembly.get_string(tr.namespace).to_string(),
                                self.assembly.get_string(tr.name).to_string(),
                            )
                        }
                        _ => (String::new(), String::new()),
                    };
                    if let Some(tid) = reg.find_by_name(&ns, &cls) {
                        if let Some(ti) = reg.get(tid) {
                            if let Some((_, fi)) = ti.find_field(name) {
                                return fi.offset;
                            }
                        }
                    }
                    0
                })
            }
            _ => 0,
        }
    }

    fn load_static_field(&self, token: u32) -> Value {
        let table = (token >> 24) as u8;
        let row = token & 0x00FFFFFF;
        match table {
            0x04 => {
                let offset = (row as usize).saturating_sub(1) * 8;
                types::with_types(|reg| {
                    let type_row = self.find_field_owner(row);
                    let td = self.assembly.get_type_def(type_row);
                    let ns = self.assembly.get_string(td.namespace);
                    let name = self.assembly.get_string(td.name);
                    if let Some(tid) = reg.find_by_name(ns, name) {
                        let ptr = reg.static_field_ptr(tid, offset);
                        unsafe { Value::I64(*(ptr as *const i64)) }
                    } else {
                        Value::I32(0)
                    }
                })
            }
            _ => Value::I32(0),
        }
    }

    fn store_static_field(&self, token: u32, val: Value) {
        let table = (token >> 24) as u8;
        let row = token & 0x00FFFFFF;
        if table == 0x04 {
            let offset = (row as usize).saturating_sub(1) * 8;
            types::with_types(|reg| {
                let type_row = self.find_field_owner(row);
                let td = self.assembly.get_type_def(type_row);
                let ns = self.assembly.get_string(td.namespace);
                let name = self.assembly.get_string(td.name);
                if let Some(tid) = reg.find_by_name(ns, name) {
                    let ptr = reg.static_field_ptr(tid, offset);
                    unsafe {
                        *(ptr as *mut i64) = val.as_i64();
                    }
                }
            });
        }
    }

    fn find_field_owner(&self, field_row: u32) -> u32 {
        let count = self.assembly.row_count(TableId::TypeDef);
        for i in 1..=count {
            let range = self.assembly.type_fields(i);
            if field_row >= range.start && field_row < range.end {
                return i;
            }
        }
        0
    }
}

enum StepResult {
    Continue,
    Branch(usize),
    Call(ExecResult),
    Return(Value),
    Err(String),
    Delegate(ExecResult),
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
