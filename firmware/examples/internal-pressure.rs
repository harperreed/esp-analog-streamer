// ABOUTME: Fragments real internal RAM while the unchanged firmware handles listener admission.
// ABOUTME: Records actual stack allocation failures without exhausting the aggregate internal heap.

// Flash with scripts/flash --example internal-pressure, with Shazam disabled and no listeners.
// During INTERNAL_READY request FLAC: require no HTTP 200, ENOMEM and checked socket/slot release.
// After INTERNAL_RELEASED require two decoded FLAC streams and the third-client 503.
// Capture must progress without new faults throughout. This is a sampled experiment, not a
// universal reserve guarantee: other tasks can allocate between the floor check and allocation.

use std::io;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys;

#[path = "../src/threads.rs"]
mod threads;

const CAPS: u32 = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
const FLOOR: usize = sys::CONFIG_SPIRAM_MALLOC_RESERVE_INTERNAL as usize;
const STACK: usize = 4096;
const HOLE: usize = 3072;
const GUARD: usize = 512;
const MAX_OWNERS: usize = 256;
static STACK_FAILURES: AtomicU32 = AtomicU32::new(0);
static OTHER_FAILURES: AtomicU32 = AtomicU32::new(0);

// This separate fixture is the sole registration owner; production registers no hook.
// No logging, allocation, or dereference of the allocator's function-name pointer occurs here.
unsafe extern "C" fn allocation_failed(size: usize, caps: u32, _: *const core::ffi::c_char) {
    if size == STACK && caps == CAPS {
        STACK_FAILURES.fetch_add(1, Ordering::Relaxed);
    } else {
        OTHER_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
}

struct Owner {
    address: usize,
    bytes: usize,
}

impl Drop for Owner {
    fn drop(&mut self) {
        if self.address != 0 {
            // SAFETY: each live address is owned once and came from heap_caps_malloc.
            unsafe { sys::heap_caps_free((self.address as *mut u8).cast()) };
        }
    }
}

fn heap() -> (usize, usize) {
    // SAFETY: these getters read allocator statistics only.
    unsafe {
        (
            sys::heap_caps_get_free_size(CAPS),
            sys::heap_caps_get_largest_free_block(CAPS),
        )
    }
}

#[derive(Default)]
struct Observations {
    allocations: usize,
    max_allocated: usize,
    max_free_delta: usize,
}

fn allocate(
    owners: &mut Vec<Owner>,
    bytes: usize,
    minimum: &mut usize,
    observations: &mut Observations,
) -> io::Result<bool> {
    let before = heap().0;
    *minimum = (*minimum).min(before);
    // Twice the requested size conservatively covers alignment/header/unsplittable tail
    // for these KiB-sized TLSF blocks. Actual free is checked again after allocation.
    if before < FLOOR + 2 * bytes {
        return Ok(false);
    }
    if owners.len() == MAX_OWNERS {
        return Err(io::Error::other("bounded owner metadata exhausted"));
    }
    // SAFETY: strict internal byte-addressable allocation; null is handled without fallback.
    let ptr = unsafe { sys::heap_caps_malloc(bytes, CAPS) }.cast::<u8>();
    if ptr.is_null() {
        return Err(io::Error::other(
            "fixture allocation failed before geometry was formed",
        ));
    }
    let owner = Owner {
        address: ptr as usize,
        bytes,
    };
    // SAFETY: ptr belongs to this live allocation.
    let allocated = unsafe { sys::heap_caps_get_allocated_size(ptr.cast()) };
    // SAFETY: both endpoints belong to this live allocation; getters inspect addresses/size.
    if unsafe {
        sys::esp_ptr_external_ram(ptr.cast())
            || sys::esp_ptr_external_ram(ptr.add(bytes - 1).cast())
            || !sys::esp_ptr_byte_accessible(ptr.cast())
            || !sys::esp_ptr_byte_accessible(ptr.add(bytes - 1).cast())
            || allocated < bytes
    } {
        return Err(io::Error::other(
            "fixture allocation capability/size mismatch",
        ));
    }
    owners.push(owner);
    let after = heap().0;
    *minimum = (*minimum).min(after);
    let free_delta = before.saturating_sub(after);
    observations.allocations += 1;
    observations.max_allocated = observations.max_allocated.max(allocated);
    observations.max_free_delta = observations.max_free_delta.max(free_delta);
    // Other tasks can affect this sampled delta; exceeding the guard fails conservatively.
    if allocated > 2 * bytes || free_delta > 2 * bytes {
        return Err(io::Error::other(
            "observed footprint exceeds conservative allocation guard",
        ));
    }
    if after < FLOOR {
        return Err(io::Error::other(
            "sampled experimental floor crossed; unwinding",
        ));
    }
    std::thread::sleep(Duration::from_millis(10));
    Ok(true)
}

fn pressure() -> io::Result<()> {
    let mut owners = Vec::<Owner>::with_capacity(MAX_OWNERS);
    // Preallocate bounded internal metadata before fragmentation; no owner-list growth follows.
    // SAFETY: this nonempty Vec allocation has a live address.
    if unsafe {
        sys::esp_ptr_external_ram(owners.as_ptr().cast())
            || !sys::esp_ptr_byte_accessible(owners.as_ptr().cast())
    } {
        return Err(io::Error::other("owner metadata is not internal"));
    }
    let baseline = heap();
    let mut minimum = baseline.0;
    let mut observations = Observations::default();
    log::info!("internal-pressure: INTERNAL_START free={} largest={} experiment_floor={} metadata_bytes={}",
        baseline.0, baseline.1, FLOOR, owners.capacity() * std::mem::size_of::<Owner>());
    while heap().0 >= FLOOR + 2 * (HOLE + GUARD) {
        if !allocate(&mut owners, HOLE, &mut minimum, &mut observations)?
            || !allocate(&mut owners, GUARD, &mut minimum, &mut observations)?
        {
            break;
        }
    }
    owners.sort_unstable_by_key(|owner| owner.address);
    // Retain boundary allocations and guards. Only release holes whose sorted neighbors
    // are guards; actual largest/free statistics below decide whether geometry succeeded.
    let mut released = 0;
    for index in 1..owners.len().saturating_sub(1) {
        if owners[index].bytes == HOLE
            && owners[index - 1].bytes == GUARD
            && owners[index + 1].bytes == GUARD
        {
            // SAFETY: this owner is live and becomes empty immediately after its single free.
            unsafe { sys::heap_caps_free((owners[index].address as *mut u8).cast()) };
            owners[index].address = 0;
            released += 1;
        }
    }
    // These larger fillers cannot occupy the intentionally smaller isolated holes.
    while heap().1 >= STACK && allocate(&mut owners, STACK, &mut minimum, &mut observations)? {}
    let ready = heap();
    if released == 0 || ready.0 < FLOOR || ready.1 >= STACK {
        return Err(io::Error::other(
            "cannot form stack-sized fragmentation above experimental floor",
        ));
    }
    let failures = STACK_FAILURES.load(Ordering::Relaxed);
    log::info!("internal-pressure: INTERNAL_READY hold_seconds=20 free={} largest={} experiment_floor={} sampled_min={} holes={} stack_failures={} allocation_count={} max_allocated={} max_free_delta={}",
        ready.0, ready.1, FLOOR, minimum, released, failures, observations.allocations, observations.max_allocated, observations.max_free_delta);
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(100));
        minimum = minimum.min(heap().0);
        if minimum < FLOOR {
            return Err(io::Error::other(
                "request window crossed sampled experimental floor",
            ));
        }
    }
    let held = heap();
    let stack_failures = STACK_FAILURES
        .load(Ordering::Relaxed)
        .wrapping_sub(failures);
    owners.clear();
    let restored = heap();
    log::info!("internal-pressure: INTERNAL_RELEASED held_free={} held_largest={} sampled_min={} free={} largest={} stack_failures={} other_failures={} baseline_free={} (live application allocations may change baseline)",
        held.0, held.1, minimum, restored.0, restored.1, stack_failures,
        OTHER_FAILURES.load(Ordering::Relaxed), baseline.0);
    if stack_failures == 0 {
        return Err(io::Error::other(
            "no real 4096-byte INTERNAL|8BIT failure observed",
        ));
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    threads::ThreadSpec {
        name: c"internal-pressure",
        core: Core::Core1,
        priority: 4,
        stack_bytes: STACK,
    }
    .spawn(|| {
        std::thread::sleep(Duration::from_secs(15));
        // SAFETY: generated callback ABI; this fixture alone owns process-global registration.
        let result =
            unsafe { sys::heap_caps_register_failed_alloc_callback(Some(allocation_failed)) };
        if result != sys::ESP_OK {
            log::error!("internal-pressure: INTERNAL_FAILED callback registration result={result}");
            return;
        }
        if let Err(error) = pressure() {
            log::error!("internal-pressure: INTERNAL_FAILED {error}; retained owners released");
        }
    })?;
    esp_shazam_firmware::run()
}
