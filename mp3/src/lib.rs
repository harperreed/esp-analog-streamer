// ABOUTME: Owns one fixed-format MP3 encoder and its allocation policy.
// ABOUTME: Produces whole independent 48 kHz stereo 320 kbps frames.
use std::ffi::c_void;
use std::ptr::NonNull;

pub const SAMPLE_RATE_HZ: usize = 48_000;
pub const CHANNELS: usize = 2;
pub const SAMPLE_FRAMES: usize = 1152;
pub const PCM_SAMPLES: usize = SAMPLE_FRAMES * CHANNELS;
pub const FRAME_BYTES: usize = 960;

/// Allocation callbacks and context, owned by the caller for the encoder lifetime.
#[repr(C)]
pub struct Allocator {
    pub context: *mut c_void,
    pub allocate: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
    pub deallocate: unsafe extern "C" fn(*mut c_void, *mut c_void),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Allocation,
    Encode,
    Frame,
}
impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Allocation => "MP3 encoder allocation failed",
            Self::Encode => "MP3 encoding failed",
            Self::Frame => "MP3 encoder produced an invalid independent frame",
        })
    }
}
impl std::error::Error for Error {}

/// One encoder epoch. Recreate after a capture discontinuity; no tail drain is supplied.
pub struct Encoder {
    state: NonNull<c_void>,
}
// SAFETY: State is owned exclusively; custom allocators must permit ownership transfer.
unsafe impl Send for Encoder {}

extern "C" {
    fn shazam_mp3_open(allocator: *const Allocator) -> *mut c_void;
    fn shazam_mp3_encode(encoder: *mut c_void, pcm: *mut i16, output: *mut u8) -> i32;
    fn shazam_mp3_close(encoder: *mut c_void);
}
impl Encoder {
    pub fn new() -> Result<Self, Error> {
        // SAFETY: A null descriptor requests the C runtime allocator.
        Self::from_pointer(unsafe { shazam_mp3_open(std::ptr::null()) })
    }

    /// Creates an encoder using a caller-supplied per-instance allocation policy.
    ///
    /// # Safety
    /// Callbacks must not unwind. Allocation must return null or distinct writable
    /// storage of the requested size, aligned at least as strictly as C malloc.
    /// Deallocation must release that storage exactly once. The context and its
    /// referenced storage must remain valid until this encoder is dropped, including
    /// after transfer to another thread. Callbacks must allow that transfer; sharing
    /// a context between encoders requires caller-provided synchronization.
    pub unsafe fn with_allocator(allocator: Allocator) -> Result<Self, Error> {
        // SAFETY: The caller supplies the documented callback and lifetime guarantees.
        Self::from_pointer(unsafe { shazam_mp3_open(&allocator) })
    }

    fn from_pointer(pointer: *mut c_void) -> Result<Self, Error> {
        NonNull::new(pointer)
            .map(|state| Self { state })
            .ok_or(Error::Allocation)
    }

    pub fn encode(
        &mut self,
        pcm: &mut [i16; PCM_SAMPLES],
        output: &mut [u8; FRAME_BYTES],
    ) -> Result<(), Error> {
        // SAFETY: Exclusive encoder and fixed-size mutable buffers satisfy the C API.
        match unsafe {
            shazam_mp3_encode(self.state.as_ptr(), pcm.as_mut_ptr(), output.as_mut_ptr())
        } {
            0 => Ok(()),
            2 => Err(Error::Frame),
            _ => Err(Error::Encode),
        }
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: This is the sole owner and the state has not been freed.
        unsafe { shazam_mp3_close(self.state.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    #[repr(C)]
    struct Bitstream {
        data: *mut u8,
        data_size: i32,
        data_position: i32,
        cache: u32,
        failed: i32,
        cache_bits: i32,
    }
    extern "C" {
        fn shine_open_bit_stream(stream: *mut Bitstream, data: *mut u8, size: i32);
        fn shine_putbits(stream: *mut Bitstream, value: u32, bits: u32);
    }
    #[test]
    fn zero_bit_write_does_not_change_cache() {
        let mut storage = [0_u32; 1];
        let mut stream = std::mem::MaybeUninit::<Bitstream>::uninit();
        // SAFETY: Aligned backing storage outlives this initialized stream.
        unsafe {
            shine_open_bit_stream(stream.as_mut_ptr(), storage.as_mut_ptr().cast(), 4);
            let mut stream = stream.assume_init();
            shine_putbits(&mut stream, 1, 0);
            assert_eq!(stream.cache, 0);
            assert_eq!(stream.cache_bits, 32);
        }
    }
    #[test]
    fn bitstream_capacity_failure_is_sticky_and_preserves_guards() {
        let mut storage = [0x12345678_u32; 3];
        let mut stream = std::mem::MaybeUninit::<Bitstream>::uninit();
        // SAFETY: The four-byte middle element is aligned writable C storage.
        unsafe {
            shine_open_bit_stream(stream.as_mut_ptr(), storage.as_mut_ptr().add(1).cast(), 4);
            let mut stream = stream.assume_init();
            shine_putbits(&mut stream, 0, 32);
            assert_eq!(stream.failed, 0);
            assert_eq!(stream.data_position, 4);
            shine_putbits(&mut stream, u32::MAX, 32);
            assert_eq!(stream.failed, 1);
            shine_putbits(&mut stream, u32::MAX, 32);
            assert_eq!(stream.data_position, 4);
        }
        assert_eq!(storage[0], 0x12345678);
        assert_eq!(storage[2], 0x12345678);
    }
}
