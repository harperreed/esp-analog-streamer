// ABOUTME: The single source of truth for what the board reports: state, loudness, now playing, history.
// ABOUTME: Serializes the /api/state body and the MQTT now_playing payload.

use std::collections::VecDeque;

use serde::Serialize;

use crate::audio::{ChannelReading, StereoReading, SILENCE_DBFS};
use crate::policy::State;
use crate::shazam::Song;

/// A song plus when it first became "now". Serializes flat: song fields, then identified_at.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NowPlaying {
    #[serde(flatten)]
    pub song: Song,
    pub identified_at: String,
}

#[derive(Debug)]
pub struct SongStore {
    now: Option<NowPlaying>,
    history: VecDeque<NowPlaying>,
    history_cap: usize,
    state: State,
    measurement: Option<StampedMeasurement>,
    rate_limited_until: Option<String>,
    shazam_enabled: bool,
    quiet_threshold_dbfs: f32,
}

#[derive(Debug)]
struct StampedMeasurement {
    reading: StereoReading,
    measured_at: std::time::Instant,
}

#[derive(Serialize)]
struct MeasurementView<'a> {
    age_ms: u128,
    window_frames: u32,
    channels: ChannelsView<'a>,
}

#[derive(Serialize)]
struct ChannelsView<'a> {
    left: &'a ChannelReading,
    right: &'a ChannelReading,
}

#[derive(Serialize)]
struct StateView<'a> {
    state: State,
    shazam_enabled: bool,
    level_dbfs: f32,
    /// Level at or above the quiet threshold on a measurement under five seconds old: audio is arriving, music or not.
    audio_present: bool,
    quiet_threshold_dbfs: f32,
    last_measurement: Option<MeasurementView<'a>>,
    rate_limited_until: Option<&'a str>,
    now: Option<&'a NowPlaying>,
    history: &'a VecDeque<NowPlaying>,
}

impl SongStore {
    /// `shazam_enabled` is the firmware's build-time setting. The store only reports it, so a reader of
    /// `/api/state` can tell a board with no recognizer from a quiet room.
    pub fn new(history_cap: usize, shazam_enabled: bool) -> Self {
        Self {
            now: None,
            history: VecDeque::new(),
            history_cap,
            state: State::Quiet,
            measurement: None,
            rate_limited_until: None,
            shazam_enabled,
            quiet_threshold_dbfs: -50.0,
        }
    }

    /// The firmware's configured quiet threshold, the line between `quiet` and `audio_present`.
    pub fn set_quiet_threshold(&mut self, dbfs: f32) {
        self.quiet_threshold_dbfs = dbfs;
    }

    fn audio_present_at(&self, now: std::time::Instant) -> bool {
        self.measurement.as_ref().is_some_and(|m| {
            now.checked_duration_since(m.measured_at)
                .is_some_and(|age| age.as_secs() < 5)
                && m.reading.level_dbfs >= self.quiet_threshold_dbfs
        })
    }
    pub fn now_key(&self) -> Option<&str> {
        self.now.as_ref().map(|n| n.song.key.as_str())
    }

    pub fn has_now(&self) -> bool {
        self.now.is_some()
    }
    /// Makes `song` the current song and puts it at the top of the history.
    /// Returns false, changing nothing, when it is already the current song.
    pub fn set_now(&mut self, song: Song, identified_at: String) -> bool {
        if self.now_key() == Some(song.key.as_str()) {
            return false;
        }
        let entry = NowPlaying {
            song,
            identified_at,
        };
        self.history.push_front(entry.clone());
        self.history.truncate(self.history_cap);
        self.now = Some(entry);
        true
    }
    /// Returns false when nothing was playing.
    pub fn clear_now(&mut self) -> bool {
        self.now.take().is_some()
    }
    /// Records the listen policy's view. It does not touch the level: the audio path measures that
    /// whether or not a recognizer runs, so the two have separate writers.
    pub fn set_status(&mut self, state: State, rate_limited_until: Option<String>) {
        self.state = state;
        self.rate_limited_until = rate_limited_until;
    }
    /// Records a completed capture measurement, rounding only the combined value to 0.1 dB.
    /// Policy and page read that same value; retries preserve the original capture time.
    pub fn set_measurement(&mut self, mut reading: StereoReading, measured_at: std::time::Instant) {
        reading.level_dbfs = (reading.level_dbfs * 10.0).round() / 10.0;
        self.measurement = Some(StampedMeasurement {
            reading,
            measured_at,
        });
    }
    pub fn level_dbfs(&self) -> f32 {
        self.measurement
            .as_ref()
            .map_or(SILENCE_DBFS, |m| m.reading.level_dbfs)
    }
    /// Includes capture freshness so automatic playback cannot act on a stalled meter.
    pub fn level_sample(&self) -> Option<(f32, std::time::Instant)> {
        self.measurement
            .as_ref()
            .map(|m| (m.reading.level_dbfs, m.measured_at))
    }
    pub fn state_json(&self) -> String {
        self.state_json_at(std::time::Instant::now())
    }

