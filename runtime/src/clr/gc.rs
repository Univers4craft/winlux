//! Ramasse-miettes simple (mark-and-sweep) pour les objets managés .NET.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::sync::Mutex;

const OBJ_ALIGN: usize = 8;
const MARK_BIT: u32 = 1;

#[repr(C)]
pub struct ObjHeader {
    pub type_id: u32,
    pub flags: u32,
    pub size: u32,
    _pad: u32,
}

pub struct GcHeap {
    objects: Vec<*mut ObjHeader>,
    roots: Vec<*mut u8>,
    total_allocated: usize,
    gc_threshold: usize,
}

unsafe impl Send for GcHeap {}

impl GcHeap {
    pub fn new() -> Self {
        GcHeap {
            objects: Vec::new(),
            roots: Vec::new(),
            total_allocated: 0,
            gc_threshold: 4 * 1024 * 1024,
        }
    }

    pub fn alloc(&mut self, type_id: u32, data_size: usize) -> *mut u8 {
        let total = std::mem::size_of::<ObjHeader>() + align_up(data_size, OBJ_ALIGN);
        let layout = Layout::from_size_align(total, OBJ_ALIGN).unwrap();
        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            self.collect();
            let ptr = unsafe { alloc_zeroed(layout) };
            if ptr.is_null() {
                panic!("CLR GC: mémoire épuisée");
            }
            self.init_obj(ptr, type_id, total);
            return unsafe { ptr.add(std::mem::size_of::<ObjHeader>()) };
        }
        self.init_obj(ptr, type_id, total);
        unsafe { ptr.add(std::mem::size_of::<ObjHeader>()) }
    }

    fn init_obj(&mut self, ptr: *mut u8, type_id: u32, total: usize) {
        let hdr = ptr as *mut ObjHeader;
        unsafe {
            (*hdr).type_id = type_id;
            (*hdr).flags = 0;
            (*hdr).size = total as u32;
        }
        self.objects.push(hdr);
        self.total_allocated += total;
    }

    pub fn alloc_array(&mut self, type_id: u32, elem_size: usize, length: usize) -> *mut u8 {
        let data_size = 8 + elem_size * length;
        let ptr = self.alloc(type_id, data_size);
        unsafe {
            *(ptr as *mut u64) = length as u64;
        }
        ptr
    }

    pub fn add_root(&mut self, ptr: *mut u8) {
        self.roots.push(ptr);
    }

    pub fn remove_root(&mut self, ptr: *mut u8) {
        self.roots.retain(|&r| r != ptr);
    }

    pub fn should_collect(&self) -> bool {
        self.total_allocated >= self.gc_threshold
    }

    pub fn collect(&mut self) {
        for obj in &self.objects {
            unsafe { (**obj).flags &= !MARK_BIT; }
        }

        for &root in &self.roots {
            if !root.is_null() {
                self.mark(root);
            }
        }

        let mut freed = 0usize;
        self.objects.retain(|&obj| {
            let marked = unsafe { (*obj).flags & MARK_BIT != 0 };
            if !marked {
                let size = unsafe { (*obj).size } as usize;
                let layout = Layout::from_size_align(size, OBJ_ALIGN).unwrap();
                unsafe { dealloc(obj as *mut u8, layout); }
                freed += size;
            }
            marked
        });

        self.total_allocated = self.total_allocated.saturating_sub(freed);
        if self.total_allocated > self.gc_threshold / 2 {
            self.gc_threshold = self.gc_threshold.saturating_mul(2);
        }

        crate::log_debug!("clr-gc", "collecte : {} octets libérés, {} objets restants",
            freed, self.objects.len());
    }

    fn mark(&self, data_ptr: *mut u8) {
        let hdr_ptr = unsafe { data_ptr.sub(std::mem::size_of::<ObjHeader>()) } as *mut ObjHeader;
        if !self.objects.contains(&hdr_ptr) {
            return;
        }
        let hdr = unsafe { &mut *hdr_ptr };
        if hdr.flags & MARK_BIT != 0 {
            return;
        }
        hdr.flags |= MARK_BIT;
        // Scan conservatif des champs de l'objet pour trouver des références
        let data_size = hdr.size as usize - std::mem::size_of::<ObjHeader>();
        let num_ptrs = data_size / 8;
        for i in 0..num_ptrs {
            let slot = unsafe { *(data_ptr.add(i * 8) as *const usize) };
            if slot != 0 {
                let candidate = slot as *mut u8;
                let candidate_hdr = unsafe {
                    candidate.sub(std::mem::size_of::<ObjHeader>())
                } as *mut ObjHeader;
                if self.objects.contains(&candidate_hdr) {
                    self.mark(candidate);
                }
            }
        }
    }

    pub fn stats(&self) -> (usize, usize) {
        (self.objects.len(), self.total_allocated)
    }
}

impl Drop for GcHeap {
    fn drop(&mut self) {
        for &obj in &self.objects {
            let size = unsafe { (*obj).size } as usize;
            let layout = Layout::from_size_align(size, OBJ_ALIGN).unwrap();
            unsafe { dealloc(obj as *mut u8, layout); }
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

fn align_up(v: usize, a: usize) -> usize {
    (v + a - 1) & !(a - 1)
}
