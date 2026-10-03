// ABOUTME: Golden tests: our copy of SongRec's fingerprint code must match songrec-lib 0.5.4 byte for byte.
// ABOUTME: Synthetic audio always runs; the real song clip runs with `cargo test -- --ignored`.

use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::signal::chord_and_chirp;

fn ours(pcm: &[i16]) -> Vec<u8> {
    SignatureGenerator::make_signature_from_buffer(pcm)
        .encode_to_binary()
        .expect("our encoder failed")
}

// The songrec-lib package names its library target `songrec`, so that is the crate path here.
fn upstream(pcm: &[i16]) -> Vec<u8> {
    songrec::SignatureGenerator::make_signature_from_buffer(pcm)
        .encode_to_binary()
        .expect("upstream encoder failed")
}

#[test]
fn synthetic_audio_matches_upstream_byte_for_byte() {
    let pcm = chord_and_chirp(12);
    let ours = ours(&pcm);
    assert!(
        ours.len() > 200,
        "suspiciously small signature: {} bytes",
        ours.len()
    );
    assert_eq!(ours, upstream(&pcm));
}

#[test]
fn our_encoding_decodes_with_upstream_decoder() {
    let pcm = chord_and_chirp(12);
    let signature = SignatureGenerator::make_signature_from_buffer(&pcm);
    let uri = signature.encode_to_uri().expect("encode_to_uri failed");
    assert!(uri.starts_with("data:audio/vnd.shazam.sig;base64,"));
    let decoded = songrec::DecodedSignature::decode_from_uri(&uri).expect("upstream decode failed");
    assert_eq!(decoded.sample_rate_hz, 16_000);
    assert_eq!(decoded.number_samples, pcm.len() as u32);
    // Identical re-encoding means every peak survived the round trip.
    assert_eq!(
        decoded.encode_to_binary().unwrap(),
        signature.encode_to_binary().unwrap()
    );
}

#[test]
#[ignore = "needs test/fixtures/songs/clip.wav; scripts/e2e-host creates it and runs this"]
fn song_clip_matches_upstream_byte_for_byte() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test/fixtures/songs/clip.wav"
    );
    let mut reader = hound::WavReader::open(path).expect("open clip.wav");
    let spec = reader.spec();
    assert_eq!(
        (spec.channels, spec.sample_rate, spec.bits_per_sample),
        (1, 16_000, 16)
    );
    let pcm: Vec<i16> = reader
        .samples::<i16>()
        .map(|s| s.expect("bad sample"))
        .collect();
    assert_eq!(ours(&pcm), upstream(&pcm));
}
