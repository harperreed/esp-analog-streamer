// ABOUTME: Whole-frame startup buffering presets for board-local FLAC playback.
// ABOUTME: Shares stable settings values and duration targets with stream admission.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BufferPreset {
    #[serde(rename = "low")]
    Low,
    #[default]
    #[serde(rename = "med")]
    Medium,
    #[serde(rename = "high")]
    High,
}

impl BufferPreset {
    pub const fn seconds(self) -> u64 {
        match self {
            Self::Low => 1,
            Self::Medium => 4,
            Self::High => 8,
        }
    }
    pub const fn frames(self) -> usize {
        match self {
            Self::Low => 12,
            Self::Medium => 47,
            Self::High => 94,
        }
    }
    pub const fn query_value(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "med",
            Self::High => "high",
        }
    }
}