    fn state_json_at(&self, now: std::time::Instant) -> String {
        // This is historical capture evidence. Age alone cannot establish capture continuity.
        let last_measurement = self.measurement.as_ref().and_then(|m| {
            now.checked_duration_since(m.measured_at)
                .map(|age| MeasurementView {
                    age_ms: age.as_millis(),
                    window_frames: m.reading.window_frames,
                    channels: ChannelsView {
                        left: &m.reading.channels[0],
                        right: &m.reading.channels[1],
                    },
                })
        });
        let view = StateView {
            state: self.state,
            shazam_enabled: self.shazam_enabled,
            level_dbfs: self.level_dbfs(),
            audio_present: self.audio_present_at(now),
            quiet_threshold_dbfs: self.quiet_threshold_dbfs,
            last_measurement,
            rate_limited_until: self.rate_limited_until.as_deref(),
            now: self.now.as_ref(),
            history: &self.history,
        };
        serde_json::to_string(&view).expect("state view always serializes")
    }

    pub fn now_json(&self) -> String {
        serde_json::to_string(&self.now).expect("now always serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn audio_present_follows_the_threshold_and_measurement_age() {
        let mut store = SongStore::new(2, false);
        store.set_quiet_threshold(-60.0);
        let t0 = std::time::Instant::now();
        let view = |store: &SongStore, at| {
            serde_json::from_str::<Value>(&store.state_json_at(at)).unwrap()
        };
        assert_eq!(
            view(&store, t0)["audio_present"],
            false,
            "no measurement yet"
        );
        store.set_measurement(reading(-23.4), t0);
        assert_eq!(view(&store, t0)["audio_present"], true);
        assert_eq!(view(&store, t0)["quiet_threshold_dbfs"], -60.0);
        assert_eq!(
            view(&store, t0 + std::time::Duration::from_secs(6))["audio_present"],
            false,
            "a stale meter is not audio"
        );
        store.set_measurement(reading(-75.0), t0);
        assert_eq!(view(&store, t0)["audio_present"], false);
        store.set_measurement(reading(-60.0), t0);
        assert_eq!(
            view(&store, t0)["audio_present"],
            true,
            "at the threshold counts"
        );
    }

    fn reading(level_dbfs: f32) -> StereoReading {
        StereoReading {
            level_dbfs,
            window_frames: 12_000,
            channels: [ChannelReading {
                rms_dbfs: level_dbfs,
                peak_normalized: 0.1,
                digital_rail_samples: 0,
                dc_offset_normalized: 0.0,
            }; 2],
        }
    }

    fn song(key: &str, title: &str, artist: &str) -> Song {
        Song {
            key: key.into(),
            title: title.into(),
            artist: artist.into(),
            album: Some("An Album".into()),
            cover_url: Some("https://example.com/cover.jpg".into()),
            shazam_url: None,
        }
    }

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("valid JSON")
    }

    #[test]
    fn empty_store_reports_quiet_and_nothing_playing() {
        let store = SongStore::new(20, true);
        assert_eq!(
            parse(&store.state_json()),
            json!({"state": "quiet", "shazam_enabled": true, "level_dbfs": -120.0, "audio_present": false, "quiet_threshold_dbfs": -50.0, "last_measurement": null, "rate_limited_until": null, "now": null, "history": []})
        );
        assert_eq!(store.now_json(), "null");
    }

