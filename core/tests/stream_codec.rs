// ABOUTME: Exercises the firmware's actual active codec adapter with real encoders.
// ABOUTME: Verifies block accumulation, 24-bit scaling and reset state.
#[path = "../../firmware/src/stream_codec.rs"]
mod stream_codec;
use esp_shazam_core::stream_buffer::OutputFormat;
use stream_codec::Encoder;
const BLOCK_FRAMES: usize = 1152;
const MAX_FRAME_BYTES: usize = 960;

#[test]
fn active_encoder_accumulates_chunks_and_reset_discards_partial_pcm() {
    assert_eq!(stream_codec::SAMPLE_RATE_HZ, 48_000);
    for format in [OutputFormat::Flac, OutputFormat::Mp3] {
        let block_frames = format.block_frames();
        let max_frame_bytes = format.max_frame_bytes();
        let mut encoder = Encoder::new(format).unwrap();
        assert_eq!(encoder.frames_wanted(), block_frames);
        encoder.push(&[256, -256]);
        assert_eq!(encoder.frames_wanted(), block_frames - 1);
        encoder.reset().unwrap();
        assert_eq!(encoder.frames_wanted(), block_frames);
        encoder.push(&vec![0; 2 * block_frames]);
        let mut bytes = Vec::with_capacity(max_frame_bytes);
        encoder.encode_block(0, &mut bytes).unwrap();
        assert!(!bytes.is_empty());
        assert!(bytes.len() <= max_frame_bytes);
        assert_eq!(encoder.frames_wanted(), block_frames);
        let first = bytes.clone();
        encoder.reset().unwrap();
        encoder.push(&vec![0; 2 * block_frames]);
        encoder.encode_block(0, &mut bytes).unwrap();
        assert_eq!(bytes, first);
        assert_eq!(
            stream_codec::prepare_listener_frame(format, &mut bytes, first.len(), 0).unwrap(),
            first.len()
        );
        let header = stream_codec::stream_header(format);
        assert_eq!(header.is_empty(), format == OutputFormat::Mp3);
    }
}

#[test]
fn mp3_adapter_uses_flac_24_to_16_scaling_without_gain_changes() {
    let mut adapter = Encoder::new(OutputFormat::Mp3).unwrap();
    let source: Vec<i32> = [
        i32::MIN,
        -8_388_608,
        -257,
        -256,
        -1,
        0,
        255,
        256,
        8_388_607,
        i32::MAX,
    ]
    .into_iter()
    .cycle()
    .take(2 * BLOCK_FRAMES)
    .collect();
    for chunk in source.chunks(480) {
        adapter.push(chunk);
    }
    let mut actual = Vec::new();
    adapter.encode_block(0, &mut actual).unwrap();
    let mut reference = esp_shazam_mp3::Encoder::new().unwrap();
    let mut pcm = [0; esp_shazam_mp3::PCM_SAMPLES];
    for (sample, raw) in pcm.iter_mut().zip(source) {
        *sample = (raw >> 8).clamp(-32_768, 32_767) as i16;
    }
    let mut expected = [0; esp_shazam_mp3::FRAME_BYTES];
    reference.encode(&mut pcm, &mut expected).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn real_mp3_history_retains_whole_ordered_frames_across_wrap_and_reset() {
    use esp_shazam_core::{flac_ring::FlacRing, stream_buffer::BufferPreset};
    let mut encoder = Encoder::new(OutputFormat::Mp3).unwrap();
    let mut ring = FlacRing::try_new(500, MAX_FRAME_BYTES).unwrap();
    let mut fast = ring.cursor_now();
    let mut stale = ring.cursor_now();
    let mut output = [0; esp_shazam_mp3::FRAME_BYTES];
    let mut bytes = Vec::with_capacity(MAX_FRAME_BYTES);
    let mut expected = Vec::new();
    for number in 0..550 {
        let pcm: Vec<i32> = (0..BLOCK_FRAMES)
            .flat_map(|sample| {
                let value = (((sample + number as usize) % 101) as i32 - 50) << 13;
                [value, -value]
            })
            .collect();
        for chunk in pcm.chunks(480) {
            encoder.push(chunk);
        }
        encoder.encode_block(number, &mut bytes).unwrap();
        ring.push(&bytes).unwrap();
        assert_eq!(ring.read(&mut fast, &mut output).unwrap(), MAX_FRAME_BYTES);
        assert_eq!(output.as_slice(), bytes);
        expected.push(bytes.clone());
    }
    assert!(ring.read(&mut stale, &mut output).is_err());
    for preset in [BufferPreset::Low, BufferPreset::Medium, BufferPreset::High] {
        let count = preset.frames(BLOCK_FRAMES);
        let mut cursor = ring.cursor_retained(count).unwrap().unwrap();
        for frame in &expected[550 - count..] {
            assert_eq!(
                ring.read(&mut cursor, &mut output).unwrap(),
                MAX_FRAME_BYTES
            );
            assert_eq!(output.as_slice(), frame);
        }
        assert_eq!(ring.read(&mut cursor, &mut output).unwrap(), 0);
    }
    let mut retained = ring.cursor_retained(500).unwrap().unwrap();
    for frame in &expected[50..] {
        assert_eq!(
            ring.read(&mut retained, &mut output).unwrap(),
            MAX_FRAME_BYTES
        );
        assert_eq!(output.as_slice(), frame);
    }
    let mut before_reset = ring.cursor_now();
    ring.reset();
    encoder.reset().unwrap();
    assert!(ring.read(&mut before_reset, &mut output).is_err());
    assert!(ring.cursor_retained(42).unwrap().is_none());
    encoder.push(&vec![0; 2 * BLOCK_FRAMES]);
    encoder.encode_block(0, &mut bytes).unwrap();
    let mut restarted = ring.cursor_now();
    ring.push(&bytes).unwrap();
    assert_eq!(
        ring.read(&mut restarted, &mut output).unwrap(),
        MAX_FRAME_BYTES
    );
    assert_eq!(output.as_slice(), bytes);
}
