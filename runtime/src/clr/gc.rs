//! Ramasse-miettes mark-and-sweep pour les objets managés .NET.
//!
//! Les racines sont fournies par l'interpréteur au moment de la collecte
//! (piles d'évaluation, variables locales, arguments, champs statiques) ;
//! le contenu des objets est parcouru de façon conservatrice.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::HashSet;
use std::sync::Mutex;

const OBJ_ALIGN: usize = 8;
const MARK_BIT: u32 = 1;
const HEADER: usize = std::mem::size_of::<ObjHeader>();

#[repr(C)]
pub struct ObjHeader {
    pub type_id: u32,
    pub flags: u32,
    pub size: u32,
    _pad: u32,
}

pub struct GcHeap {
    objects: HashSet<usize>,
    total_allocated: usize,
    gc_threshold: usize,
}

impl GcHeap {
    fn new() -> Self {
        GcHeap {
            objects: HashSet::new(),
            total_allocated: 0,
            gc_threshold: 4 * 1024 * 1024,
        }
    }

    pub fn alloc(&mut self, type_id: u32, data_size: usize) -> *mut u8 {
        let total = HEADER + align_up(data_size.max(OBJ_ALIGN), OBJ_ALIGN);
        let layout = Layout::from_size_align(total, OBJ_ALIGN).unwrap();
        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        let hdr = ptr as *mut ObjHeader;
        unsafe {
            (*hdr).type_id = type_id;
            (*hdr).size = total as u32;
        }
        self.objects.insert(ptr as usize);
        self.total_allocated += total;
        unsafe { ptr.add(HEADER) }
    }

    pub fn alloc_array(&mut self, type_id: u32, elem_size: usize, length: usize) -> *mut u8 {
        let ptr = self.alloc(type_id, 8 + elem_size * length);
        unsafe {
            *(ptr as *mut u64) = length as u64;
        }
        ptr
    }

    pub fn is_object(&self, data_ptr: usize) -> bool {
        data_ptr >= HEADER && self.objects.contains(&(data_ptr - HEADER))
    }

    pub fn should_collect(&self) -> bool {
        self.total_allocated >= self.gc_threshold
    }

    pub fn collect(&mut self, roots: &[usize]) {
        let mut work: Vec<usize> = roots
            .iter()
            .copied()
            .filter(|&r| self.is_object(r))
            .collect();
        while let Some(data) = work.pop() {
            let hdr = (data - HEADER) as *mut ObjHeader;
            let (flags, size) = unsafe { ((*hdr).flags, (*hdr).size as usize) };
            if flags & MARK_BIT != 0 {
                continue;
            }
            unsafe { (*hdr).flags |= MARK_BIT };
            for i in 0..(size - HEADER) / 8 {
                let slot = unsafe { *((data + i * 8) as *const usize) };
                if self.is_object(slot) {
                    work.push(slot);
                }
            }
        }

        let mut freed = 0usize;
        self.objects.retain(|&obj| {
            let hdr = obj as *mut ObjHeader;
            if unsafe { (*hdr).flags } & MARK_BIT != 0 {
                unsafe { (*hdr).flags &= !MARK_BIT };
                return true;
            }
            let size = unsafe { (*hdr).size } as usize;
            unsafe {
                dealloc(
                    obj as *mut u8,
                    Layout::from_size_align(size, OBJ_ALIGN).unwrap(),
                )
            };
            freed += size;
            false
        });

        self.total_allocated = self.total_allocated.saturating_sub(freed);
        if self.total_allocated > self.gc_threshold / 2 {
            self.gc_threshold = self.gc_threshold.saturating_mul(2);
        }
        crate::log_debug!(
            "clr-gc",
            "collecte : {} octets libérés, {} objets restants",
            freed,
            self.objects.len()
        );
    }

    pub fn total_allocated(&self) -> usize {
        self.total_allocated
    }
}

impl Drop for GcHeap {
    fn drop(&mut self) {
        for &obj in &self.objects {
            let size = unsafe { (*(obj as *const ObjHeader)).size } as usize;
            unsafe {
                dealloc(
                    obj as *mut u8,
                    Layout::from_size_align(size, OBJ_ALIGN).unwrap(),
                )
            };
        }
    }
}

static GC: Mutex<Option<GcHeap>> = Mutex::new(None);

pub fn init() {
    *GC.lock().unwrap() = Some(GcHeap::new());
}

pub fn with_gc<T>(f: impl FnOnce(&mut GcHeap) -> T) -> T {
    let mut lock = GC.lock().unwrap();
    let gc = lock.as_mut().expect("CLR GC non initialisé");
    f(gc)
}

/// Type d'un objet managé, ou `None` si `ptr` n'est pas un objet du tas.
pub fn type_of(ptr: *mut u8) -> Option<u32> {
    if ptr.is_null() || !with_gc(|gc| gc.is_object(ptr as usize)) {
        return None;
    }
    Some(unsafe { (*(ptr.sub(HEADER) as *const ObjHeader)).type_id })
}

fn align_up(v: usize, a: usize) -> usize {
    (v + a - 1) & !(a - 1)
}
