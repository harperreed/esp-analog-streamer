// ABOUTME: Runs the real firmware while a separate observer creates temporary PSRAM fragmentation.
// ABOUTME: PRESSURE_READY marks a 20 s window for real FLAC HTTP 503 checks, followed by recovery checks.

// Flash with scripts/flash --example listener-pressure; keep cfg.toml shazam_enabled=false.
// Start with no listeners. During PRESSURE_READY request the real /stream.flac endpoint;
// after PRESSURE_RELEASED verify two listeners receive HTTP 200 and decoded audio.
// This fixture creates real allocator pressure, without routes, mocks, or production thresholds.

use std::io;
use std::time::Duration;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::sys;
use esp_shazam_core::flac::MAX_FRAME_BYTES;

#[path = "../src/psram.rs"]
mod psram;
#[path = "../src/threads.rs"]
mod threads;

const CHUNK_BYTES: usize = 16 * 1024;
const MAX_CHUNKS: usize = 320;

struct Chunk {
    bytes: Option<psram::PsramBytes>,
    address: usize,
    len: usize,
}

fn main() -> anyhow::Result<()> {
    threads::ThreadSpec {
        name: c"listener-pressure",
        core: Core::Core1,
        priority: 4,
        stack_bytes: 4 * 1024,
    }
    .spawn(|| {
        std::thread::sleep(Duration::from_secs(15));
        if let Err(error) = pressure() {
            log::error!("listener-pressure: PRESSURE_FAILED {error}; retained owners released");
        }
    })?;
    // The library performs the unchanged production boot and runs its real network supervisor.
    esp_shazam_firmware::run()
}

fn heap() -> (usize, usize, usize, usize) {
    let psram = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
    let internal = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
    // SAFETY: these getters only read allocator statistics; no monitor reset or injection.
    unsafe {
        (
            sys::heap_caps_get_free_size(psram),
            sys::heap_caps_get_largest_free_block(psram),
            sys::heap_caps_get_free_size(internal),
            sys::heap_caps_get_largest_free_block(internal),
        )
    }
}

fn pressure() -> io::Result<()> {
    let mut chunks = Vec::<Chunk>::with_capacity(MAX_CHUNKS);
    // Metadata stays bounded and internal so exhausting PSRAM cannot invalidate this owner list.
    if chunks.capacity() * std::mem::size_of::<Chunk>() > 8 * 1024
        // SAFETY: chunks owns a nonempty allocation; this getter checks its address only.
        || unsafe { sys::esp_ptr_external_ram(chunks.as_ptr().cast()) }
    {
        return Err(io::Error::other(
            "bounded metadata was not allocated internally",
        ));
    }
    let baseline = heap();
    log::info!("listener-pressure: PRESSURE_START psram_free={} psram_largest={} internal_free={} internal_largest={}",
        baseline.0, baseline.1, baseline.2, baseline.3);
    let mut size = CHUNK_BYTES;
    while size > 0 {
        match psram::PsramBytes::try_new(size) {
            Ok(mut bytes) => {
                if chunks.len() == MAX_CHUNKS {
                    return Err(io::Error::other("fragmentation exhausted bounded metadata"));
                }
                let address = bytes.as_mut_slice().as_ptr() as usize;
                chunks.push(Chunk {
                    bytes: Some(bytes),
                    address,
                    len: size,
                });
                // One full tick yields to the real encoder and capture supervisor.
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) if error.kind() == io::ErrorKind::OutOfMemory => size /= 2,
            Err(error) => return Err(error),
        }
    }
    // Free isolated full blocks, retaining both boundary blocks and every smaller tail block.
    // Holes remain usable for small real HTTP/Wi-Fi allocations but cannot hold a FLAC frame.
    chunks.sort_unstable_by_key(|chunk| chunk.address);
    let count = chunks.len();
    let mut released = 0;
    for (index, chunk) in chunks.iter_mut().enumerate() {
        if index > 0 && index + 1 < count && index % 2 == 1 && chunk.len == CHUNK_BYTES {
            drop(chunk.bytes.take());
            released += 1;
        }
    }
    let ready = heap();
    if ready.0 <= MAX_FRAME_BYTES || ready.1 >= MAX_FRAME_BYTES {
        return Err(io::Error::other(format!(
            "cannot form fragmentation state: free={} largest={} requested={} isolated_holes={}",
            ready.0, ready.1, MAX_FRAME_BYTES, released
        )));
    }
    log::info!("listener-pressure: PRESSURE_READY hold_seconds=20 isolated_holes={} requested={} psram_free={} psram_largest={} internal_free={} internal_largest={}",
        released, MAX_FRAME_BYTES, ready.0, ready.1, ready.2, ready.3);
    std::thread::sleep(Duration::from_secs(20));
    let held = heap();
    log::info!("listener-pressure: PRESSURE_END psram_free={} psram_largest={} internal_free={} internal_largest={}",
        held.0, held.1, held.2, held.3);
    chunks.clear();
    let restored = heap();
    log::info!("listener-pressure: PRESSURE_RELEASED psram_free={} psram_largest={} internal_free={} internal_largest={} (real application allocations may change the baseline)",
        restored.0, restored.1, restored.2, restored.3);
    Ok(())
}
