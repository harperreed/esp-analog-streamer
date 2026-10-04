// ABOUTME: Turns the 48 kHz stereo capture into the 16 kHz mono feed the fingerprint needs.
// ABOUTME: Averages the channels, low-pass filters with a 63-tap FIR, and keeps one output in three.

//! Input: interleaved left, right frames of `i32` holding sign-extended 24-bit samples
//! (-2^23 to 2^23 - 1). I2S delivers 24 bits left-justified in a 32-bit slot, so the caller shifts
//! each word right by 8 (arithmetic) first; the `StreamRing` takes the same values.
//! Output: 16-bit mono at the same full scale, so 24-bit full scale is 16-bit full scale.
//! Anything beyond 24 bits clips at the 16-bit rails and never panics.
//!
//! Filter design. Keeping one sample in three folds everything above 8 kHz down into 0-8 kHz, so
//! the filter must stop it first. Shazam's bands end at 5.5 kHz, so it must also pass up to there.
//! - A linear-phase FIR of 63 taps (symmetric, so every frequency is delayed the same 31 frames),
//!   with unit gain at DC.
//! - The window method: a sinc low-pass with its -6 dB point at 6.5 kHz (at 48 kHz), times a
//!   Kaiser window with beta 4. That leaves a transition of about 2 kHz, from 5.5 kHz to 7.5 kHz.
//!   Both goals (flat within 0.1 dB to 5.5 kHz, 40 dB down from 7.5 kHz) need about 57 taps: a
//!   search over cutoffs and betas found 55 taps just short (39.8 dB) and 57 just enough (42.6 dB).
//!   63 gives more than 4 dB of margin for 10% more multiply-adds.
//! - Measured on the table: within 0.06 dB up to 5.5 kHz, -3 dB at 6.2 kHz, -6 dB at 6.5 kHz,
//!   -44 dB at 7.5 kHz, below -46 dB from 7.6 kHz and below -49 dB from 8 kHz up to 24 kHz.
//!   The tone tests below check the same thing through the whole decimator.
//! - `COEFFS` was printed by the ignored test `print_taps`, which runs the recipe in f64
//!   (`designed_taps`). `the_taps_match_the_documented_design` fails if table and recipe drift
//!   apart. To change the design, edit the recipe, run
//!   `cargo test -p esp-shazam-core decimate::tests::print_taps -- --ignored --nocapture`
//!   and paste the output over the table.
//!
//! Cost. The 63 multiply-adds run only for the one frame in three whose output is kept, so 16,000
//! times a second, plus the channel sum and two stores for each of the 48,000 frames. The maths
//! is f32, which the ESP32's FPU does in hardware (it has no f64 unit).

const TAPS: usize = 63;
/// Input rate over output rate: 48 kHz to 16 kHz.
const DECIMATION: usize = 3;
/// The mono average halves the channel sum, and the shift from 24 to 16 bits divides by 2^8.
const OUTPUT_SCALE: f32 = 1.0 / 512.0;

const COEFFS: [f32; TAPS] = [
    0.00086070737,
    0.00045665243,
    -0.00067272625,
    -0.0018313864,
    -0.0019310073,
    -0.00036609668,
    0.0022075342,
    0.003957765,
    0.0030593746,
    -0.00070507557,
    -0.0051976205,
    -0.006952131,
    -0.003649608,
    0.0036076596,
    0.010170558,
    0.010575678,
    0.0027051915,
    -0.00958019,
    -0.017828884,
    -0.014363836,
    0.0015143914,
    0.021024436,
    0.029924614,
    0.017712398,
    -0.013378258,
    -0.045940794,
    -0.054600276,
    -0.020019677,
    0.058027167,
    0.15673484,
    0.2389991,
    0.270959,
    0.2389991,
    0.15673484,
    0.058027167,
    -0.020019677,
    -0.054600276,
    -0.045940794,
    -0.013378258,
    0.017712398,
    0.029924614,
    0.021024436,
    0.0015143914,
    -0.014363836,
    -0.017828884,
    -0.00958019,
    0.0027051915,
    0.010575678,
    0.010170558,
    0.0036076596,
    -0.003649608,
    -0.006952131,
    -0.0051976205,
    -0.00070507557,
    0.0030593746,
    0.003957765,
    0.0022075342,
    -0.00036609668,
    -0.0019310073,
    -0.0018313864,
    -0.00067272625,
    0.00045665243,
    0.00086070737,
];

