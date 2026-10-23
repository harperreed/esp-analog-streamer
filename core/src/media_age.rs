// ABOUTME: Bounds payload age using accepted-source time and rounded startup history.
// ABOUTME: Uses caller-supplied boot-monotonic microseconds and checked deadlines.
use crate::stream_buffer::{BufferPreset, OutputFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeError {
    FutureAcceptance,
    TimeOverflow,
    Stale,
}
#[derive(Debug, Clone, Copy)]
pub struct MediaAge {
    limit_us: u64,
}
impl MediaAge {
    pub fn for_stream(preset: Option<BufferPreset>, format: OutputFormat) -> Self {
        let startup_us = preset.map_or(0, |preset| {
            let samples = (preset.frames(format.block_frames()) * format.block_frames()) as u64;
            (samples * 1_000_000).div_ceil(48_000)
        });
        Self {
            limit_us: startup_us + 4_000_000,
        }
    }
    /// Both deadline and acceptance time use boot-monotonic microseconds.
    pub fn deadline(
        self,
        now_us: u64,
        oldest_us: u64,
        socket_deadline_us: u64,
    ) -> Result<u64, AgeError> {
        Ok(self
            .media_deadline(now_us, oldest_us)?
            .min(socket_deadline_us))
    }
    /// Also call after the final write: a send may return after its deadline.
    pub fn check(self, now_us: u64, oldest_us: u64) -> Result<(), AgeError> {
        self.media_deadline(now_us, oldest_us).map(|_| ())
    }
    fn media_deadline(self, now_us: u64, oldest_us: u64) -> Result<u64, AgeError> {
        if oldest_us > now_us {
            return Err(AgeError::FutureAcceptance);
        }
        let deadline = oldest_us
            .checked_add(self.limit_us)
            .ok_or(AgeError::TimeOverflow)?;
        if now_us >= deadline {
            return Err(AgeError::Stale);
        }
        Ok(deadline)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_payload_expires_at_four_seconds() {
        let age = MediaAge::for_stream(None, OutputFormat::Mp3);
        assert_eq!(age.deadline(10, 0, 9_000_000), Ok(4_000_000));
        assert_eq!(age.check(3_999_999, 0), Ok(()));
        assert_eq!(age.check(4_000_000, 0), Err(AgeError::Stale));
    }
    #[test]
    fn rounded_history_and_socket_deadlines_bound_payload() {
        let age = MediaAge::for_stream(Some(BufferPreset::Medium), OutputFormat::Flac);
        assert_eq!(age.deadline(1, 0, 20_000_000), Ok(8_010_667));
        assert_eq!(age.deadline(1, 0, 2_000_000), Ok(2_000_000));
        assert_eq!(
            MediaAge::for_stream(Some(BufferPreset::High), OutputFormat::Mp3)
                .deadline(1, 0, 20_000_000),
            Ok(12_016_000)
        );
        assert_eq!(
            MediaAge::for_stream(Some(BufferPreset::Low), OutputFormat::Mp3)
                .deadline(1, 0, 20_000_000),
            Ok(5_008_000)
        );
    }
    #[test]
    fn invalid_and_overflowing_acceptance_are_rejected() {
        let age = MediaAge::for_stream(None, OutputFormat::Flac);
        assert_eq!(age.deadline(1, 2, 5), Err(AgeError::FutureAcceptance));
        assert_eq!(
            age.deadline(u64::MAX, u64::MAX, u64::MAX),
            Err(AgeError::TimeOverflow)
        );
        assert_eq!(age.check(9, 10), Err(AgeError::FutureAcceptance));
        assert_eq!(age.check(u64::MAX, u64::MAX), Err(AgeError::TimeOverflow));
    }
}
