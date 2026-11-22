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
/// Objet construit comme délégué (ECMA-335 §II.14.6).
const DELEGATE_BIT: u32 = 2;
/// Rang d'un tableau à plusieurs dimensions (bits 8 à 15 des marques ; 0
/// pour un tableau à une dimension indexé depuis 0).
const RANK_SHIFT: u32 = 8;
const RANK_MASK: u32 = 0xFF << RANK_SHIFT;
/// Rang maximal d'un tableau (documentation « Array Class » : 32).
pub const MAX_RANK: usize = 32;
const HEADER: usize = std::mem::size_of::<ObjHeader>();

#[repr(C)]
pub struct ObjHeader {
    pub type_id: u32,
    pub flags: u32,
    pub size: u32,
    /// Type des éléments d'un tableau (0 : références).
    pub extra: u32,
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

    /// Tableau dont les éléments sont du type `elem` (taille `elem_size`).
    pub fn alloc_array_of(&mut self, elem: u32, elem_size: usize, length: usize) -> *mut u8 {
        let ptr = self.alloc_array(super::types::TYPE_ARRAY, elem_size, length);
        unsafe { (*(ptr.sub(HEADER) as *mut ObjHeader)).extra = elem };
        ptr
    }

    /// Tableau à plusieurs dimensions (#2848) : même disposition qu'un
    /// tableau à une dimension (nombre total d'éléments à +0, éléments
    /// rangés ligne par ligne à +8), pour que Copy, Clear, Length et
    /// l'énumération le parcourent à plat comme le fait .NET ; les bornes
    /// (borne inférieure i32, longueur i32 par dimension) sont rangées à la
    /// fin de l'objet et le rang dans les marques.
    pub fn alloc_array_md(
        &mut self,
        elem: u32,
        elem_size: usize,
        bounds: &[(i32, i32)],
    ) -> *mut u8 {
        let total: usize = bounds.iter().map(|&(_, n)| n.max(0) as usize).product();
        let data = align_up(8 + elem_size * total, OBJ_ALIGN) + bounds.len() * 8;
        let ptr = self.alloc(super::types::TYPE_ARRAY, data);
        unsafe {
            *(ptr as *mut u64) = total as u64;
            let hdr = &mut *(ptr.sub(HEADER) as *mut ObjHeader);
            hdr.extra = elem;
            hdr.flags |= ((bounds.len() as u32) << RANK_SHIFT) & RANK_MASK;
            let end = ptr.add(hdr.size as usize - HEADER - bounds.len() * 8) as *mut i32;
            for (d, &(lo, n)) in bounds.iter().enumerate() {
                *end.add(2 * d) = lo;
                *end.add(2 * d + 1) = n;
            }
        }
        ptr
    }

    /// Copie superficielle d'un objet : même type, même taille, mêmes
    /// marques (hors marquage de collecte).
    pub fn clone_object(&mut self, data: *mut u8) -> *mut u8 {
        let hdr = unsafe { &*(data.sub(HEADER) as *const ObjHeader) };
        let (type_id, flags, extra, size) = (hdr.type_id, hdr.flags, hdr.extra, hdr.size as usize);
        let copy = self.alloc(type_id, size - HEADER);
        unsafe {
            std::ptr::copy_nonoverlapping(data, copy, size - HEADER);
            let new_hdr = &mut *(copy.sub(HEADER) as *mut ObjHeader);
            new_hdr.flags = flags & !MARK_BIT;
            new_hdr.extra = extra;
        }
        copy
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
/// Type des éléments d'un tableau créé par `alloc_array_of` (0 : références).
pub fn array_elem_type(ptr: *mut u8) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    unsafe { (*(ptr.sub(HEADER) as *const ObjHeader)).extra }
}

/// Bornes (borne inférieure, longueur) de chaque dimension d'un tableau
/// créé par `alloc_array_md`, ou `None` pour un tableau à une dimension.
pub fn array_bounds(ptr: *mut u8) -> Option<Vec<(i32, i32)>> {
    if ptr.is_null() {
        return None;
    }
    let hdr = unsafe { &*(ptr.sub(HEADER) as *const ObjHeader) };
    let rank = ((hdr.flags & RANK_MASK) >> RANK_SHIFT) as usize;
    if rank == 0 {
        return None;
    }
    let end = unsafe { ptr.add(hdr.size as usize - HEADER - rank * 8) as *const i32 };
    Some(
        (0..rank)
            .map(|d| unsafe { (*end.add(2 * d), *end.add(2 * d + 1)) })
            .collect(),
    )
}

pub fn type_of(ptr: *mut u8) -> Option<u32> {
    if ptr.is_null() || !with_gc(|gc| gc.is_object(ptr as usize)) {
        return None;
    }
    Some(unsafe { (*(ptr.sub(HEADER) as *const ObjHeader)).type_id })
}

/// Marque un objet comme délégué.
pub fn set_delegate(ptr: *mut u8) {
    if with_gc(|gc| gc.is_object(ptr as usize)) {
        unsafe { (*(ptr.sub(HEADER) as *mut ObjHeader)).flags |= DELEGATE_BIT };
    }
}

/// Vrai si l'objet a été construit comme délégué.
pub fn is_delegate(ptr: *mut u8) -> bool {
    with_gc(|gc| gc.is_object(ptr as usize))
        && unsafe { (*(ptr.sub(HEADER) as *const ObjHeader)).flags } & DELEGATE_BIT != 0
}

fn align_up(v: usize, a: usize) -> usize {
    (v + a - 1) & !(a - 1)
}
