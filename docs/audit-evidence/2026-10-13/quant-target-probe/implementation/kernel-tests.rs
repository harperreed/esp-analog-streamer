// ABOUTME: Checks signed arithmetic, ties, saturation and discrete dither using the real kernel.
// ABOUTME: Uses literal vectors and exhaustive byte pairs rather than mocked quantization.

#[path = "quant-cost/contract.rs"]
#[allow(dead_code)]
mod contract;
use contract::{dither_quantize, quantize, Policy};

#[test]
fn xorshift_matches_literal_seed_one_sequence() {
    let mut state = 1;
    for expected in [270369, 67634689, 2647435461, 307599695, 2398689233] {
        assert_eq!(contract::next_u32(&mut state), expected);
    }
}

#[test]
fn even_chunk_boundaries_preserve_samples_and_rng_state() {
    let mut source = vec![0; 8192];
    for kind in contract::KINDS {
        contract::fill_input(kind, &mut source);
        for policy in [Policy::Trunc, Policy::Round, Policy::Tpdf] {
            let mut whole = vec![0; source.len()];
            let mut whole_state = contract::SEEDS;
            contract::transform(&source, &mut whole, policy, &mut whole_state);
            for size in [14, contract::CHUNK_SAMPLES] {
                let mut split = vec![0; source.len()];
                let mut split_state = contract::SEEDS;
                for (input, output) in source.chunks(size).zip(split.chunks_mut(size)) {
                    contract::transform(input, output, policy, &mut split_state);
                }
                assert_eq!(split, whole, "{kind}/{policy:?}/{size}");
                assert_eq!(split_state, whole_state);
            }
        }
    }
}

#[test]
fn sign_ties_and_saturation() {
    for (x, trunc, round) in [
        (0, 0, 0),
        (64, 0, 0),
        (-64, -1, 0),
        (127, 0, 0),
        (128, 0, 1),
        (129, 0, 1),
        (-127, -1, 0),
        (-128, -1, 0),
        (-129, -1, -1),
        (384, 1, 2),
        (-384, -2, -1),
        (8_388_479, 32767, 32767),
        (8_388_480, 32767, 32767),
        (-8_388_608, -32768, -32768),
        (i32::MIN, -32768, -32768),
        (i32::MAX, 32767, 32767),
    ] {
        assert_eq!(quantize(x, Policy::Trunc, &mut 1), trunc, "trunc {x}");
        assert_eq!(quantize(x, Policy::Round, &mut 1), round, "round {x}");
    }
}

#[test]
fn exhaustive_discrete_tpdf_zero_bias_and_support() {
    let mut counts = [0_i64; 3];
    for a in 0..=255_u8 {
        for b in 0..=255_u8 {
            let q = dither_quantize(0, a, b);
            assert!((-1..=1).contains(&q));
            counts[(q + 1) as usize] += 1;
            assert_eq!(dither_quantize(i32::MAX, a, b), 32767);
            assert_eq!(dither_quantize(i32::MIN, a, b), -32768);
        }
    }
    // The discrete ties-up lattice has mean +128/65536 = +1/512 LSB.
    assert_eq!(counts, [8128, 49152, 8256]);
}

#[test]
fn actual_seeded_kernel_has_bounded_bias_variance_and_channel_correlation() {
    let mut states = contract::SEEDS;
    let mut sum = [0_f64; 2];
    let mut squares = [0_f64; 2];
    let mut cross = 0_f64;
    let mut extrema = [[i32::MAX, i32::MIN]; 2];
    let count = 262_144;
    for _ in 0..count {
        let q = [
            quantize(0, Policy::Tpdf, &mut states[0]),
            quantize(0, Policy::Tpdf, &mut states[1]),
        ];
        for c in 0..2 {
            assert!((-1..=1).contains(&q[c]));
            extrema[c][0] = extrema[c][0].min(q[c]);
            extrema[c][1] = extrema[c][1].max(q[c]);
            sum[c] += f64::from(q[c]);
            squares[c] += f64::from(q[c] * q[c]);
        }
        cross += f64::from(q[0] * q[1]);
    }
    let mean = sum.map(|s| s / f64::from(count));
    let variance = [
        squares[0] / f64::from(count) - mean[0] * mean[0],
        squares[1] / f64::from(count) - mean[1] * mean[1],
    ];
    for c in 0..2 {
        assert_eq!(extrema[c], [-1, 1]);
        assert!((mean[c] - 1.0 / 512.0).abs() < 0.004, "mean {mean:?}");
        assert!(
            (variance[c] - (0.25 - 1.0 / 262144.0)).abs() < 0.005,
            "variance {variance:?}"
        );
    }
    let correlation =
        (cross / f64::from(count) - mean[0] * mean[1]) / (variance[0] * variance[1]).sqrt();
    assert!(
        correlation.abs() < 0.015,
        "cross-channel correlation {correlation}"
    );
    eprintln!(
        "seeded zero statistics mean={mean:?} variance={variance:?} correlation={correlation}"
    );
}
