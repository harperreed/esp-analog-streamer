// ABOUTME: Owns a fallible byte buffer allocated strictly in byte-addressable PSRAM.
// ABOUTME: Keeps listener frame storage out of internal RAM and frees it through ESP-IDF.

use std::io;
use std::ptr::NonNull;

use esp_idf_svc::sys;

pub struct PsramBytes {
    ptr: NonNull<u8>,
    len: usize,
}

impl PsramBytes {
    /// Allocates and initializes bytes without falling back to internal RAM.
    /// Empty buffers do not allocate; lengths beyond Rust's slice limit are rejected.
    pub fn try_new(len: usize) -> io::Result<Self> {
        if len > isize::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PSRAM buffer exceeds slice length limit",
            ));
        }
        if len == 0 {
            return Ok(Self {
                ptr: NonNull::dangling(),
                len: 0,
            });
        }
        // SAFETY: explicit capabilities request writable byte-addressable PSRAM.
        // No internal-memory fallback is requested. A null allocation is handled below.
        let ptr =
            unsafe { sys::heap_caps_malloc(len, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) };
        let ptr = NonNull::new(ptr.cast::<u8>()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::OutOfMemory, "PSRAM byte allocation failed")
        })?;
        let buffer = Self { ptr, len };
        // SAFETY: ptr is a live allocation returned by heap_caps_malloc.
        let allocated = unsafe { sys::heap_caps_get_allocated_size(ptr.as_ptr().cast()) };
        if allocated < len {
            return Err(io::Error::other(
                "PSRAM allocation is smaller than requested",
            ));
        }
        // SAFETY: both pointers lie within the live allocation, whose size was checked above.
        let external = unsafe {
            sys::esp_ptr_external_ram(ptr.as_ptr().cast())
                && sys::esp_ptr_external_ram(ptr.as_ptr().add(len - 1).cast())
        };
        if !external {
            return Err(io::Error::other("frame allocation is outside PSRAM"));
        }
        // SAFETY: the allocation owns len writable bytes; initialization makes slice reads valid.
        unsafe { ptr.as_ptr().write_bytes(0, len) };
        log::info!(
            "stream: PSRAM buffer {len} B, allocated {allocated} B at {:p}",
            ptr.as_ptr()
        );
        Ok(buffer)
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: ptr is nonnull and aligned for u8, including the empty dangling case.
        // Nonempty storage contains len initialized bytes. The mutable borrow is exclusive
        // and its lifetime cannot outlive this owner; len is bounded by isize::MAX.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

// SAFETY: this owner has no aliases and exposes storage only through exclusive borrows.
// ESP-IDF's heap allocation/free may run on either core; moving ownership is safe.
unsafe impl Send for PsramBytes {}

impl Drop for PsramBytes {
    fn drop(&mut self) {
        if self.len != 0 {
            // SAFETY: ptr came from heap_caps_malloc and this sole owner frees it once.
            unsafe { sys::heap_caps_free(self.ptr.as_ptr().cast()) };
        }
    }
}
