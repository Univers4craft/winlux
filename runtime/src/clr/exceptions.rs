//! Exceptions .NET : objets d'exception et recherche des gestionnaires
//! (clauses catch / finally / fault d'un corps de méthode).

use super::bcl;
use super::gc;
use super::metadata::ExceptionClause;
use super::types;

const CLAUSE_EXCEPTION: u32 = 0x0000;
const CLAUSE_FILTER: u32 = 0x0001;
const CLAUSE_FINALLY: u32 = 0x0002;
const CLAUSE_FAULT: u32 = 0x0004;

/// Le message d'une exception est rangé dans le premier emplacement de l'objet,
/// réservé par le runtime pour tous les types (voir `clr::load_types`).
pub const MESSAGE_SLOT: usize = 0;
/// Exception interne (`InnerException`), juste après le message.
pub const INNER_SLOT: usize = 8;
/// Dictionnaire `Exception.Data`, créé à la première lecture par notre
/// mscorlib (#2361).
pub const DATA_SLOT: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct ClrException {
    pub obj_ptr: *mut u8,
}

impl ClrException {
    pub fn new(namespace: &str, name: &str, message: &str) -> Self {
        let type_id = types::with_types(|reg| reg.ensure_external(namespace, name));
        let obj_ptr = gc::with_gc(|gc| gc.alloc(type_id, 64));
        let msg = bcl::alloc_managed_string(message).as_ref();
        unsafe {
            *(obj_ptr.add(MESSAGE_SLOT) as *mut *mut u8) = msg;
        }
        ClrException { obj_ptr }
    }

    pub fn null_reference() -> Self {
        Self::new(
            "System",
            "NullReferenceException",
            "Object reference not set to an instance of an object.",
        )
    }

    pub fn index_out_of_range() -> Self {
        Self::new(
            "System",
            "IndexOutOfRangeException",
            "Index was outside the bounds of the array.",
        )
    }

    pub fn divide_by_zero() -> Self {
        Self::new(
            "System",
            "DivideByZeroException",
            "Attempted to divide by zero.",
        )
    }

    pub fn stack_overflow() -> Self {
        Self::new(
            "System",
            "StackOverflowException",
            "Operation caused a stack overflow.",
        )
    }

    pub fn type_name(&self) -> String {
        let id = gc::type_of(self.obj_ptr).unwrap_or(types::TYPE_EXCEPTION);
        types::with_types(|reg| {
            reg.get(id)
                .map(|t| t.full_name())
                .unwrap_or_else(|| "System.Exception".into())
        })
    }

    pub fn message(&self) -> String {
        if gc::type_of(self.obj_ptr).is_none() {
            return String::new();
        }
        let msg = unsafe { *(self.obj_ptr.add(MESSAGE_SLOT) as *const *mut u8) };
        if gc::type_of(msg) == Some(types::TYPE_STRING) {
            bcl::read_managed_string(msg)
        } else {
            String::new()
        }
    }
}

impl std::fmt::Display for ClrException {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let msg = self.message();
        if msg.is_empty() {
            write!(f, "{}", self.type_name())
        } else {
            write!(f, "{}: {}", self.type_name(), msg)
        }
    }
}

/// Le `try` de `clause` couvre-t-il `ip` ?
pub fn try_covers(clause: &ExceptionClause, ip: usize) -> bool {
    let start = clause.try_offset as usize;
    ip >= start && ip < start + clause.try_length as usize
}

/// Le gestionnaire de `clause` contient-il `ip` ?
pub fn handler_covers(clause: &ExceptionClause, ip: usize) -> bool {
    let start = clause.handler_offset as usize;
    ip >= start && ip < start + clause.handler_length as usize
}

pub fn is_catch(clause: &ExceptionClause) -> bool {
    matches!(clause.flags, CLAUSE_EXCEPTION | CLAUSE_FILTER)
}

pub fn is_filter(clause: &ExceptionClause) -> bool {
    clause.flags == CLAUSE_FILTER
}

pub fn is_finally(clause: &ExceptionClause) -> bool {
    matches!(clause.flags, CLAUSE_FINALLY | CLAUSE_FAULT)
}

/// Gestionnaire le plus interne contenant `ip` parmi les clauses retenues
/// par `kind`. Les clauses sont rangées de la plus interne à la plus
/// externe (ECMA-335 §II.19).
pub fn enclosing_handler(
    clauses: &[ExceptionClause],
    ip: usize,
    kind: impl Fn(&ExceptionClause) -> bool,
) -> Option<&ExceptionClause> {
    clauses.iter().find(|c| kind(c) && handler_covers(c, ip))
}

/// Bloc finally à exécuter avant qu'un `leave` depuis `ip` atteigne `target`.
pub fn find_leave_target(clauses: &[ExceptionClause], ip: usize, target: usize) -> Option<usize> {
    clauses.iter().find_map(|clause| {
        let leaves = try_covers(clause, ip) && !try_covers(clause, target);
        (clause.flags == CLAUSE_FINALLY && leaves).then_some(clause.handler_offset as usize)
    })
}
