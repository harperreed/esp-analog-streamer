// ABOUTME: Decodes real MP3 frames with FFmpeg and checks frame-boundary joins.
// ABOUTME: Checks encoder recreation starts a clean epoch with deterministic output.
use esp_shazam_mp3::{Encoder, FRAME_BYTES, PCM_SAMPLES, SAMPLE_FRAMES};
use std::io::Write;
use std::process::{Command, Stdio};

fn fixture(block: usize, amplitudes: [f64; 2]) -> [i16; PCM_SAMPLES] {
    let mut pcm = [0; PCM_SAMPLES];
    for frame in 0..SAMPLE_FRAMES {
        let t = (block * SAMPLE_FRAMES + frame) as f64 / 48_000.0;
        pcm[frame * 2] = ((t * 997.0 * std::f64::consts::TAU).sin() * amplitudes[0]) as i16;
        pcm[frame * 2 + 1] = ((t * 1703.0 * std::f64::consts::TAU).sin() * amplitudes[1]) as i16;
    }
    pcm
}
fn encode(blocks: usize) -> Vec<u8> {
    encode_with_amplitudes(blocks, [12_000.0, 9_000.0])
}
fn encode_with_amplitudes(blocks: usize, amplitudes: [f64; 2]) -> Vec<u8> {
    let mut encoder = Encoder::new().unwrap();
    let mut bytes = Vec::new();
    for block in 0..blocks {
        let mut out = [0; FRAME_BYTES];
        encoder
            .encode(&mut fixture(block, amplitudes), &mut out)
            .unwrap();
        bytes.extend_from_slice(&out);
    }
    bytes
}
fn decode(bytes: Vec<u8>, format: &str, codec: &str) -> Vec<u8> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "mp3",
            "-i",
            "pipe:0",
            "-f",
            format,
            "-acodec",
            codec,
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("FFmpeg is required for real MP3 decoder tests");
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&bytes).unwrap());
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
#[test]
fn real_decoder_counts_samples_and_recovers_after_boundary_join() {
    let encoded = encode(48);
    let full = decode(encoded.clone(), "s16le", "pcm_s16le");
    assert_eq!(full.len(), 48 * SAMPLE_FRAMES * 4);
    let join_frame = 6;
    let joined = decode(
        encoded[join_frame * FRAME_BYTES..].to_vec(),
        "s16le",
        "pcm_s16le",
    );
    assert_eq!(joined.len(), (48 - join_frame) * SAMPLE_FRAMES * 4);
    let warmup = 2 * SAMPLE_FRAMES * 4;
    assert_eq!(
        &joined[warmup..],
        &full[join_frame * SAMPLE_FRAMES * 4 + warmup..]
    );
    assert!(full.iter().any(|byte| *byte != 0));
}

#[test]
fn real_decoder_preserves_per_channel_tone_gain() {
    const BLOCKS: usize = 96;
    // Raw MP3 has no gapless metadata; the combined encoder/decoder delay is 1057 frames.
    const DELAY: usize = 1057;
    for (case, amplitudes) in [
        ("moderate level", [12_000.0, 9_000.0]),
        ("near full scale", [0.98 * 32768.0; 2]),
    ] {
        let decoded = decode(
            encode_with_amplitudes(BLOCKS, amplitudes),
            "f32le",
            "pcm_f32le",
        );
        assert_eq!(decoded.len(), BLOCKS * SAMPLE_FRAMES * 2 * 4, "{case}");
        let decoded: Vec<f64> = decoded
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()) as f64)
            .collect();
        let source: Vec<i16> = (0..BLOCKS)
            .flat_map(|block| fixture(block, amplitudes))
            .collect();
        // Exclude filter startup and the undrained encoder tail from the measurement.
        let window = 4 * SAMPLE_FRAMES..(BLOCKS - 2) * SAMPLE_FRAMES;
        for channel in 0..2 {
            let mut input_energy = 0.0;
            let mut output_energy = 0.0;
            let mut product = 0.0;
            for frame in window.clone() {
                let input = f64::from(source[(frame - DELAY) * 2 + channel]) / 32768.0;
                let output = decoded[frame * 2 + channel];
                input_energy += input * input;
                output_energy += output * output;
                product += input * output;
            }
            let gain = (output_energy / input_energy).sqrt();
            let correlation = product / (input_energy * output_energy).sqrt();
            assert!(
                correlation > 0.999,
                "{case}, channel {channel}: decoded tone correlation {correlation:.6}"
            );
            assert!(
                (gain - 1.0).abs() <= 0.005,
                "{case}, channel {channel}: decoded/input RMS gain {gain:.6}, expected 1.0 ± 0.005"
            );
        }
    }
}

#[test]
fn recreation_restarts_encoder_epoch() {
    assert_eq!(encode(8), encode(8));
}
