// ABOUTME: Exports target-compiled channel diagnostic type sizes without running or flashing firmware.
// ABOUTME: This is throwaway evidence source; old layouts are explicitly reconstructed from 28575fc.

use esp_shazam_core::audio::{StereoLevelMeter, StereoReading};
use esp_shazam_core::policy::State;
use esp_shazam_core::store::{NowPlaying, SongStore};
use std::collections::VecDeque;
use std::mem::{align_of, size_of};
use std::time::Instant;

// These retain the field types/order in 28575fc:audio.rs and store.rs. Their
// repr(Rust) layouts are reconstructed, not evidence of the archived ELF ABI.
#[allow(dead_code)]
struct ReconstructedSums {
    count: u64,
    sum: i64,
    sum_sq: i64,
}

#[allow(dead_code)]
struct ReconstructedStereoLevelMeter {
    intervals: [[ReconstructedSums; 2]; 4],
    filling: [ReconstructedSums; 2],
    next: usize,
    channel: usize,
    interval: usize,
}

#[allow(dead_code)]
struct ReconstructedSongStore {
    now: Option<NowPlaying>,
    history: VecDeque<NowPlaying>,
    history_cap: usize,
    state: State,
    level_dbfs: f32,
    level_measured_at: Option<Instant>,
    rate_limited_until: Option<String>,
    shazam_enabled: bool,
}

type PendingReading = Option<(StereoReading, Instant)>;
type PendingScalar = Option<(f32, Instant)>;

// Sum of baseline field sizes is a lower bound, independent of field reordering
// and padding. NowPlaying/State must remain unchanged from the inspected HEAD.
const BASELINE_METER_MIN: usize = 10 * (size_of::<u64>() + 2 * size_of::<i64>())
    + 3 * size_of::<usize>();
const BASELINE_STORE_MIN: usize = size_of::<Option<NowPlaying>>()
    + size_of::<VecDeque<NowPlaying>>()
    + size_of::<usize>()
    + size_of::<State>()
    + size_of::<f32>()
    + size_of::<Option<Instant>>()
    + size_of::<Option<String>>()
    + size_of::<bool>();

// Includes one conservative transient reading. This bounds type storage growth;
// it cannot certify generated frame size or real capture stack high-water.
const CURRENT_STORAGE: usize = size_of::<StereoLevelMeter>()
    + size_of::<SongStore>()
    + size_of::<PendingReading>()
    + size_of::<StereoReading>();
const BASELINE_STORAGE_MIN: usize = BASELINE_METER_MIN
    + BASELINE_STORE_MIN
    + size_of::<PendingScalar>()
    + size_of::<f32>();
const _: () = assert!(CURRENT_STORAGE <= BASELINE_STORAGE_MIN + 512);

// Entries, in order: actual meter size/alignment, reading size/alignment,
// store size/alignment, pending reading size/alignment, Instant size/alignment,
// old scalar pending size, reconstructed old meter/store sizes,
// baseline meter/store field-size lower bounds, current total storage,
// baseline storage lower bound. Each word is target-native u32, little-endian.
#[used]
#[no_mangle]
pub static CHANNEL_LAYOUT_SIZES: [u32; 17] = [
    size_of::<StereoLevelMeter>() as u32,
    align_of::<StereoLevelMeter>() as u32,
    size_of::<StereoReading>() as u32,
    align_of::<StereoReading>() as u32,
    size_of::<SongStore>() as u32,
    align_of::<SongStore>() as u32,
    size_of::<PendingReading>() as u32,
    align_of::<PendingReading>() as u32,
    size_of::<Instant>() as u32,
    align_of::<Instant>() as u32,
    size_of::<PendingScalar>() as u32,
    size_of::<ReconstructedStereoLevelMeter>() as u32,
    size_of::<ReconstructedSongStore>() as u32,
    BASELINE_METER_MIN as u32,
    BASELINE_STORE_MIN as u32,
    CURRENT_STORAGE as u32,
    BASELINE_STORAGE_MIN as u32,
];

fn main() {
    esp_idf_svc::sys::link_patches();
    std::hint::black_box(&CHANNEL_LAYOUT_SIZES);
}
