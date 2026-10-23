// ABOUTME: Counts heap allocations on the calling thread inside a bounded closure.
// ABOUTME: Installs a delegating global allocator in whichever test binary includes this module.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

pub struct CaptureAllocator;

// SAFETY: each operation delegates unchanged to System; the thread-local counter
// only observes allocations in the test's bounded capture interval.
unsafe impl GlobalAlloc for CaptureAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        System.realloc(ptr, layout, size)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: CaptureAllocator = CaptureAllocator;

fn count_allocation() {
    let _ = COUNT_ALLOCATIONS.try_with(|count| {
        if count.get() {
            let _ = ALLOCATIONS.try_with(|allocations| allocations.set(allocations.get() + 1));
        }
    });
}

/// Runs `capture` and returns its result with the number of allocations it made on this thread.
pub fn allocations_during<T>(capture: impl FnOnce() -> T) -> (T, usize) {
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            COUNT_ALLOCATIONS.with(|count| count.set(false));
        }
    }

    ALLOCATIONS.with(|allocations| allocations.set(0));
    COUNT_ALLOCATIONS.with(|count| count.set(true));
    let stop = StopCounting;
    let result = capture();
    drop(stop);
    (result, ALLOCATIONS.with(Cell::get))
}
