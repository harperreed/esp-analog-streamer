// ABOUTME: Exercises the production PSRAM owner against real exhaustion and fragmentation on the board.
// ABOUTME: Starts no capture or network; allocator failure does not establish HTTP admission behavior.

use std::io;
use std::time::Duration;

use esp_idf_svc::sys;
use esp_shazam_core::flac::MAX_FRAME_BYTES;

#[path = "../src/psram.rs"]
mod psram;

const CHUNK_BYTES: usize = 16 * 1024;
const MAX_CHUNKS: usize = 320;
const INTERNAL_FLOOR: usize = 64 * 1024;

struct Chunk {
    bytes: Option<psram::PsramBytes>,
    address: usize,
    len: usize,
}

fn main() {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    check();
    log::info!(
        "psram-check: allocator assertions passed; HTTP 503 under pressure remains untested"
    );
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn free(caps: u32) -> usize {
    // SAFETY: this getter reads heap statistics and does not change allocations.
    unsafe { sys::heap_caps_get_free_size(caps) }
}

fn require_failure(label: &str) {
    let internal = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
    let before = free(internal);
    assert!(
        before >= INTERNAL_FLOOR,
        "internal memory must remain healthy"
    );
    match psram::PsramBytes::try_new(MAX_FRAME_BYTES) {
        Ok(_) => panic!("{label}: frame allocation unexpectedly succeeded"),
        Err(error) => assert_eq!(error.kind(), io::ErrorKind::OutOfMemory, "{label}: {error}"),
    }
    // Error construction may use small temporary metadata; it must not retain a frame-sized fallback.
    let after = free(internal);
    assert!(
        after + 256 >= before,
        "{label}: internal memory unexpectedly consumed"
    );
    log::info!(
        "psram-check: {label} refused {} B; internal_before={} internal_after={}",
        MAX_FRAME_BYTES,
        before,
        after
    );
}

fn check() {
    let caps = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
    // The production owner checks allocated size and both PSRAM endpoints before initialization.
    let mut frame = psram::PsramBytes::try_new(MAX_FRAME_BYTES).expect("initial frame allocation");
    frame.as_mut_slice().fill(0x5a);
    assert!(frame.as_mut_slice().iter().all(|&byte| byte == 0x5a));
    drop(frame);
    let mut chunks = Vec::<Chunk>::with_capacity(MAX_CHUNKS);
    assert!(chunks.capacity() * std::mem::size_of::<Chunk>() <= 8 * 1024);
    // SAFETY: chunks owns a nonempty metadata allocation; the getter checks its address only.
    assert!(
        !unsafe { sys::esp_ptr_external_ram(chunks.as_ptr().cast()) },
        "metadata must remain internal"
    );
    // Establish the restoration baseline after logger warmup and bounded retained metadata.
    let baseline = free(caps);
    let mut size = CHUNK_BYTES;
    while size > 0 {
        match psram::PsramBytes::try_new(size) {
            Ok(mut bytes) => {
                assert!(chunks.len() < MAX_CHUNKS, "bounded metadata exhausted");
                let address = bytes.as_mut_slice().as_ptr() as usize;
                chunks.push(Chunk {
                    bytes: Some(bytes),
                    address,
                    len: size,
                });
            }
            Err(error) => {
                assert_eq!(error.kind(), io::ErrorKind::OutOfMemory);
                size /= 2;
            }
        }
    }
    log::info!(
        "psram-check: exhausted with {} retained chunks; psram_free={}",
        chunks.len(),
        free(caps)
    );
    require_failure("exhaustion");

    // Keep every other full-sized block and every residual-tail block allocated. Sorting by
    // address prevents allocation order from disguising adjacency; avoid both end blocks.
    chunks.sort_unstable_by_key(|chunk| chunk.address);
    let count = chunks.len();
    let mut released = 0;
    for (index, chunk) in chunks.iter_mut().enumerate() {
        if index > 0 && index + 1 < count && index % 2 == 1 && chunk.len == CHUNK_BYTES {
            drop(chunk.bytes.take());
            released += 1;
        }
    }
    let available = free(caps);
    // SAFETY: reads the allocator's actual largest eligible block, rather than estimating holes.
    let largest = unsafe { sys::heap_caps_get_largest_free_block(caps) };
    log::info!(
        "psram-check: fragmentation released={} free={} largest={} requested={}",
        released,
        available,
        largest,
        MAX_FRAME_BYTES
    );
    assert!(
        available > MAX_FRAME_BYTES,
        "fragmentation must leave enough aggregate free bytes"
    );
    assert!(
        largest < MAX_FRAME_BYTES,
        "fragmentation must leave no frame-sized contiguous block"
    );
    require_failure("fragmentation");
    chunks.clear();
    let restored = free(caps);
    log::info!(
        "psram-check: baseline_psram_free={} restored_psram_free={}",
        baseline,
        restored
    );
    assert_eq!(
        restored, baseline,
        "all strict PSRAM allocations must be reclaimed"
    );
}
