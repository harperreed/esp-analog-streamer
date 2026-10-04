// ABOUTME: Deterministic test audio: a chord plus a rising chirp at 16 kHz mono, and 48 kHz 24-bit stereo signals for the stream.
// ABOUTME: Used by the golden fingerprint tests, the FLAC encoder's tests and the on-device fingerprint timing check.

pub const SAMPLE_RATE_HZ: u32 = 16_000;
/// The LINE IN stream's sample rate.
pub const STREAM_RATE_HZ: u32 = 48_000;

/// The rails of a sign-extended 24-bit sample, the stream ring's format.
const S24_MAX: i32 = (1 << 23) - 1;
const S24_MIN: i32 = -(1 << 23);

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

/// `frames` frames of interleaved stereo at 48 kHz, in sign-extended 24-bit samples as the stream
/// ring holds them: the chord and chirp above on the left, and another chord over the same chirp on
/// the right, so a swapped or copied channel shows.
pub fn chord_and_chirp_stereo_48k(frames: usize) -> Vec<i32> {
    let two_pi = 2.0 * std::f64::consts::PI;
    let tone = |hz: f64, t: f64| (two_pi * hz * t).sin();
    (0..frames)
        .flat_map(|i| {
            let t = i as f64 / f64::from(STREAM_RATE_HZ);
            let chirp = 0.20 * (two_pi * (300.0 * t + 200.0 * t * t)).sin();
            let left = 0.25 * tone(440.0, t) + 0.20 * tone(1_200.0, t) + 0.15 * tone(2_600.0, t);
            let right = 0.25 * tone(554.37, t) + 0.20 * tone(1_500.0, t) + 0.15 * tone(3_300.0, t);
            [left + chirp, right + chirp].map(|x| (x * 0.9 * f64::from(S24_MAX)) as i32)
        })
        .collect()
}

/// `frames` frames of a 500 Hz square wave on the 24-bit rails at 48 kHz, the right channel in
/// opposite phase to the left.
pub fn full_scale_square_stereo_48k(frames: usize) -> Vec<i32> {
    let half_period = STREAM_RATE_HZ as usize / 1_000;
    (0..frames)
        .flat_map(|i| {
            if i % (2 * half_period) < half_period {
                [S24_MAX, S24_MIN]
            } else {
                [S24_MIN, S24_MAX]
            }
        })
        .collect()
}

/// `frames` frames of white noise over the whole 24-bit range on both channels, from a fixed seed
/// (xorshift32).
pub fn full_scale_noise_stereo(frames: usize) -> Vec<i32> {
    let mut x: u32 = 0x9E37_79B9;
    (0..2 * frames)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            // The top 24 bits, sign-extended.
            (x as i32) >> 8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stereo_signals_stay_in_24_bits_with_channels_that_differ() {
        let frames = 4_800;
        for (name, signal) in [
            ("chord and chirp", chord_and_chirp_stereo_48k(frames)),
            ("square", full_scale_square_stereo_48k(frames)),
            ("noise", full_scale_noise_stereo(frames)),
        ] {
            assert_eq!(signal.len(), 2 * frames, "{name}");
            assert!(
                signal.iter().all(|s| (S24_MIN..=S24_MAX).contains(s)),
                "{name}: a sample beyond 24 bits"
            );
            let equal = signal.chunks_exact(2).filter(|f| f[0] == f[1]).count();
            assert!(equal * 100 < frames, "{name}: L == R in {equal} frames");
        }
    }

    #[test]
    fn the_square_wave_sits_on_the_rails() {
        let square = full_scale_square_stereo_48k(4_800);
        assert!(square.iter().all(|&s| s == S24_MAX || s == S24_MIN));
        // 48 frames high, then 48 low: 500 Hz at 48 kHz.
        assert_eq!(square[2 * 47], S24_MAX);
        assert_eq!(square[2 * 48], S24_MIN);
    }
}
