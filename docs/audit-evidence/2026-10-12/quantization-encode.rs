// ABOUTME: Throwaway host driver of the unchanged production FLAC encoder.
// ABOUTME: Reads sign-extended capture samples and writes the complete stream.
#[path = "../../../core/src/flac.rs"]
mod flac;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    let samples: Vec<i32> = bytes.chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).collect();
    assert_eq!(samples.len() % (flac::BLOCK_FRAMES * 2), 0);
    let mut encoder = flac::Encoder::new();
    let mut out = flac::stream_header().to_vec();
    let mut frame = Vec::with_capacity(flac::MAX_FRAME_BYTES);
    let start = std::time::Instant::now();
    for (n, block) in samples.chunks_exact(flac::BLOCK_FRAMES * 2).enumerate() {
        encoder.push(block);
        encoder.encode_block(n as u32, &mut frame);
        out.extend_from_slice(&frame);
    }
    println!("{}", start.elapsed().as_nanos());
    std::fs::write(&args[2], out).unwrap();
}
