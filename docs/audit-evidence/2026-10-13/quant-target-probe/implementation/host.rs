// ABOUTME: Encodes shared throwaway fixtures through the unchanged production FLAC encoder.
// ABOUTME: Emits target receipts and real PCM/FLAC streams for independent FFmpeg verification.

extern crate self as esp_shazam_core;
#[path = "quant-cost/contract.rs"]
#[allow(dead_code)]
mod contract;
#[path = "../../../core/src/flac.rs"]
#[allow(dead_code)]
mod flac;

use contract::{Expected, Policy, Receipt, BLOCKS, CHUNK_SAMPLES, KINDS, SEEDS};
use flac::{Encoder, BLOCK_FRAMES, CHANNELS, MAX_FRAME_BYTES};
use std::{fs, io::Write, path::PathBuf};

fn main() -> std::io::Result<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: host OUTPUT_DIRECTORY"),
    );
    fs::create_dir_all(&out)?;
    let mut receipts = fs::File::create(out.join("receipts.jsonl"))?;
    let mut constants = fs::File::create(out.join("expected.rs"))?;
    let mut encoder = Encoder::new();
    let mut control_encoder = Encoder::new();
    let mut input = vec![0_i32; BLOCK_FRAMES * CHANNELS];
    let mut chunk = vec![0_i32; CHUNK_SAMPLES];
    let mut frame = Vec::with_capacity(MAX_FRAME_BYTES);
    let mut control = Vec::with_capacity(MAX_FRAME_BYTES);
    let pointer = frame.as_ptr();
    let capacity = frame.capacity();
    let mut all = Vec::new();
    for (kind_index, kind) in KINDS.into_iter().enumerate() {
        contract::fill_input(kind, &mut input);
        assert_eq!(
            contract::source_hash(&input),
            contract::SOURCE_HASHES[kind_index]
        );
        let mut source = fs::File::create(out.join(format!("{kind}.i32le")))?;
        for sample in &input {
            source.write_all(&sample.to_le_bytes())?;
        }
        let mut policies = Vec::new();
        for policy in [Policy::Trunc, Policy::Round, Policy::Tpdf] {
            let mut phases = Vec::new();
            for (phase_index, (phase, blocks)) in [("warmup", 1), ("measured", BLOCKS)]
                .into_iter()
                .enumerate()
            {
                let mut states = SEEDS;
                let mut receipt = Receipt::new();
                let stem = format!("{kind}-{}-{phase}", policy.name());
                let mut stream = fs::File::create(out.join(format!("{stem}.flac")))?;
                let mut pcm = fs::File::create(out.join(format!("{stem}.s16le")))?;
                stream.write_all(&flac::stream_header())?;
                for block in 0..blocks {
                    encoder.reset();
                    for samples in input.chunks(CHUNK_SAMPLES) {
                        contract::transform(
                            samples,
                            &mut chunk[..samples.len()],
                            policy,
                            &mut states,
                        );
                        encoder.push(&chunk[..samples.len()]);
                        receipt.add_pcm(&chunk[..samples.len()]);
                        for &sample in &chunk[..samples.len()] {
                            pcm.write_all(&((sample / 256) as i16).to_le_bytes())?;
                        }
                    }
                    encoder.encode_block(0, &mut frame);
                    assert_eq!(frame.as_ptr(), pointer);
                    assert_eq!(frame.capacity(), capacity);
                    assert!(frame.len() <= MAX_FRAME_BYTES);
                    receipt.add(&frame);
                    if matches!(policy, Policy::Trunc) {
                        control_encoder.reset();
                        for samples in input.chunks(CHUNK_SAMPLES) {
                            control_encoder.push(samples);
                        }
                        control_encoder.encode_block(0, &mut control);
                        assert_eq!(
                            frame, control,
                            "raw truncation control {kind}/{phase}/{block}"
                        );
                    }
                    // Target hashes frame number zero. Number the retained decoder stream after hashing.
                    let len = frame.len();
                    let stream_len = flac::renumber_frame(&mut frame, len, block as u32).unwrap();
                    stream.write_all(&frame[..stream_len])?;
                }
                writeln!(receipts, "{{\"kind\":\"{kind}\",\"policy\":\"{}\",\"phase\":\"{phase}\",\"blocks\":{blocks},\"source_hash\":{},\"bytes\":{},\"frame_hash\":{},\"pcm_hash\":{},\"largest\":{},\"rng_left\":{},\"rng_right\":{},\"samples\":{}}}", policy.name(), contract::source_hash(&input), receipt.bytes, receipt.hash, receipt.pcm_hash, receipt.largest, states[0], states[1], blocks * BLOCK_FRAMES * CHANNELS)?;
                phases.push(Expected { receipt, states });
                let expected = contract::EXPECTED[kind_index][policy.index()][phase_index];
                assert_eq!(
                    receipt, expected.receipt,
                    "embedded target receipt mismatch"
                );
                assert_eq!(states, expected.states, "embedded target RNG mismatch");
            }
            policies.push(phases);
        }
        all.push(policies);
    }
    writeln!(
        constants,
        "// Host-generated frame-number-zero receipts; [fixture][policy][warmup/measured]."
    )?;
    writeln!(constants, "pub const EXPECTED: [[[Expected; 2]; 3]; 5] = [")?;
    for policies in all {
        writeln!(constants, "[")?;
        for phases in policies {
            writeln!(constants, "{:?},", phases)?;
        }
        writeln!(constants, "],")?;
    }
    writeln!(constants, "];")?;
    println!("30 real production encoder streams; raw truncation controls byte-identical");
    Ok(())
}
