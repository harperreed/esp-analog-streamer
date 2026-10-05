// ABOUTME: Fragments real internal RAM while the unchanged firmware handles listener admission.
// ABOUTME: Probes actual stack-sized allocation failure without exhausting the aggregate internal heap.

// Flash with scripts/flash --example internal-pressure, with Shazam disabled and no listeners.
// During INTERNAL_READY request FLAC: require no HTTP 200, ENOMEM and checked socket/slot release.
// After INTERNAL_RELEASED require two decoded FLAC streams and the third-client 503.
// Capture must progress without new faults throughout. This is a sampled experiment, not a
// universal reserve guarantee: other tasks can allocate between the floor check and allocation.

use std::io;
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

fn stack_probe() -> io::Result<()> {
    // SAFETY: same strict capabilities and 4096-byte request as the listener pthread stack.
    let ptr = unsafe { sys::heap_caps_malloc(STACK, CAPS) };
    if !ptr.is_null() {
        // SAFETY: the unexpected successful allocation is still owned here and freed once.
        unsafe { sys::heap_caps_free(ptr) };
        return Err(io::Error::other(
            "strict stack probe unexpectedly succeeded",
        ));
    }
    Ok(())
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
    minimum = minimum.min(ready.0);
    if released == 0 || ready.0 < FLOOR || ready.1 >= STACK {
        return Err(io::Error::other(
            "cannot form stack-sized fragmentation above experimental floor",
        ));
    }
    stack_probe()?;
    let ready_after = heap();
    minimum = minimum.min(ready_after.0);
    if ready_after.0 < FLOOR || ready_after.1 >= STACK {
        return Err(io::Error::other("geometry changed during READY probe"));
    }
    log::info!("internal-pressure: INTERNAL_READY hold_seconds=20 free={} largest={} experiment_floor={} sampled_min={} holes={} stack_probe_failed=true probe_before_free={} probe_before_largest={} allocation_count={} max_allocated={} max_free_delta={}",
        ready_after.0, ready_after.1, FLOOR, minimum, released, ready.0, ready.1, observations.allocations, observations.max_allocated, observations.max_free_delta);
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
    minimum = minimum.min(held.0);
    if held.0 < FLOOR || held.1 >= STACK {
        return Err(io::Error::other("invalid geometry before END probe"));
    }
    stack_probe()?;
    let end_after = heap();
    minimum = minimum.min(end_after.0);
    if end_after.0 < FLOOR || end_after.1 >= STACK {
        return Err(io::Error::other("geometry changed during END probe"));
    }
    log::info!("internal-pressure: INTERNAL_END free={} largest={} experiment_floor={} sampled_min={} stack_probe_failed=true probe_before_free={} probe_before_largest={}",
        end_after.0, end_after.1, FLOOR, minimum, held.0, held.1);
    owners.clear();
    let restored = heap();
    log::info!("internal-pressure: INTERNAL_RELEASED sampled_min={} free={} largest={} baseline_free={} (live application allocations may change baseline)",
        minimum, restored.0, restored.1, baseline.0);
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
        if let Err(error) = pressure() {
            log::error!("internal-pressure: INTERNAL_FAILED {error}; retained owners released");
        }
    })?;
    esp_shazam_firmware::run()
}
