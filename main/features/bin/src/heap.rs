//! mmap-backed global allocator for no_std builds.

use linked_list_allocator::LockedHeap;

const HEAP_SIZE: usize = 4 * 1024 * 1024; // 4 MB

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init() {
    let heap_start = unsafe { crate::ffi::mmap_anon(HEAP_SIZE) };
    // MAP_FAILED is (void *)-1; treat null as failure too.
    if heap_start.is_null() || heap_start as isize == -1 {
        unsafe { crate::ffi::exit_group(99) }
    }
    unsafe { ALLOCATOR.lock().init(heap_start, HEAP_SIZE); }
}
