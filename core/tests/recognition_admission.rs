// ABOUTME: Exercises capture-gap admission with real recognition audio and listen state.
// ABOUTME: Keeps the initial eight-second request and rate-limit recovery under regression tests.

#[path = "../../firmware/src/recognition_admission.rs"]
mod recognition_admission;

use esp_shazam_core::audio::RecognitionBuffer;
use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::policy::{Decision, Outcome, Policy, PolicyConfig};
use esp_shazam_core::store::SongStore;

fn snapshot(buffer: &RecognitionBuffer) -> Vec<i16> {
    let mut samples = Vec::new();
    buffer
        .shared_ring()
        .lock()
        .unwrap()
        .copy_latest(192_000, &mut samples);
    samples
}

#[test]
fn reset_immediately_before_due_ask_defers_until_eight_seconds_of_fresh_audio() {
    let mut buffer = RecognitionBuffer::new(true, 192_000, 16_000, 4_000).unwrap();
    let mut store = SongStore::new(20, true);
    store.set_level(-20.0, std::time::Instant::now());
    let mut policy = Policy::new(PolicyConfig::default());
    assert_eq!(
        policy.tick(0, store.level_dbfs(), true, store.has_now()),
        Decision::Idle
    );
    buffer.push(&vec![1_000; 128_000]);
    buffer.reset();
    assert_eq!(
        policy.tick(8_000, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
    let samples = snapshot(&buffer);
    assert!(!recognition_admission::admit(
        &mut policy,
        PolicyConfig::default(),
        samples.len()
    ));
    buffer.push(&vec![2_000; 127_999]);
    assert_eq!(
        policy.tick(15_999, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
    assert!(!recognition_admission::admit(
        &mut policy,
        PolicyConfig::default(),
        snapshot(&buffer).len()
    ));
    buffer.push(&[2_000]);
    assert_eq!(
        policy.tick(16_000, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
    let samples = snapshot(&buffer);
    assert!(recognition_admission::admit(
        &mut policy,
        PolicyConfig::default(),
        samples.len()
    ));
    let signature = SignatureGenerator::make_signature_from_buffer(&samples);
    assert_eq!(signature.number_samples, 128_000);
    assert!(signature.encode_to_uri().unwrap().starts_with("data:"));
    assert_eq!(
        policy.tick(16_250, store.level_dbfs(), true, store.has_now()),
        Decision::Idle
    );
    policy.on_outcome(16_250, &Outcome::Miss, store.now_key());
    assert_eq!(
        policy.tick(24_249, store.level_dbfs(), true, store.has_now()),
        Decision::Idle
    );
    assert_eq!(
        policy.tick(24_250, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
}

#[test]
fn configured_two_second_initial_ask_keeps_its_audio_duration() {
    let cfg = PolicyConfig {
        search_interval_ms: 2_000,
        ..PolicyConfig::default()
    };
    let mut policy = Policy::new(cfg);
    let mut buffer = RecognitionBuffer::new(true, 192_000, 16_000, 4_000).unwrap();
    let mut store = SongStore::new(20, true);
    store.set_level(-20.0, std::time::Instant::now());
    assert_eq!(
        policy.tick(0, store.level_dbfs(), true, store.has_now()),
        Decision::Idle
    );
    let mut noise = 1u32;
    let audio: Vec<i16> = (0..32_000)
        .map(|_| {
            noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (noise >> 16) as i16
        })
        .collect();
    buffer.push(&audio);
    assert_eq!(
        policy.tick(2_000, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
    assert!(recognition_admission::admit(
        &mut policy,
        cfg,
        snapshot(&buffer).len()
    ));
    let signature = SignatureGenerator::make_signature_from_buffer(&snapshot(&buffer));
    assert_eq!(signature.number_samples, 32_000);
    assert!(signature
        .frequency_band_to_sound_peaks
        .values()
        .any(|peaks| !peaks.is_empty()));
}

#[test]
fn long_initial_interval_and_rate_limit_keep_their_deadlines_after_deferral() {
    let cfg = PolicyConfig {
        search_interval_ms: 20_000,
        ..PolicyConfig::default()
    };
    let mut policy = Policy::new(cfg);
    let mut buffer = RecognitionBuffer::new(true, 192_000, 16_000, 4_000).unwrap();
    let mut store = SongStore::new(20, true);
    store.set_level(-20.0, std::time::Instant::now());
    assert_eq!(
        policy.tick(0, store.level_dbfs(), true, store.has_now()),
        Decision::Idle
    );
    buffer.push(&vec![1_000; 192_000]);
    assert_eq!(
        policy.tick(19_999, store.level_dbfs(), true, store.has_now()),
        Decision::Idle
    );
    assert_eq!(
        policy.tick(20_000, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
    assert!(recognition_admission::admit(
        &mut policy,
        cfg,
        snapshot(&buffer).len()
    ));
    policy.on_outcome(20_000, &Outcome::RateLimited, store.now_key());
    buffer.reset();
    assert_eq!(
        policy.tick(319_999, store.level_dbfs(), true, store.has_now()),
        Decision::Idle
    );
    assert_eq!(policy.rate_limited_until(319_999), Some(320_000));
    assert_eq!(
        policy.tick(320_000, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
    assert!(!recognition_admission::admit(
        &mut policy,
        cfg,
        snapshot(&buffer).len()
    ));
    buffer.push(&vec![1_000; 128_000]);
    assert_eq!(
        policy.tick(328_000, store.level_dbfs(), true, store.has_now()),
        Decision::Ask
    );
    assert!(recognition_admission::admit(
        &mut policy,
        cfg,
        snapshot(&buffer).len()
    ));
    assert_eq!(
        SignatureGenerator::make_signature_from_buffer(&snapshot(&buffer)).number_samples,
        128_000
    );
    assert_eq!(policy.rate_limited_until(328_000), None);
}

#[test]
fn zero_interval_uses_the_default_audio_duration_without_changing_scheduling() {
    let cfg = PolicyConfig {
        search_interval_ms: 0,
        ..PolicyConfig::default()
    };
    let mut policy = Policy::new(cfg);
    assert_eq!(policy.tick(0, -20.0, true, false), Decision::Ask);
    assert!(!recognition_admission::admit(&mut policy, cfg, 0));
    assert_eq!(policy.tick(250, -20.0, true, false), Decision::Ask);
    assert!(!recognition_admission::admit(&mut policy, cfg, 127));
    assert_eq!(policy.tick(500, -20.0, true, false), Decision::Ask);
    assert!(!recognition_admission::admit(&mut policy, cfg, 128));
    assert_eq!(policy.tick(750, -20.0, true, false), Decision::Ask);
    assert!(recognition_admission::admit(&mut policy, cfg, 128_000));
}