    #[test]
    fn state_reports_shazam_on() {
        let store = SongStore::new(20, true);
        assert_eq!(parse(&store.state_json())["shazam_enabled"], json!(true));
    }

    #[test]
    fn state_reports_shazam_off_and_still_publishes_null() {
        let store = SongStore::new(20, false);
        assert_eq!(parse(&store.state_json())["shazam_enabled"], json!(false));
        assert_eq!(store.now_json(), "null");
    }

    #[test]
    fn set_now_fills_now_and_the_top_of_history() {
        let mut store = SongStore::new(20, true);
        assert!(store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:22:01Z".into()));
        assert_eq!(store.now_key(), Some("1"));
        let state = parse(&store.state_json());
        let expected = json!({
            "key": "1", "title": "Title", "artist": "Artist", "album": "An Album",
            "cover_url": "https://example.com/cover.jpg", "shazam_url": null,
            "identified_at": "2026-10-03T21:22:01Z"
        });
        assert_eq!(state["now"], expected);
        assert_eq!(state["history"][0], expected);
        assert_eq!(parse(&store.now_json()), expected);
    }

    #[test]
    fn the_same_key_twice_is_not_a_change() {
        let mut store = SongStore::new(20, true);
        assert!(store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:22:01Z".into()));
        assert!(!store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:23:01Z".into()));
        let state = parse(&store.state_json());
        assert_eq!(state["history"].as_array().unwrap().len(), 1);
        assert_eq!(state["now"]["identified_at"], "2026-10-03T21:22:01Z");
    }

    #[test]
    fn history_keeps_the_newest_twenty() {
        let mut store = SongStore::new(20, true);
        for i in 0..25 {
            store.set_now(
                song(&i.to_string(), "T", "A"),
                "2026-10-03T21:22:01Z".into(),
            );
        }
        let state = parse(&store.state_json());
        let history = state["history"].as_array().unwrap();
        assert_eq!(history.len(), 20);
        assert_eq!(history[0]["key"], "24");
        assert_eq!(history[19]["key"], "5");
    }

    #[test]
    fn clear_now_keeps_history() {
        let mut store = SongStore::new(20, true);
        store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:22:01Z".into());
        assert!(store.clear_now());
        assert!(!store.clear_now());
        assert!(!store.has_now());
        let state = parse(&store.state_json());
        assert_eq!(state["now"], Value::Null);
        assert_eq!(state["history"].as_array().unwrap().len(), 1);
        assert_eq!(store.now_json(), "null");
    }

    #[test]
    fn non_ascii_titles_survive_unescaped() {
        let mut store = SongStore::new(20, true);
        store.set_now(
            song("7", "Beyoncé", "夜に駆ける"),
            "2026-10-03T21:22:01Z".into(),
        );
        let json = store.now_json();
        assert!(
            json.contains("Beyoncé") && json.contains("夜に駆ける"),
            "got {json}"
        );
        assert_eq!(parse(&json)["artist"], "夜に駆ける");
    }

    #[test]
    fn status_reports_state_rounded_level_and_rate_limit() {
        let mut store = SongStore::new(20, true);
        store.set_measurement(reading(-18.437), std::time::Instant::now());
        store.set_status(State::RateLimited, Some("2026-10-03T21:27:01Z".into()));
        let state = parse(&store.state_json());
        assert_eq!(state["state"], "rate_limited");
        assert_eq!(state["level_dbfs"], json!(-18.4));
        assert_eq!(state["rate_limited_until"], "2026-10-03T21:27:01Z");
    }

    #[test]
    fn a_fresh_store_reads_silence() {
        assert_eq!(SongStore::new(20, true).level_dbfs(), SILENCE_DBFS);
    }

    #[test]
    fn level_is_stored_rounded_to_a_tenth_and_reported_as_stored() {
        let mut store = SongStore::new(20, true);
        store.set_measurement(reading(-18.437), std::time::Instant::now());
        assert_eq!(store.level_dbfs(), -18.4);
        assert_eq!(parse(&store.state_json())["level_dbfs"], json!(-18.4));
    }

    #[test]
    fn level_sample_requires_capture_and_refreshes_even_for_equal_levels() {
        let mut store = SongStore::new(20, false);
        assert_eq!(store.level_sample(), None);
        let before = std::time::Instant::now();
        store.set_measurement(reading(-42.04), std::time::Instant::now());
        let (level, measured) = store.level_sample().unwrap();
        assert_eq!(level, -42.0);
        assert!(measured >= before && measured <= std::time::Instant::now());
        let before = std::time::Instant::now();
        store.set_measurement(reading(-42.04), std::time::Instant::now());
        assert!(store.level_sample().unwrap().1 >= before);
    }

    #[test]
    fn delayed_level_publication_preserves_measurement_age() {
        let mut store = SongStore::new(20, false);
        let measured = std::time::Instant::now();
        let serialized_at = measured + std::time::Duration::from_millis(10_250);
        store.set_measurement(reading(-20.0), measured);
        assert_eq!(store.level_sample(), Some((-20.0, measured)));
        assert_eq!(
            parse(&store.state_json_at(serialized_at))["last_measurement"]["age_ms"],
            10_250
        );
        // A publication retry must not reset age even for an identical measurement.
        store.set_measurement(reading(-20.0), measured);
        assert_eq!(
            parse(&store.state_json_at(serialized_at))["last_measurement"]["age_ms"],
            10_250
        );
        let later = measured + std::time::Duration::from_millis(2_000);
        store.set_measurement(reading(-20.0), later);
        assert_eq!(
            parse(&store.state_json_at(serialized_at))["last_measurement"]["age_ms"],
            8_250
        );
        assert_eq!(store.level_sample(), Some((-20.0, later)));
        // Future capture times provide no historical snapshot.
        assert_eq!(
            parse(&store.state_json_at(measured))["last_measurement"],
            Value::Null
        );
    }

    #[test]
    fn set_status_leaves_the_level_alone() {
        let mut store = SongStore::new(20, true);
        store.set_measurement(reading(-30.0), std::time::Instant::now());
        store.set_status(State::Searching, None);
        store.set_status(State::RateLimited, Some("2026-10-03T21:27:01Z".into()));
        assert_eq!(store.level_dbfs(), -30.0);
        assert_eq!(parse(&store.state_json())["level_dbfs"], json!(-30.0));
    }

    #[test]
    fn set_measurement_leaves_the_status_alone() {
        let mut store = SongStore::new(20, true);
        store.set_status(State::RateLimited, Some("2026-10-03T21:27:01Z".into()));
        store.set_measurement(reading(-42.0), std::time::Instant::now());
        let state = parse(&store.state_json());
        assert_eq!(state["state"], "rate_limited");
        assert_eq!(state["rate_limited_until"], "2026-10-03T21:27:01Z");
        assert_eq!(state["level_dbfs"], json!(-42.0));
    }

    #[test]
    fn a_board_that_only_meters_reports_a_live_level_and_stays_quiet() {
        // With Shazam off nothing calls set_status; the audio thread's set_measurement is the only writer.
        let mut store = SongStore::new(20, false);
        let at = std::time::Instant::now();
        store.set_measurement(reading(-23.46), at);
        assert_eq!(
            parse(&store.state_json_at(at + std::time::Duration::from_millis(2500))),
            json!({"state": "quiet", "shazam_enabled": false, "level_dbfs": -23.5,
                "audio_present": true, "quiet_threshold_dbfs": -50.0,
                "last_measurement": {"age_ms": 2500, "window_frames": 12000,
                    "channels": {"left": {"rms_dbfs": -23.46, "peak_normalized": 0.1, "digital_rail_samples": 0, "dc_offset_normalized": 0.0},
                        "right": {"rms_dbfs": -23.46, "peak_normalized": 0.1, "digital_rail_samples": 0, "dc_offset_normalized": 0.0}}},
                "rate_limited_until": null, "now": null, "history": []})
        );
    }
}
