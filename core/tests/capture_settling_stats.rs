// ABOUTME: Checks real settling-fixture channel moments against hand-derived numeric expectations.
// ABOUTME: Covers channel selection, silence, signed/DC samples and a full block of 24-bit extremes.

#[path = "../../firmware/examples/capture-settling/stats.rs"]
mod stats;

use stats::ChannelStats;

#[test]
fn silence_has_zero_extrema_and_moments_on_both_channels() {
    let samples = [0; 480];
    for channel in 0..2 {
        let stats = ChannelStats::measure(&samples, channel);
        assert_eq!(
            (stats.min, stats.max, stats.sum, stats.sum_squares),
            (0, 0, 0, 0)
        );
    }
}

#[test]
fn selects_each_channel_and_preserves_negative_sum_and_dc_energy() {
    // Left: -3, -1, 2, 4. Right: four -7 values.
    let samples = [-3, -7, -1, -7, 2, -7, 4, -7];
    let left = ChannelStats::measure(&samples, 0);
    let right = ChannelStats::measure(&samples, 1);
    assert_eq!(
        (left.min, left.max, left.sum, left.sum_squares),
        (-3, 4, 2, 30)
    );
    assert_eq!(
        (right.min, right.max, right.sum, right.sum_squares),
        (-7, -7, -28, 196)
    );
}

#[test]
fn full_block_of_alternating_24_bit_extremes_keeps_64_bit_energy() {
    let mut samples = [0; 480];
    for stereo_pair in samples.chunks_exact_mut(4) {
        stereo_pair.copy_from_slice(&[-8_388_608, 8_388_607, 8_388_607, -8_388_608]);
    }
    // Each channel has 120 of each extreme. Sum = -120; energy is
    // 120 * (70,368,744,177,664 + 70,368,727,400,449) = 16,888,496,589,373,560.
    for channel in 0..2 {
        let stats = ChannelStats::measure(&samples, channel);
        assert_eq!(
            (stats.min, stats.max, stats.sum, stats.sum_squares),
            (-8_388_608, 8_388_607, -120, 16_888_496_589_373_560),
        );
    }
}

#[test]
fn empty_read_reports_zero_stats_instead_of_extrema_sentinels() {
    for channel in 0..2 {
        let stats = ChannelStats::measure(&[], channel);
        assert_eq!(
            (stats.min, stats.max, stats.sum, stats.sum_squares),
            (0, 0, 0, 0)
        );
    }
}
