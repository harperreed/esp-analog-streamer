// ABOUTME: The single source of truth for what the board reports: state, loudness, now playing, history.
// ABOUTME: Serializes the /api/state body and the MQTT now_playing payload.

use std::collections::VecDeque;

use serde::Serialize;

use crate::audio::SILENCE_DBFS;
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
    level_dbfs: f32,
    rate_limited_until: Option<String>,
}

#[derive(Serialize)]
struct StateView<'a> {
    state: State,
    level_dbfs: f32,
    rate_limited_until: Option<&'a str>,
    now: Option<&'a NowPlaying>,
    history: &'a VecDeque<NowPlaying>,
}

impl SongStore {
    pub fn new(history_cap: usize) -> Self {
        Self {
            now: None,
            history: VecDeque::new(),
            history_cap,
            state: State::Quiet,
            level_dbfs: SILENCE_DBFS,
            rate_limited_until: None,
        }
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
    pub fn set_status(
        &mut self,
        state: State,
        level_dbfs: f32,
        rate_limited_until: Option<String>,
    ) {
        self.state = state;
        self.level_dbfs = (level_dbfs * 10.0).round() / 10.0;
        self.rate_limited_until = rate_limited_until;
    }
    pub fn state_json(&self) -> String {
        let view = StateView {
            state: self.state,
            level_dbfs: self.level_dbfs,
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
        let store = SongStore::new(20);
        assert_eq!(
            parse(&store.state_json()),
            json!({"state": "quiet", "level_dbfs": -120.0, "rate_limited_until": null, "now": null, "history": []})
        );
        assert_eq!(store.now_json(), "null");
    }

    #[test]
    fn set_now_fills_now_and_the_top_of_history() {
        let mut store = SongStore::new(20);
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
        let mut store = SongStore::new(20);
        assert!(store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:22:01Z".into()));
        assert!(!store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:23:01Z".into()));
        let state = parse(&store.state_json());
        assert_eq!(state["history"].as_array().unwrap().len(), 1);
        assert_eq!(state["now"]["identified_at"], "2026-10-03T21:22:01Z");
    }

    #[test]
    fn history_keeps_the_newest_twenty() {
        let mut store = SongStore::new(20);
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
        let mut store = SongStore::new(20);
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
        let mut store = SongStore::new(20);
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
        let mut store = SongStore::new(20);
        store.set_status(
            State::RateLimited,
            -18.437,
            Some("2026-10-03T21:27:01Z".into()),
        );
        let state = parse(&store.state_json());
        assert_eq!(state["state"], "rate_limited");
        assert_eq!(state["level_dbfs"], json!(-18.4));
        assert_eq!(state["rate_limited_until"], "2026-10-03T21:27:01Z");
    }
}
