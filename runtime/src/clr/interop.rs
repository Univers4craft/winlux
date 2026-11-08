//! Appel d'une méthode `[DllImport]` : préparation des arguments selon la
//! signature (ECMA-335 §II.15.5 ; disposition Win64 des structures),
//! appel natif, puis conversion du résultat.

use super::bcl;
use super::callbacks;
use super::exceptions::ClrException;
use super::gc;
use super::il::{Gen, Interpreter, Value};
use super::metadata::{ElementType, MethodSig};
use super::pinvoke::{self, Arg, Import};
use super::tables::{TableId, Token};
use super::types::Kind;

/// Une structure de 1, 2, 4 ou 8 octets voyage dans un registre ; les
/// autres par adresse (convention Win64).
fn in_register(size: usize) -> bool {
    matches!(size, 1 | 2 | 4 | 8)
}

fn read_small(ptr: *const u8, size: usize) -> u64 {
    let mut bytes = [0u8; 8];
    unsafe { std::ptr::copy_nonoverlapping(ptr, bytes.as_mut_ptr(), size) };
    u64::from_le_bytes(bytes)
}

impl Interpreter {
    pub(super) fn call_pinvoke(
        &mut self,
        method_row: u32,
        args: Vec<Value>,
        sig: &MethodSig,
        gen: &Gen,
    ) -> Result<Value, ClrException> {
        let token = Token::new(TableId::MethodDef, method_row);
        let Some(imp) = self.assembly.find_impl_map(token) else {
            return Err(ClrException::new(
                "System",
                "EntryPointNotFoundException",
                "P/Invoke sans ImplMap",
            ));
        };
        let module = self
            .assembly
            .get_string(self.assembly.get_module_ref(imp.import_scope).name)
            .to_string();
        let name = self.assembly.get_string(imp.import_name).to_string();
        // CharSet.Unicode (4) et Auto (6) : UTF-16 ; SupportsLastError (0x40).
        let unicode = imp.mapping_flags & 0x0004 != 0;
        let last_error = imp.mapping_flags & 0x0040 != 0;
        let addr = pinvoke::resolve(&Import {
            module: &module,
            name: &name,
            unicode,
        })
        .map_err(|(kind, msg)| ClrException::new("System", kind, &msg))?;

        let asm = self.current;
        let pinned = self.pins.len();
        self.pins.extend(args.iter().copied());
        let mut out = Vec::with_capacity(args.len() + 1);

        // Structure rendue par valeur : dans RAX si elle tient dans un
        // registre, sinon par une adresse cachée en premier argument.
        let ret_kind = match sig.ret_type {
            ElementType::Void => None,
            ref et => Some(self.kind_of_sig(asm, et, gen)),
        };
        let ret_struct = match ret_kind {
            Some(Kind::Struct(id)) => {
                let obj = self.new_struct(id);
                self.pins.push(Value::Ref(obj));
                let size = Kind::Struct(id).elem_size();
                if !in_register(size) {
                    out.push(Arg::Word(obj as u64));
                }
                Some((obj, size))
            }
            _ => None,
        };

        let mut failure = None;
        for (i, v) in args.iter().enumerate() {
            let arg = match sig.param_types.get(i) {
                Some(ElementType::R4) => Arg::F32(v.as_f64() as f32),
                Some(ElementType::R8) => Arg::F64(v.as_f64()),
                Some(ElementType::String) => match v {
                    Value::Ref(p) if !p.is_null() => Arg::Str(bcl::read_managed_string(*p)),
                    _ => Arg::Word(0),
                },
                // Tableau blittable : adresse du premier élément.
                Some(ElementType::SzArray(_)) => match v {
                    Value::Ref(p) if !p.is_null() => Arg::Word(*p as u64 + 8),
                    _ => Arg::Word(0),
                },
                Some(et) => match (self.kind_of_sig(asm, et, gen), v) {
                    (Kind::Struct(id), Value::Ref(p)) => {
                        let size = Kind::Struct(id).elem_size();
                        if in_register(size) {
                            Arg::Word(read_small(*p, size))
                        } else {
                            Arg::Word(*p as u64)
                        }
                    }
                    (Kind::Ref, Value::Ref(p)) if gc::is_delegate(*p) => {
                        match self.callback_for(*p, unicode) {
                            Some(f) => Arg::Word(f as u64),
                            None => {
                                failure = Some(ClrException::new(
                                    "System",
                                    "NotSupportedException",
                                    &format!(
                                        "plus de {} délégués confiés au code natif",
                                        callbacks::CAPACITY
                                    ),
                                ));
                                break;
                            }
                        }
                    }
                    (_, v) => Arg::Word(v.as_i64() as u64),
                },
                None => Arg::Word(v.as_i64() as u64),
            };
            out.push(arg);
        }
        if let Some(e) = failure {
            self.pins.truncate(pinned);
            return Err(e);
        }

        let raw = callbacks::with_interpreter(self, || {
            // Le code natif peut bloquer (boucle de messages, attente) : les
            // autres fils managés continuent pendant ce temps.
            let depth = super::threads::gil_release_all();
            let raw = pinvoke::call(addr, &out, unicode, last_error);
            super::threads::gil_restore(depth);
            raw
        });
        self.pins.truncate(pinned);

        Ok(match (&sig.ret_type, ret_kind) {
            (ElementType::Void, _) | (_, None) => Value::Null,
            (ElementType::String, _) => {
                if raw.int == 0 {
                    Value::Null
                } else {
                    bcl::alloc_managed_string(&pinvoke::read_native_string(raw.int, unicode))
                }
            }
            (ElementType::R4, _) => Value::F32(f32::from_bits(raw.float as u32)),
            (ElementType::R8, _) => Value::F64(f64::from_bits(raw.float)),
            (_, Some(Kind::Struct(_))) => {
                let (obj, size) = ret_struct.expect("structure de retour");
                if in_register(size) {
                    let bytes = raw.int.to_le_bytes();
                    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), obj, size) };
                }
                Value::Ref(obj)
            }
            // bool .NET = BOOL Win32 sur 4 octets.
            (_, Some(Kind::Bool)) => Value::I32((raw.int as u32 != 0) as i32),
            (_, Some(Kind::Ref)) => Value::I64(raw.int as i64),
            (_, Some(kind)) => super::il::load(&raw.int as *const u64 as *const u8, kind),
        })
    }
}
