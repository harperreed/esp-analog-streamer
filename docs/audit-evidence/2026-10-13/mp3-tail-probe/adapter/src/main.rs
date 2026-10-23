// ABOUTME: Checks real firmware codec reset clears encoded filter history and partial PCM.
// ABOUTME: Exports a no-reset control and a clean new epoch without hardware or playback.
#[allow(dead_code)]
#[path = "../../../../../firmware/src/stream_codec.rs"]
mod stream_codec;
use esp_shazam_core::stream_buffer::OutputFormat;
use stream_codec::Encoder;
const BLOCK: usize = 1152;

fn emit(encoder: &mut Encoder, pcm: &[i32]) -> Vec<u8> {
    encoder.push(pcm);
    let mut output = Vec::new();
    encoder.encode_block(0, &mut output).unwrap();
    assert_eq!(output.len(), 960);
    output
}

fn silence(encoder: &mut Encoder) -> Vec<u8> {
    (0..4)
        .flat_map(|_| emit(encoder, &vec![0; BLOCK * 2]))
        .collect()
}

fn main() {
    let output = std::env::args()
        .nth(1)
        .expect("usage: mp3-tail-adapter-probe OUTPUT_DIRECTORY");
    let output = std::path::Path::new(&output);
    let mut reset = Encoder::new(OutputFormat::Mp3).unwrap();
    let mut continuous = Encoder::new(OutputFormat::Mp3).unwrap();
    let mut fresh = Encoder::new(OutputFormat::Mp3).unwrap();
    let mut impulse = vec![0; BLOCK * 2];
    impulse[(BLOCK - 1) * 2] = 16000 << 8;
    impulse[(BLOCK - 1) * 2 + 1] = -12000 << 8;
    let old = emit(&mut reset, &impulse);
    assert_eq!(old, emit(&mut continuous, &impulse));
    // A partial buffer cannot be encoded, and reset must discard it as well as filter history.
    reset.push(&vec![8388352; 137 * 2]);
    assert_eq!(reset.frames_wanted(), BLOCK - 137);
    assert!(reset.encode_block(1, &mut Vec::new()).is_err());
    reset.reset().unwrap();
    assert_eq!(reset.frames_wanted(), BLOCK);
    let after_reset = silence(&mut reset);
    let after_fresh = silence(&mut fresh);
    let after_continuous = silence(&mut continuous);
    assert_eq!(after_reset, after_fresh);
    assert_ne!(after_continuous, after_fresh);
    for (name, bytes) in [
        ("epoch-before-reset", old),
        ("epoch-after-reset", after_reset),
        ("epoch-fresh", after_fresh),
        ("epoch-continuous-control", after_continuous),
    ] {
        std::fs::write(output.join(format!("{name}.mp3")), bytes).unwrap();
    }
    println!("partial_discard_sample_frames=137 incomplete_rejected=true reset_silence_equals_fresh=true continuous_tail_differs=true emitted_post_reset_frames=4");
}
