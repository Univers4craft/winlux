//! Gestion des exceptions .NET pour le runtime CLR Winlux.

use super::gc;
use super::metadata::ExceptionClause;
use super::types::{self, TypeId, TYPE_EXCEPTION};

const COR_ILEXCEPTION_CLAUSE_EXCEPTION: u32 = 0x0000;
const COR_ILEXCEPTION_CLAUSE_FILTER: u32 = 0x0001;
const COR_ILEXCEPTION_CLAUSE_FINALLY: u32 = 0x0002;
const COR_ILEXCEPTION_CLAUSE_FAULT: u32 = 0x0004;

#[derive(Debug, Clone)]
pub struct ClrException {
    pub type_id: TypeId,
    pub message: String,
    pub obj_ptr: *mut u8,
}

unsafe impl Send for ClrException {}

impl ClrException {
    pub fn new(type_id: TypeId, message: &str) -> Self {
        let obj_ptr = gc::with_gc(|gc| gc.alloc(type_id, 64));
        ClrException {
            type_id,
            message: message.to_string(),
            obj_ptr,
        }
    }

    pub fn from_object(ptr: *mut u8) -> Self {
        if ptr.is_null() {
            return Self::null_reference();
        }
        let type_id = unsafe {
            let hdr = ptr.sub(std::mem::size_of::<gc::ObjHeader>()) as *const gc::ObjHeader;
            (*hdr).type_id
        };
        let type_name = types::with_types(|reg| {
            reg.get(type_id).map(|t| t.full_name()).unwrap_or_else(|| "Exception".into())
        });
        ClrException {
            type_id,
            message: type_name,
            obj_ptr: ptr,
        }
    }

    pub fn null_reference() -> Self {
        Self::new(TYPE_EXCEPTION, "NullReferenceException: Object reference not set to an instance of an object.")
    }

    pub fn invalid_cast(from: &str, to: &str) -> Self {
        Self::new(TYPE_EXCEPTION, &format!("InvalidCastException: Unable to cast object of type '{}' to type '{}'.", from, to))
    }

    pub fn index_out_of_range() -> Self {
        Self::new(TYPE_EXCEPTION, "IndexOutOfRangeException: Index was outside the bounds of the array.")
    }

    pub fn overflow() -> Self {
        Self::new(TYPE_EXCEPTION, "OverflowException: Arithmetic operation resulted in an overflow.")
    }

    pub fn divide_by_zero() -> Self {
        Self::new(TYPE_EXCEPTION, "DivideByZeroException: Attempted to divide by zero.")
    }

    pub fn stack_overflow() -> Self {
        Self::new(TYPE_EXCEPTION, "StackOverflowException: Operation caused a stack overflow.")
    }

    pub fn not_implemented(what: &str) -> Self {
        Self::new(TYPE_EXCEPTION, &format!("NotImplementedException: {}", what))
    }

    pub fn argument(param: &str, msg: &str) -> Self {
        Self::new(TYPE_EXCEPTION, &format!("ArgumentException: {} (Parameter '{}')", msg, param))
    }

    pub fn argument_null(param: &str) -> Self {
        Self::new(TYPE_EXCEPTION, &format!("ArgumentNullException: Value cannot be null. (Parameter '{}')", param))
    }

    pub fn invalid_operation(msg: &str) -> Self {
        Self::new(TYPE_EXCEPTION, &format!("InvalidOperationException: {}", msg))
    }

    pub fn file_not_found(path: &str) -> Self {
        Self::new(TYPE_EXCEPTION, &format!("FileNotFoundException: Could not find file '{}'.", path))
    }

    pub fn type_name(&self) -> String {
        types::with_types(|reg| {
            reg.get(self.type_id).map(|t| t.full_name()).unwrap_or_else(|| "System.Exception".into())
        })
    }
}

#[derive(Debug)]
pub enum ExceptionAction {
    Catch { handler_offset: usize },
    Finally { handler_offset: usize },
    Unhandled,
}

pub fn find_handler(
    clauses: &[ExceptionClause],
    ip: usize,
    exception: &ClrException,
) -> ExceptionAction {
    let mut finally_clause: Option<&ExceptionClause> = None;

    for clause in clauses {
        let try_start = clause.try_offset as usize;
        let try_end = try_start + clause.try_length as usize;

        if ip < try_start || ip >= try_end {
            continue;
        }

        match clause.flags {
            COR_ILEXCEPTION_CLAUSE_EXCEPTION => {
                if exception_matches(exception, clause.class_token_or_filter) {
                    return ExceptionAction::Catch {
                        handler_offset: clause.handler_offset as usize,
                    };
                }
            }
            COR_ILEXCEPTION_CLAUSE_FINALLY => {
                if finally_clause.is_none() {
                    finally_clause = Some(clause);
                }
            }
            COR_ILEXCEPTION_CLAUSE_FAULT => {
                if finally_clause.is_none() {
                    finally_clause = Some(clause);
                }
            }
            COR_ILEXCEPTION_CLAUSE_FILTER => {
                crate::log_debug!("clr-exc", "clause filter non supportée, ignorée");
            }
            _ => {}
        }
    }

    if let Some(fc) = finally_clause {
        return ExceptionAction::Finally {
            handler_offset: fc.handler_offset as usize,
        };
    }

    ExceptionAction::Unhandled
}

fn exception_matches(exception: &ClrException, class_token: u32) -> bool {
    if class_token == 0 {
        return true;
    }

    let table = (class_token >> 24) as u8;
    let _rid = class_token & 0x00FFFFFF;

    if table == 0x01 {
        return true;
    }

    types::with_types(|reg| {
        reg.is_subclass_of(exception.type_id, TYPE_EXCEPTION)
    })
}

pub fn find_leave_target(
    clauses: &[ExceptionClause],
    ip: usize,
    target: usize,
) -> Option<usize> {
    for clause in clauses {
        if clause.flags != COR_ILEXCEPTION_CLAUSE_FINALLY {
            continue;
        }
        let try_start = clause.try_offset as usize;
        let try_end = try_start + clause.try_length as usize;

        if ip >= try_start && ip < try_end && (target < try_start || target >= try_end) {
            return Some(clause.handler_offset as usize);
        }
    }
    None
}

pub fn format_exception(exc: &ClrException) -> String {
    format!("Unhandled Exception: {}", exc.message)
}
