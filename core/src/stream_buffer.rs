// ABOUTME: Whole-frame startup buffering presets for board-local compressed playback.
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
    /// Rounds the duration up to whole encoder blocks at 48 kHz.
    pub const fn frames(self, block_samples: usize) -> usize {
        assert!(block_samples > 0 && block_samples <= 48_000);
        (self.seconds() as usize * 48_000).div_ceil(block_samples)
    }
    pub const fn query_value(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "med",
            Self::High => "high",
        }
    }
}
