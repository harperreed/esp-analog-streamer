// ABOUTME: Exercises channel diagnostics through real I2S decoding, metering and state JSON.
// ABOUTME: Covers rolling windows, reset history and capture-time age without sleeps or mocks.

use std::sync::{Mutex, TryLockError};
use std::time::{Duration, Instant};

use esp_shazam_core::audio::{samples_from_i2s_slots, StereoLevelMeter};
use esp_shazam_core::store::SongStore;
use serde_json::Value;

const FULL_SCALE: f64 = 8_388_608.0;

fn decode(samples: &[i32]) -> Vec<i32> {
    let bytes: Vec<u8> = samples
        .iter()
        .flat_map(|sample| ((sample << 8) | 0xA5).to_le_bytes())
        .collect();
    let mut decoded = Vec::new();
    samples_from_i2s_slots(&bytes, &mut decoded);
    assert_eq!(decoded, samples);
    decoded
}

fn square(frames: usize, amplitude: [i32; 2], dc: [i32; 2]) -> Vec<i32> {
    (0..frames)
        .flat_map(|i| {
            let sign = if i % 2 == 0 { 1 } else { -1 };
            [dc[0] + sign * amplitude[0], dc[1] + sign * amplitude[1]]
        })
        .collect()
}

fn publish(meter: &mut StereoLevelMeter, store: &mut SongStore, samples: &[i32], at: Instant) {
    let reading = meter.push(&decode(samples)).expect("completed interval");
    store.set_measurement(reading, at);
}

fn state(store: &SongStore) -> Value {
    serde_json::from_str(&store.state_json()).unwrap()
}