/// Turns 48 kHz interleaved stereo into 16 kHz mono: the channels are averaged, low-passed by the
/// FIR above, and one output in three is kept. It remembers the filter and its place in the
/// three-to-one pattern between calls, so chunk boundaries change nothing.
#[derive(Debug)]
pub struct Decimator {
    /// The newest channel sums, stored twice (at `i` and `i + TAPS`) so that the last `TAPS` of them
    /// are always one contiguous slice and the filter needs no wrap-around.
    history: [f32; 2 * TAPS],
    /// Where the next sum goes, below `TAPS`. The last `TAPS` sums, oldest first, start here.
    next: usize,
    /// Frames since the last output, below `DECIMATION`.
    phase: usize,
}

impl Decimator {
    pub fn new() -> Self {
        Self {
            history: [0.0; 2 * TAPS],
            next: 0,
            phase: 0,
        }
    }

    /// Replaces the contents of `out` with the 16 kHz samples that `interleaved` completes: one per
    /// three whole frames, counting on from earlier calls. A trailing half frame is dropped, as in
    /// `stereo_to_mono`. It allocates only if `out` has less room than this call's output needs,
    /// so a caller that reuses one `Vec` allocates once.
    pub fn process(&mut self, interleaved: &[i32], out: &mut Vec<i16>) {
        out.clear();
        out.reserve(interleaved.len() / (2 * DECIMATION) + 1);
        for frame in interleaved.chunks_exact(2) {
            // Added as f32, so that no i32 can overflow, whatever the caller passes.
            let sum = frame[0] as f32 + frame[1] as f32;
            self.history[self.next] = sum;
            self.history[self.next + TAPS] = sum;
            self.next += 1;
            if self.next == TAPS {
                self.next = 0;
            }
            self.phase += 1;
            if self.phase == DECIMATION {
                self.phase = 0;
                out.push(self.filtered());
            }
        }
    }

    /// The filter output for the newest frame, scaled to 16 bits.
    fn filtered(&self) -> i16 {
        let window = &self.history[self.next..self.next + TAPS];
        let acc: f32 = window.iter().zip(&COEFFS).map(|(x, h)| x * h).sum();
        // `as` saturates, so overshoot past full scale clips at the rails instead of wrapping.
        (acc * OUTPUT_SCALE).round() as i16
    }
}

