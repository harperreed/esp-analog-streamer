// ABOUTME: Adapts the selected shared compressed encoder and listener frame format at runtime.
// ABOUTME: Keeps capture scaling, complete-frame publication and reset behavior in one adapter.

use std::io;

pub const SAMPLE_RATE_HZ: u32 = 48_000;
use esp_shazam_core::stream_buffer::OutputFormat;
const _: () = assert!(OutputFormat::Mp3.block_frames() == esp_shazam_mp3::SAMPLE_FRAMES);
const _: () = assert!(OutputFormat::Mp3.max_frame_bytes() == esp_shazam_mp3::FRAME_BYTES);

pub enum Encoder {
    Flac(esp_shazam_core::flac::Encoder),
    Mp3(Mp3Encoder),
}
impl Encoder {
    pub fn new(format: OutputFormat) -> io::Result<Self> {
        match format {
            OutputFormat::Flac => esp_shazam_core::flac::Encoder::try_new()
                .map(Self::Flac)
                .map_err(io::Error::other),
            OutputFormat::Mp3 => Mp3Encoder::new().map(Self::Mp3),
        }
    }
    #[cfg(feature = "encoder-probe")]
    pub fn mp3_storage_receipt(&self) -> Option<(usize, usize, esp_shazam_mp3::EncoderReceipt)> {
        match self {
            Self::Mp3(encoder) => Some((
                encoder.pcm.as_ptr() as usize,
                encoder.pcm.len() * std::mem::size_of::<i16>(),
                encoder.codec.as_ref()?.allocation_receipt()?,
            )),
            Self::Flac(_) => None,
        }
    }
    pub fn frames_wanted(&self) -> usize {
        match self {
            Self::Flac(e) => e.frames_wanted(),
            Self::Mp3(e) => e.frames_wanted(),
        }
    }
    pub fn push(&mut self, pcm: &[i32]) {
        match self {
            Self::Flac(e) => e.push(pcm),
            Self::Mp3(e) => e.push(pcm),
        }
    }
    pub fn encode_block(&mut self, number: u32, bytes: &mut Vec<u8>) -> io::Result<()> {
        match self {
            Self::Flac(e) => {
                e.encode_block(number, bytes);
                Ok(())
            }
            Self::Mp3(e) => e.encode_block(number, bytes),
        }
    }
    pub fn reset(&mut self) -> io::Result<()> {
        match self {
            Self::Flac(e) => {
                e.reset();
                Ok(())
            }
            Self::Mp3(e) => e.reset(),
        }
    }
}

pub struct Mp3Encoder {
    codec: Option<esp_shazam_mp3::Encoder>,
    pcm: Box<[i16; esp_shazam_mp3::PCM_SAMPLES]>,
    filled: usize,
}

impl Mp3Encoder {
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
        esp_shazam_mp3::SAMPLE_FRAMES - self.filled / 2
    }
    pub fn push(&mut self, pcm: &[i32]) {
        assert_eq!(pcm.len() % 2, 0, "capture must contain whole stereo frames");
        assert!(
            pcm.len() <= self.pcm.len() - self.filled,
            "MP3 block is full"
        );
        for (destination, &source) in self.pcm[self.filled..].iter_mut().zip(pcm) {
            *destination = esp_shazam_core::flac::quantize(source) as i16;
        }
        self.filled += pcm.len();
    }
    pub fn encode_block(&mut self, _number: u32, bytes: &mut Vec<u8>) -> io::Result<()> {
        if self.filled != self.pcm.len() {
            return Err(io::Error::other("MP3 PCM block is incomplete"));
        }
        bytes.resize(esp_shazam_mp3::FRAME_BYTES, 0);
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

#[cfg(target_os = "espidf")]
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

#[cfg(not(target_os = "espidf"))]
fn mp3_encoder() -> io::Result<esp_shazam_mp3::Encoder> {
    esp_shazam_mp3::Encoder::new()
        .map_err(|error| io::Error::other(format!("MP3 allocation failed: {error:?}")))
}

pub fn stream_header(format: OutputFormat) -> Vec<u8> {
    match format {
        OutputFormat::Flac => esp_shazam_core::flac::stream_header().to_vec(),
        OutputFormat::Mp3 => Vec::new(),
    }
}

pub fn prepare_listener_frame(
    format: OutputFormat,
    bytes: &mut [u8],
    len: usize,
    number: u32,
) -> io::Result<usize> {
    match format {
        OutputFormat::Flac => esp_shazam_core::flac::renumber_frame(bytes, len, number)
            .map_err(|error| io::Error::other(format!("FLAC numbering failed: {error:?}"))),
        OutputFormat::Mp3 if len == esp_shazam_mp3::FRAME_BYTES => Ok(len),
        OutputFormat::Mp3 => Err(io::Error::other("MP3 frame length is invalid")),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "encoder-probe")]
    #[test]
    fn real_mp3_storage_stays_stable_across_reserved_output_encoding() {
        use esp_shazam_core::stream_buffer::OutputFormat;
        let mut encoder = super::Encoder::new(OutputFormat::Mp3).unwrap();
        let receipt = encoder.mp3_storage_receipt().unwrap();
        assert_eq!(receipt.1, 4608);
        let mut output = Vec::with_capacity(960);
        let pointer = output.as_ptr();
        encoder.push(&vec![12345; 2304]);
        encoder.encode_block(0, &mut output).unwrap();
        assert_eq!(output.len(), 960);
        assert_eq!(output.as_ptr(), pointer);
        assert_eq!(encoder.mp3_storage_receipt(), Some(receipt));
        assert!(super::Encoder::new(OutputFormat::Flac)
            .unwrap()
            .mp3_storage_receipt()
            .is_none());
    }

    #[test]
    fn mp3_moves_do_not_carry_a_pcm_block_on_the_stack() {
        assert!(std::mem::size_of::<super::Mp3Encoder>() <= 8 * std::mem::size_of::<usize>());
    }
}
