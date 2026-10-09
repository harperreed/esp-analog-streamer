// ABOUTME: Selects one shared compressed encoder and listener frame format at build time.
// ABOUTME: Keeps capture scaling, complete-frame publication and reset behavior in one adapter.

use esp_shazam_core::stream_http::Format;
use std::io;

pub const FORMAT: Format = Format::COMPRESSED;
#[cfg(not(feature = "mp3-stream"))]
pub const SAMPLE_RATE_HZ: u32 = esp_shazam_core::flac::SAMPLE_RATE_HZ;
#[cfg(feature = "mp3-stream")]
pub const SAMPLE_RATE_HZ: u32 = esp_shazam_mp3::SAMPLE_RATE_HZ as u32;
#[cfg(not(feature = "mp3-stream"))]
pub const BLOCK_FRAMES: usize = esp_shazam_core::flac::BLOCK_FRAMES;
#[cfg(feature = "mp3-stream")]
pub const BLOCK_FRAMES: usize = esp_shazam_mp3::SAMPLE_FRAMES;
#[cfg(not(feature = "mp3-stream"))]
pub const MAX_FRAME_BYTES: usize = esp_shazam_core::flac::MAX_FRAME_BYTES;
#[cfg(feature = "mp3-stream")]
pub const MAX_FRAME_BYTES: usize = esp_shazam_mp3::FRAME_BYTES;

#[cfg(not(feature = "mp3-stream"))]
pub struct Encoder(esp_shazam_core::flac::Encoder);

#[cfg(not(feature = "mp3-stream"))]
impl Encoder {
    pub fn new() -> io::Result<Self> {
        Ok(Self(esp_shazam_core::flac::Encoder::new()))
    }
    pub fn frames_wanted(&self) -> usize {
        self.0.frames_wanted()
    }
    pub fn push(&mut self, pcm: &[i32]) {
        self.0.push(pcm);
    }
    pub fn encode_block(&mut self, number: u32, bytes: &mut Vec<u8>) -> io::Result<()> {
        self.0.encode_block(number, bytes);
        Ok(())
    }
    pub fn reset(&mut self) -> io::Result<()> {
        self.0.reset();
        Ok(())
    }
}

#[cfg(feature = "mp3-stream")]
pub struct Encoder {
    codec: Option<esp_shazam_mp3::Encoder>,
    pcm: Box<[i16; esp_shazam_mp3::PCM_SAMPLES]>,
    filled: usize,
}

#[cfg(feature = "mp3-stream")]
impl Encoder {
    pub fn new() -> io::Result<Self> {
        // Allocate and initialize in place: an inline block would travel through
        // the startup stack and worker closure before reaching the encoding task.
        let mut pcm = Vec::new();
        pcm.try_reserve_exact(esp_shazam_mp3::PCM_SAMPLES)
            .map_err(|error| io::Error::other(format!("MP3 PCM allocation failed: {error}")))?;
        pcm.resize(esp_shazam_mp3::PCM_SAMPLES, 0_i16);
        let pcm = pcm
            .into_boxed_slice()
            .try_into()
            .map_err(|_| io::Error::other("MP3 PCM allocation has invalid length"))?;
        Ok(Self {
            codec: Some(mp3_encoder()?),
            pcm,
            filled: 0,
        })
    }
    pub fn frames_wanted(&self) -> usize {
        BLOCK_FRAMES - self.filled / 2
    }
    pub fn push(&mut self, pcm: &[i32]) {
        assert_eq!(pcm.len() % 2, 0, "capture must contain whole stereo frames");
        assert!(
            pcm.len() <= self.pcm.len() - self.filled,
            "MP3 block is full"
        );
        for (destination, &source) in self.pcm[self.filled..].iter_mut().zip(pcm) {
            *destination = (source >> 8).clamp(-32_768, 32_767) as i16;
        }
        self.filled += pcm.len();
    }
    pub fn encode_block(&mut self, _number: u32, bytes: &mut Vec<u8>) -> io::Result<()> {
        if self.filled != self.pcm.len() {
            return Err(io::Error::other("MP3 PCM block is incomplete"));
        }
        bytes.resize(MAX_FRAME_BYTES, 0);
        let frame: &mut [u8; esp_shazam_mp3::FRAME_BYTES] = bytes
            .as_mut_slice()
            .try_into()
            .expect("fixed MP3 frame size");
        self.codec
            .as_mut()
            .ok_or_else(|| io::Error::other("MP3 encoder is unavailable"))?
            .encode(&mut self.pcm, frame)
            .map_err(|error| io::Error::other(format!("MP3 encoding failed: {error:?}")))?;
        self.filled = 0;
        Ok(())
    }
    pub fn reset(&mut self) -> io::Result<()> {
        // Release old state first so reset never temporarily owns two encoders.
        drop(self.codec.take());
        self.codec = Some(mp3_encoder()?);
        self.filled = 0;
        Ok(())
    }
}

#[cfg(all(feature = "mp3-stream", target_os = "espidf"))]
fn mp3_encoder() -> io::Result<esp_shazam_mp3::Encoder> {
    use esp_idf_svc::sys;
    unsafe extern "C" fn allocate(
        _context: *mut std::ffi::c_void,
        size: usize,
    ) -> *mut std::ffi::c_void {
        // SAFETY: the C encoder requests ordinary writable, aligned storage. Null propagates.
        unsafe { sys::heap_caps_calloc(1, size, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) }
    }
    unsafe extern "C" fn deallocate(
        _context: *mut std::ffi::c_void,
        pointer: *mut std::ffi::c_void,
    ) {
        // SAFETY: the C owner returns each allocation from allocate exactly once.
        unsafe { sys::heap_caps_free(pointer) }
    }
    // SAFETY: callbacks have process lifetime and no context; capability allocation may run on either core.
    unsafe {
        esp_shazam_mp3::Encoder::with_allocator(esp_shazam_mp3::Allocator {
            context: std::ptr::null_mut(),
            allocate,
            deallocate,
        })
    }
    .map_err(|error| io::Error::other(format!("MP3 allocation failed: {error:?}")))
}

#[cfg(all(feature = "mp3-stream", not(target_os = "espidf")))]
fn mp3_encoder() -> io::Result<esp_shazam_mp3::Encoder> {
    esp_shazam_mp3::Encoder::new()
        .map_err(|error| io::Error::other(format!("MP3 allocation failed: {error:?}")))
}

pub fn stream_header() -> Vec<u8> {
    #[cfg(not(feature = "mp3-stream"))]
    {
        esp_shazam_core::flac::stream_header().to_vec()
    }
    #[cfg(feature = "mp3-stream")]
    {
        Vec::new()
    }
}

pub fn prepare_listener_frame(bytes: &mut [u8], len: usize, number: u32) -> io::Result<usize> {
    #[cfg(not(feature = "mp3-stream"))]
    {
        esp_shazam_core::flac::renumber_frame(bytes, len, number)
            .map_err(|error| io::Error::other(format!("FLAC numbering failed: {error:?}")))
    }
    #[cfg(feature = "mp3-stream")]
    {
        let _ = (bytes, number);
        if len != MAX_FRAME_BYTES {
            return Err(io::Error::other("MP3 frame length is invalid"));
        }
        Ok(len)
    }
}

#[cfg(all(test, feature = "mp3-stream"))]
mod tests {
    #[test]
    fn encoder_moves_do_not_carry_a_pcm_block_on_the_stack() {
        assert!(std::mem::size_of::<super::Encoder>() <= 8 * std::mem::size_of::<usize>());
    }
}