fn near(value: &Value, expected: f64, tolerance: f64) {
    let actual = value.as_f64().expect("numeric diagnostic");
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn history_is_explicitly_null_before_first_completed_interval() {
    let store = SongStore::new(20, false);
    assert_eq!(state(&store).get("last_measurement"), Some(&Value::Null));
    assert_eq!(store.level_sample(), None);
}

#[test]
fn i2s_channels_keep_separate_ac_rms_peak_and_signed_dc() {
    let mut meter = StereoLevelMeter::new(48_000);
    let mut store = SongStore::new(20, false);
    publish(
        &mut meter,
        &mut store,
        &square(12_000, [4_194_304, -2_097_152], [1_048_576, -1_048_576]),
        Instant::now(),
    );
    let json = state(&store);
    let history = &json["last_measurement"];
    assert_eq!(history["window_frames"], 12_000);
    let left = &history["channels"]["left"];
    let right = &history["channels"]["right"];
    near(&left["rms_dbfs"], -6.0206, 0.001);
    near(&right["rms_dbfs"], -12.0412, 0.001);
    near(&left["peak_normalized"], 0.625, 0.000_001);
    near(&right["peak_normalized"], 0.375, 0.000_001);
    near(&left["dc_offset_normalized"], 0.125, 0.000_001);
    near(&right["dc_offset_normalized"], -0.125, 0.000_001);
    assert_eq!(left["digital_rail_samples"], 0);
    assert_eq!(right["digital_rail_samples"], 0);
    near(&json["level_dbfs"], -8.1, 0.000_001);
    assert_eq!(store.level_sample().unwrap().0, store.level_dbfs());
    let fields = history.as_object().unwrap();
    assert_eq!(fields.len(), 3);
    assert!(fields.contains_key("age_ms"));
}

#[test]
fn either_silent_channel_and_opposite_polarity_preserve_combined_energy() {
    for (amplitude, rms, combined) in [
        ([4_194_304, 0], [-6.0206, -120.0], -9.0),
        ([0, 4_194_304], [-120.0, -6.0206], -9.0),
        ([4_194_304, -4_194_304], [-6.0206; 2], -6.0),
    ] {
        let mut store = SongStore::new(20, false);
        publish(
            &mut StereoLevelMeter::new(48_000),
            &mut store,
            &square(48_000, amplitude, [0, 0]),
            Instant::now(),
        );
        let json = state(&store);
        near(
            &json["last_measurement"]["channels"]["left"]["rms_dbfs"],
            rms[0],
            0.001,
        );
        near(
            &json["last_measurement"]["channels"]["right"]["rms_dbfs"],
            rms[1],
            0.001,
        );
        near(&json["level_dbfs"], combined, 0.000_001);
    }
}

#[test]
fn signed_rails_and_constant_dc_are_not_ac_sound() {
    let mut store = SongStore::new(20, false);
    publish(
        &mut StereoLevelMeter::new(48_000),
        &mut store,
        &square(48_000, [0, 0], [8_388_607, -8_388_608]),
        Instant::now(),
    );
    let json = state(&store);
    for (name, dc, peak) in [
        ("left", 8_388_607.0 / FULL_SCALE, 8_388_607.0 / FULL_SCALE),
        ("right", -1.0, 1.0),
    ] {
        let channel = &json["last_measurement"]["channels"][name];
        near(&channel["rms_dbfs"], -120.0, 0.0);
        near(&channel["peak_normalized"], peak, 0.000_000_05);
        near(&channel["dc_offset_normalized"], dc, 0.000_000_05);
        assert_eq!(channel["digital_rail_samples"], 48_000);
    }
    near(&json["level_dbfs"], -120.0, 0.0);
}

#[test]
fn alternating_signed_extremes_retain_fractional_mean_without_overflow() {
    let samples: Vec<_> = [8_388_607, -8_388_608, -8_388_608, 8_388_607]
        .into_iter()
        .cycle()
        .take(96_000)
        .collect();
    let mut meter = StereoLevelMeter::new(48_000);
    let mut store = SongStore::new(20, false);
    for _ in 0..3 {
        publish(&mut meter, &mut store, &samples, Instant::now());
        let json = state(&store);
        for name in ["left", "right"] {
            let channel = &json["last_measurement"]["channels"][name];
            near(&channel["rms_dbfs"], 0.0, 0.001);
            near(&channel["peak_normalized"], 1.0, 0.0);
            near(
                &channel["dc_offset_normalized"],
                -0.5 / FULL_SCALE,
                0.000_000_000_001,
            );
            assert_eq!(channel["digital_rail_samples"], 48_000);
        }
    }
}

#[test]
fn fractional_dc_below_one_sample_of_variance_stays_silent() {
    let mut store = SongStore::new(20, false);
    publish(
        &mut StereoLevelMeter::new(8),
        &mut store,
        &[0, 0, 1, -1],
        Instant::now(),
    );
    let json = state(&store);
    for (name, dc) in [("left", 0.5 / FULL_SCALE), ("right", -0.5 / FULL_SCALE)] {
        let channel = &json["last_measurement"]["channels"][name];
        near(&channel["rms_dbfs"], -120.0, 0.0);
        near(&channel["dc_offset_normalized"], dc, 0.000_000_000_001);
    }
}

#[test]
fn full_scale_sine_has_distinct_rms_and_peak() {
    let samples: Vec<_> = (0..48_000)
        .flat_map(|i| {
            let x = ((i as f64 * std::f64::consts::TAU / 48.0).sin() * 8_388_607.0).round() as i32;
            [x, x]
        })
        .collect();
    let mut store = SongStore::new(20, false);
    publish(
        &mut StereoLevelMeter::new(48_000),
        &mut store,
        &samples,
        Instant::now(),
    );
    let json = state(&store);
    near(
        &json["last_measurement"]["channels"]["left"]["rms_dbfs"],
        -3.0103,
        0.001,
    );
    near(
        &json["last_measurement"]["channels"]["left"]["peak_normalized"],
        8_388_607.0 / FULL_SCALE,
        0.000_000_05,
    );
}

#[test]
fn startup_reports_actual_completed_frames_and_excludes_partial_frame() {
    let mut meter = StereoLevelMeter::new(48_000);
    let mut store = SongStore::new(20, false);
    for interval in 1..=6 {
        publish(
            &mut meter,
            &mut store,
            &square(12_000, [4_194_304; 2], [0; 2]),
            Instant::now(),
        );
        assert_eq!(
            state(&store)["last_measurement"]["window_frames"],
            12_000 * interval.min(4)
        );
    }
    assert!(meter.push(&decode(&[8_388_607])).is_none());
    assert_eq!(state(&store)["last_measurement"]["window_frames"], 48_000);
}

#[test]
fn peak_and_rail_counts_expire_with_their_completed_bucket() {
    let mut meter = StereoLevelMeter::new(8);
    let mut store = SongStore::new(20, false);
    publish(
        &mut meter,
        &mut store,
        &[8_388_607, -8_388_608, -8_388_608, 0],
        Instant::now(),
    );
    for interval in 1..=4 {
        publish(&mut meter, &mut store, &[0; 4], Instant::now());
        let json = state(&store);
        let left = &json["last_measurement"]["channels"]["left"];
        let right = &json["last_measurement"]["channels"]["right"];
        assert_eq!(
            left["digital_rail_samples"],
            if interval < 4 { 2 } else { 0 }
        );
        assert_eq!(
            right["digital_rail_samples"],
            if interval < 4 { 1 } else { 0 }
        );
        near(
            &left["peak_normalized"],
            if interval < 4 { 1.0 } else { 0.0 },
            0.0,
        );
        near(
            &right["peak_normalized"],
            if interval < 4 { 1.0 } else { 0.0 },
            0.0,
        );
    }
}

#[test]
fn odd_chunks_and_multi_interval_push_return_the_same_last_completed_window() {
    let input = [
        square(12, [2_097_152, -4_194_304], [123, -456]),
        vec![8_388_607],
    ]
    .concat();
    let samples = decode(&input);
    let expected = StereoLevelMeter::new(8).push(&samples).unwrap();
    for size in [1, 3, 7, 10, 25] {
        let mut meter = StereoLevelMeter::new(8);
        let mut last = None;
        for chunk in samples.chunks(size) {
            if let Some(reading) = meter.push(chunk) {
                last = Some(reading);
            }
        }
        assert_eq!(last, Some(expected));
    }
    let mut store = SongStore::new(20, false);
    store.set_measurement(expected, Instant::now());
    assert_eq!(state(&store)["last_measurement"]["window_frames"], 8);
    near(
        &state(&store)["last_measurement"]["channels"]["left"]["peak_normalized"],
        (2_097_152.0 + 123.0) / FULL_SCALE,
        0.000_001,
    );
}

#[test]
fn delayed_publication_and_equal_readings_keep_the_original_receipt_time() {
    let reading = StereoLevelMeter::new(8)
        .push(&decode(&[1000, -1000, -1000, 1000]))
        .unwrap();
    let at = Instant::now() - Duration::from_secs(10);
    let mut store = SongStore::new(20, false);
    store.set_measurement(reading, at);
    let json = state(&store);
    assert!(json["last_measurement"]["age_ms"].as_u64().unwrap() >= 10_000);
    assert_eq!(store.level_sample().unwrap().1, at);
    let later = at + Duration::from_secs(5);
    store.set_measurement(reading, later);
    assert_eq!(store.level_sample().unwrap().1, later);
    assert!(
        state(&store)["last_measurement"]["age_ms"]
            .as_u64()
            .unwrap()
            >= 5_000
    );
}

#[test]
fn a_future_receipt_is_unavailable_rather_than_zero_age() {
    let reading = StereoLevelMeter::new(8)
        .push(&decode(&[1, 1, -1, -1]))
        .unwrap();
    let mut store = SongStore::new(20, false);
    store.set_measurement(reading, Instant::now() + Duration::from_secs(3600));
    assert_eq!(state(&store).get("last_measurement"), Some(&Value::Null));
}

#[test]
fn meter_reset_with_store_held_retains_history_until_new_interval() {
    let mut meter = StereoLevelMeter::new(48_000);
    let mut initial = SongStore::new(20, false);
    let old_at = Instant::now() - Duration::from_secs(20);
    publish(
        &mut meter,
        &mut initial,
        &square(12_000, [0; 2], [8_388_607, -8_388_608]),
        old_at,
    );
    let shared = Mutex::new(initial);
    let held = shared.lock().unwrap();
    meter.push(&decode(&square(12_000, [2_097_152; 2], [0; 2])));
    assert!(matches!(shared.try_lock(), Err(TryLockError::WouldBlock)));
    meter.push(&decode(&[8_388_607]));
    meter.reset();
    drop(held);
    let mut store = shared.lock().unwrap();
    assert_eq!(store.level_sample().unwrap().1, old_at);
    assert_eq!(
        state(&store)["last_measurement"]["channels"]["left"]["digital_rail_samples"],
        12_000
    );
    assert!(
        state(&store)["last_measurement"]["age_ms"]
            .as_u64()
            .unwrap()
            >= 20_000
    );
    assert!(meter.push(&decode(&vec![0; 23_999])).is_none());
    assert_eq!(store.level_sample().unwrap().1, old_at);
    publish(&mut meter, &mut store, &[0], Instant::now());
    let json = state(&store);
    assert_eq!(json["last_measurement"]["window_frames"], 12_000);
    assert_eq!(
        json["last_measurement"]["channels"]["left"]["digital_rail_samples"],
        0
    );
    assert_eq!(
        json["last_measurement"]["channels"]["right"]["digital_rail_samples"],
        0
    );
    near(&json["level_dbfs"], -120.0, 0.0);
}

#[test]
fn maximum_accepted_rate_keeps_exact_counts_and_extremes_after_window_rollover() {
    // 131,068 is the largest rate divisible by four below the square-sum bound.
    let rate = 131_068;
    let samples: Vec<_> = [8_388_607, -8_388_608, -8_388_608, 8_388_607]
        .into_iter()
        .cycle()
        .take(rate * 2)
        .collect();
    let mut meter = StereoLevelMeter::new(rate);
    let mut store = SongStore::new(20, false);
    for _ in 0..2 {
        publish(&mut meter, &mut store, &samples, Instant::now());
        let json = state(&store);
        assert_eq!(json["last_measurement"]["window_frames"], rate);
        for name in ["left", "right"] {
            let channel = &json["last_measurement"]["channels"][name];
            assert_eq!(channel["digital_rail_samples"], rate);
            near(&channel["rms_dbfs"], 0.0, 0.001);
            near(&channel["peak_normalized"], 1.0, 0.0);
            near(
                &channel["dc_offset_normalized"],
                -0.5 / FULL_SCALE,
                0.000_000_000_001,
            );
        }
    }
}
