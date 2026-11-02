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

/// Persisted compressed output and its complete frame/storage profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Flac,
    Mp3,
}
impl OutputFormat {
    /// Lock-free word for the format; the runtime and health gate store this in atomics.
    pub const fn code(self) -> u32 {
        match self {
            Self::Flac => 0,
            Self::Mp3 => 1,
        }
    }
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Flac),
            1 => Some(Self::Mp3),
            _ => None,
        }
    }
    pub const fn format(self) -> crate::stream_http::Format {
        match self {
            Self::Flac => crate::stream_http::Format::Flac,
            Self::Mp3 => crate::stream_http::Format::Mp3,
        }
    }
    pub const fn block_frames(self) -> usize {
        match self {
            Self::Flac => crate::flac::BLOCK_FRAMES,
            Self::Mp3 => 1152,
        }
    }
    pub const fn max_frame_bytes(self) -> usize {
        match self {
            Self::Flac => crate::flac::MAX_FRAME_BYTES,
            Self::Mp3 => 960,
        }
    }
    pub const fn history_frames(self, recognition: bool) -> usize {
        match self {
            Self::Flac if recognition => 24,
            Self::Flac => 141,
            Self::Mp3 => 500,
        }
    }
    /// How long the encoded ring holds audio.
    pub const fn ring_depth_ms(self, recognition: bool) -> u64 {
        (self.history_frames(recognition) * self.block_frames()) as u64 * 1_000 / 48_000
    }
    /// The most a live listener's frame write may take: twice the ring depth. The frame being
    /// written is a private copy, so the ring wrapping underneath is harmless; the bound exists to
    /// call a socket dead. One ring depth was too tight on hardware: an 8 s blackhole ended a
    /// listener with a 12.05 s poll, because lwIP's retransmit backoff adds several seconds after
    /// the path returns (oct16-interruption-1, stall-8-2).
    pub const fn live_write_limit_ms(self, recognition: bool) -> u64 {
        2 * self.ring_depth_ms(recognition)
    }
    pub const fn write_limit_ms(self, recognition: bool) -> u64 {
        match self {
            Self::Flac if recognition => 2048,
            Self::Flac => 8021,
            Self::Mp3 => 8016,
        }
    }
    pub const fn priority(self) -> u8 {
        match self {
            Self::Flac => 7,
            Self::Mp3 => 8,
        }
    }
    pub fn validate(self, recognition: bool) -> Result<(), &'static str> {
        if self == Self::Mp3 && recognition {
            Err("MP3 unavailable while recognition is enabled")
        } else {
            Ok(())
        }
    }
}