impl Default for Decimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{loudness_dbfs, zero_crossing_hz};
    use std::f64::consts::PI;

    /// Largest 24-bit sample.
    const FULL_SCALE: f64 = 8_388_607.0;
    const MAX: i32 = (1 << 23) - 1;
    const MIN: i32 = -(1 << 23);

    /// `seconds` of the same sine in both channels at 48 kHz, as 24-bit samples. It is computed in
    /// f64, because an f32 phase error would put noise into the signal that is louder than the
    /// filter's stopband. The phase offset keeps a 24 kHz tone off its zero crossings.
    fn tone(freq_hz: f64, amplitude: f64, seconds: f64) -> Vec<i32> {
        let frames = (48_000.0 * seconds) as usize;
        (0..frames)
            .flat_map(|n| {
                let phase = 2.0 * PI * freq_hz * n as f64 / 48_000.0 + 0.5;
                let v = (amplitude * FULL_SCALE * phase.sin()).round() as i32;
                [v, v]
            })
            .collect()
    }

    /// Deterministic 24-bit stereo noise from a linear congruential generator, so no crate is needed.
    fn noise(frames: usize) -> Vec<i32> {
        let mut state = 0x1234_5678u32;
        (0..2 * frames)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as i32 - (1 << 23)
            })
            .collect()
    }

    fn decimate(input: &[i32]) -> Vec<i16> {
        let mut out = vec![];
        Decimator::new().process(input, &mut out);
        out
    }

    /// RMS in dBFS with DC included. `loudness_dbfs` removes DC, which would hide a 16 kHz tone
    /// that folds down to DC.
    fn rms_dbfs(samples: &[i16]) -> f64 {
        let mean_square =
            samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / samples.len() as f64;
        10.0 * (mean_square / 32_768f64.powi(2)).log10()
    }

    /// How much a tone's level changes through the decimator, in dB. It skips the first 100 output
    /// samples, while the filter is still filling.
    fn gain_db(freq_hz: f64) -> f64 {
        const AMPLITUDE: f64 = 0.9;
        let out = decimate(&tone(freq_hz, AMPLITUDE, 0.2));
        rms_dbfs(&out[100..]) - 20.0 * (AMPLITUDE / 2f64.sqrt()).log10()
    }

    /// Modified Bessel function of the first kind, order 0, from its power series.
    fn bessel_i0(x: f64) -> f64 {
        let (mut sum, mut term) = (1.0, 1.0);
        for k in 1..50 {
            term *= (x / (2.0 * f64::from(k))).powi(2);
            sum += term;
        }
        sum
    }

    /// The recipe `COEFFS` was printed from: a sinc low-pass with its -6 dB point at 6.5 kHz
    /// (at 48 kHz), a Kaiser window with beta 4, scaled to unit gain at DC. All in f64.
    fn designed_taps() -> Vec<f64> {
        const CUTOFF_HZ: f64 = 6_500.0;
        const BETA: f64 = 4.0;
        let middle = (TAPS - 1) as f64 / 2.0;
        let omega = 2.0 * PI * CUTOFF_HZ / 48_000.0;
        let windowed: Vec<f64> = (0..TAPS)
            .map(|n| {
                let x = n as f64 - middle;
                let sinc = if x == 0.0 {
                    omega / PI
                } else {
                    (omega * x).sin() / (PI * x)
                };
                let r = x / middle;
                sinc * bessel_i0(BETA * (1.0 - r * r).sqrt()) / bessel_i0(BETA)
            })
            .collect();
        let sum: f64 = windowed.iter().sum();
        windowed.iter().map(|h| h / sum).collect()
    }

    /// The obvious implementation, to compare the real one against: convolve the mono sum in f64,
    /// keep every third output from the third frame on, scale to 16 bits and clamp.
    fn reference(input: &[i32]) -> Vec<i16> {
        let taps = designed_taps();
        let mono: Vec<f64> = input
            .chunks_exact(2)
            .map(|f| f64::from(f[0]) + f64::from(f[1]))
            .collect();
        (2..mono.len())
            .step_by(3)
            .map(|n| {
                let acc: f64 = taps
                    .iter()
                    .zip(mono[..=n].iter().rev())
                    .map(|(h, x)| h * x)
                    .sum();
                (acc / 512.0).round().clamp(-32_768.0, 32_767.0) as i16
            })
            .collect()
    }

    #[test]
    fn a_one_khz_tone_keeps_its_level_and_pitch() {
        // Half scale in 24 bits is -9.03 dBFS RMS, and must read the same after the shift to 16 bits.
        let out = decimate(&tone(1_000.0, 0.5, 1.0));
        let settled = &out[100..];
        let level = loudness_dbfs(settled);
        assert!((level - -9.03).abs() < 0.5, "got {level} dBFS");
        let hz = zero_crossing_hz(settled, 16_000);
        assert!((hz - 1_000.0).abs() < 5.0, "got {hz} Hz");
    }

    #[test]
    fn the_passband_is_flat_up_to_the_top_of_the_fingerprint_bands() {
        for hz in [100.0, 1_000.0, 2_500.0, 4_000.0, 5_000.0, 5_500.0] {
            let gain = gain_db(hz);
            assert!(gain.abs() < 0.1, "{hz} Hz changed by {gain} dB");
        }
    }

    #[test]
    fn a_seven_and_a_half_khz_tone_is_attenuated_at_least_40_db() {
        let gain = gain_db(7_500.0);
        assert!(gain < -40.0, "got {gain} dB");
    }

    #[test]
    fn everything_that_could_alias_into_the_band_is_attenuated_at_least_40_db() {
        // From 7.5 kHz to just under the input's 24 kHz Nyquist, in 250 Hz steps. These tones fold
        // down into the output band (16 kHz lands on DC, 12 kHz on 4 kHz), so this measures the
        // aliasing the filter prevents, not only its response.
        for hz in (7_500..24_000).step_by(250) {
            let gain = gain_db(f64::from(hz));
            assert!(gain < -40.0, "{hz} Hz fell only {gain} dB");
        }
    }

    #[test]
    fn both_channels_count_equally() {
        let both = tone(1_000.0, 0.5, 0.2);
        let left_only: Vec<i32> = both.chunks_exact(2).flat_map(|f| [f[0], 0]).collect();
        let opposed: Vec<i32> = both.chunks_exact(2).flat_map(|f| [f[0], -f[1]]).collect();
        let level_of = |input: &[i32]| rms_dbfs(&decimate(input)[100..]);
        // One channel of two carries half the amplitude: 6.02 dB down. (That the mix is an average
        // and not a sum is pinned by the 1 kHz test, which puts the tone in both channels.)
        let drop = level_of(&both) - level_of(&left_only);
        assert!((drop - 6.02).abs() < 0.05, "got {drop} dB");
        // Opposite channels cancel exactly.
        assert!(decimate(&opposed).iter().all(|&s| s == 0));
    }

    #[test]
    fn feeding_odd_sized_chunks_matches_feeding_one_call() {
        let input = noise(4_800);
        let whole = decimate(&input);
        assert_eq!(whole.len(), 1_600);

        let mut decimator = Decimator::new();
        let (mut chunked, mut out) = (vec![], vec![]);
        let mut rest = &input[..];
        // Frames per chunk: none of them lines up with the three-to-one ratio, and one is empty.
        for frames in [1, 7, 5, 101, 0, 13, 997, 3, 2, 31].iter().cycle() {
            let take = (2 * frames).min(rest.len());
            decimator.process(&rest[..take], &mut out);
            chunked.extend_from_slice(&out);
            rest = &rest[take..];
            if rest.is_empty() {
                break;
            }
        }
        assert_eq!(chunked, whole);
    }

    #[test]
    fn a_half_frame_at_the_end_is_dropped() {
        let input = noise(300);
        let mut with_stray_sample = input.clone();
        with_stray_sample.push(12_345);
        let out = decimate(&input);
        assert_eq!(out.len(), 100);
        assert_eq!(decimate(&with_stray_sample), out);
    }

    #[test]
    fn an_impulse_comes_out_as_every_third_tap() {
        // Left and right at 2^22 make a mono sum of 2^23, which the output scale (1/512) turns into
        // 16384 times each tap. The frames 2, 5, 8, ... that get an output see taps 2, 5, 8, ...
        let mut input = vec![0; 2 * TAPS];
        input[0] = 1 << 22;
        input[1] = 1 << 22;
        let expected: Vec<i16> = (2..TAPS)
            .step_by(3)
            .map(|k| (COEFFS[k] * 16_384.0).round() as i16)
            .collect();
        assert_eq!(decimate(&input), expected);
    }

    #[test]
    fn it_matches_a_plain_f64_convolution() {
        let input = noise(4_800);
        let got = decimate(&input);
        let want = reference(&input);
        assert_eq!(got.len(), want.len());
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert!(
                (i32::from(*g) - i32::from(*w)).abs() <= 1,
                "output {i}: {g} vs {w}"
            );
        }
    }

    #[test]
    fn full_scale_input_settles_on_the_rails_without_overflow() {
        // The filter has unity gain at DC, so constant full scale ends up on the rail.
        let high = decimate(&vec![MAX; 2 * 600]);
        assert!(
            high[30..].iter().all(|&s| s == i16::MAX),
            "{:?}",
            &high[30..40]
        );
        let low = decimate(&vec![MIN; 2 * 600]);
        assert!(
            low[30..].iter().all(|&s| s == i16::MIN),
            "{:?}",
            &low[30..40]
        );
    }

    #[test]
    fn a_full_scale_square_wave_clips_at_the_rails_instead_of_wrapping() {
        // Half periods of 300 frames (100 outputs). The low-pass overshoots each edge by several
        // percent, which is past full scale: it has to clip there.
        let square: Vec<i32> = (0..2 * 60 * 300)
            .map(|i| if (i / 2 / 300) % 2 == 0 { MAX } else { MIN })
            .collect();
        let got = decimate(&square);
        let want = reference(&square);
        assert_eq!(got.len(), want.len());
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert!(
                (i32::from(*g) - i32::from(*w)).abs() <= 1,
                "output {i}: {g} vs {w}"
            );
        }
        assert_eq!(got.iter().max(), Some(&i16::MAX));
        assert_eq!(got.iter().min(), Some(&i16::MIN));
    }

    #[test]
    fn samples_beyond_24_bits_saturate_instead_of_panicking() {
        // Outside the input contract, but a wrong shift in the firmware must not take the audio thread down.
        let loud: Vec<i32> = (0..2 * 600)
            .map(|i| if i % 2 == 0 { i32::MAX } else { i32::MIN })
            .collect();
        assert_eq!(decimate(&loud).len(), 200);
        let high = decimate(&vec![i32::MAX; 2 * 600]);
        assert!(high[30..].iter().all(|&s| s == i16::MAX));
    }

    #[test]
    fn the_taps_are_symmetric_and_pass_dc_at_unity() {
        for (k, (a, b)) in COEFFS.iter().zip(COEFFS.iter().rev()).enumerate() {
            assert_eq!(a, b, "tap {k} differs from its mirror");
        }
        let sum: f64 = COEFFS.iter().map(|&h| f64::from(h)).sum();
        assert!((sum - 1.0).abs() < 1e-6, "got {sum}");
    }

    #[test]
    fn the_taps_match_the_documented_design() {
        let designed = designed_taps();
        assert_eq!(COEFFS.len(), designed.len());
        for (k, (&got, want)) in COEFFS.iter().zip(&designed).enumerate() {
            assert!(
                (f64::from(got) - want).abs() < 1e-7,
                "tap {k}: {got} vs {want}"
            );
        }
    }

    /// Regenerates the table:
    /// `cargo test -p esp-shazam-core decimate::tests::print_taps -- --ignored --nocapture`
    #[test]
    #[ignore = "prints the COEFFS table"]
    fn print_taps() {
        for h in designed_taps() {
            println!("    {:?},", h as f32);
        }
    }
}
