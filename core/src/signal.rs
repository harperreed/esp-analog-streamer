// ABOUTME: Deterministic test audio at 16 kHz: a three-note chord plus a rising chirp.
// ABOUTME: Used by the golden fingerprint tests and by the on-device fingerprint timing check.

pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// `seconds` of mono 16-bit audio whose peaks land in all four fingerprint bands.
pub fn chord_and_chirp(seconds: u32) -> Vec<i16> {
    let n = (seconds * SAMPLE_RATE_HZ) as usize;
    let two_pi = 2.0 * std::f32::consts::PI;
    (0..n)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE_HZ as f32;
            let chord = 0.25 * (two_pi * 440.0 * t).sin()
                + 0.20 * (two_pi * 1_200.0 * t).sin()
                + 0.15 * (two_pi * 2_600.0 * t).sin();
            // Instantaneous frequency 300 Hz + 400 Hz/s: sweeps 300 Hz to 5100 Hz over 12 s.
            let chirp = 0.20 * (two_pi * (300.0 * t + 200.0 * t * t)).sin();
            ((chord + chirp) * 0.9 * 32_767.0) as i16
        })
        .collect()
}
