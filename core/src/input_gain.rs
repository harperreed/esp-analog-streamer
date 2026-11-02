// ABOUTME: Input gain for the ES8388 line input: whole 3 dB steps from 0 to +24 dB, the PGA register byte they map
// ABOUTME: to, and the settings page's request and status bodies. Pure; the firmware applies and persists a gain.
/// The ES8388's ADC PGA moves in 3 dB steps per channel nibble of register 0x09; both channels stay equal here.
pub const STEP_DB: u8 = 3;
pub const MAX_DB: u8 = 24;
/// `{"gain_db": 24}` with room for whitespace; anything longer is not a gain request.
pub const MAX_BODY_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputGain {
    steps: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GainError {
    /// Not a multiple of STEP_DB.
    NotAStep(u8),
    /// Above MAX_DB.
    OutOfRange(u8),
    /// Not a `{"gain_db": N}` object with a non-negative integer.
    Malformed,
}

impl std::fmt::Display for GainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GainError::NotAStep(db) => write!(f, "gain {db} dB is not a multiple of {STEP_DB} dB"),
            GainError::OutOfRange(db) => write!(f, "gain {db} dB is above {MAX_DB} dB"),
            GainError::Malformed => {
                write!(f, "expected {{\"gain_db\": N}} with N from 0 to {MAX_DB}")
            }
        }
    }
}

impl InputGain {
    pub const ZERO: InputGain = InputGain { steps: 0 };

    pub fn from_db(db: u8) -> Result<Self, GainError> {
        if db > MAX_DB {
            return Err(GainError::OutOfRange(db));
        }
        if !db.is_multiple_of(STEP_DB) {
            return Err(GainError::NotAStep(db));
        }
        Ok(InputGain {
            steps: db / STEP_DB,
        })
    }

    pub fn db(self) -> u8 {
        self.steps * STEP_DB
    }

    /// Register 0x09: left PGA in the high nibble, right in the low, 3 dB per step.
    pub fn register(self) -> u8 {
        (self.steps << 4) | self.steps
    }

    /// The gain a register byte encodes, when both channels agree and the step is in range.
    pub fn from_register(register: u8) -> Option<Self> {
        let (left, right) = (register >> 4, register & 0x0F);
        (left == right && left <= MAX_DB / STEP_DB).then_some(InputGain { steps: left })
    }
}

/// Parses the settings page's body. Any shape other than an object with the one non-negative integer field
/// is malformed; serde would also accept a one-element array for a struct, so the object is checked by hand.
pub fn parse_request(body: &[u8]) -> Result<InputGain, GainError> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| GainError::Malformed)?;
    let object = value.as_object().ok_or(GainError::Malformed)?;
    if object.len() != 1 {
        return Err(GainError::Malformed);
    }
    let db = object
        .get("gain_db")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GainError::Malformed)?;
    let db = u8::try_from(db).map_err(|_| GainError::OutOfRange(u8::MAX))?;
    InputGain::from_db(db)
}

pub fn status_json(gain: InputGain) -> String {
    format!(
        "{{\"gain_db\":{},\"max_db\":{MAX_DB},\"step_db\":{STEP_DB}}}",
        gain.db()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_map_to_both_pga_nibbles() {
        assert_eq!(InputGain::from_db(0).unwrap().register(), 0x00);
        assert_eq!(InputGain::from_db(3).unwrap().register(), 0x11);
        assert_eq!(InputGain::from_db(12).unwrap().register(), 0x44);
        assert_eq!(InputGain::from_db(24).unwrap().register(), 0x88);
        assert_eq!(InputGain::ZERO.db(), 0);
        for db in (0..=24).step_by(3) {
            let gain = InputGain::from_db(db).unwrap();
            assert_eq!(gain.db(), db);
            assert_eq!(InputGain::from_register(gain.register()), Some(gain));
        }
    }

    #[test]
    fn off_step_and_out_of_range_are_refused() {
        assert_eq!(InputGain::from_db(1), Err(GainError::NotAStep(1)));
        assert_eq!(InputGain::from_db(25), Err(GainError::OutOfRange(25)));
        assert_eq!(InputGain::from_db(27), Err(GainError::OutOfRange(27)));
        assert_eq!(InputGain::from_db(255), Err(GainError::OutOfRange(255)));
    }

    #[test]
    fn registers_with_unequal_channels_or_a_step_past_24_db_are_not_a_gain() {
        assert_eq!(InputGain::from_register(0x80), None);
        assert_eq!(InputGain::from_register(0x99), None);
        assert_eq!(InputGain::from_register(0xFF), None);
        assert_eq!(InputGain::from_register(0x22).map(InputGain::db), Some(6));
    }

    #[test]
    fn request_body_is_one_integer_field() {
        assert_eq!(
            parse_request(br#"{"gain_db": 12}"#).map(InputGain::db),
            Ok(12)
        );
        assert_eq!(parse_request(b" {\"gain_db\":0} "), Ok(InputGain::ZERO));
        assert_eq!(
            parse_request(br#"{"gain_db": 13}"#),
            Err(GainError::NotAStep(13))
        );
        assert_eq!(
            parse_request(br#"{"gain_db": 30}"#),
            Err(GainError::OutOfRange(30))
        );
        assert_eq!(
            parse_request(br#"{"gain_db": 300}"#),
            Err(GainError::OutOfRange(255))
        );
        for body in [
            &br#"{"gain_db": -3}"#[..],
            br#"{"gain_db": "12"}"#,
            br#"{"gain_db": 12.0}"#,
            br#"{"gain_db": 12, "other": 1}"#,
            br#"{"gain": 12}"#,
            br#"[12]"#,
            b"",
            b"12",
        ] {
            assert_eq!(
                parse_request(body),
                Err(GainError::Malformed),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn status_names_the_bounds() {
        assert_eq!(
            status_json(InputGain::from_db(9).unwrap()),
            r#"{"gain_db":9,"max_db":24,"step_db":3}"#
        );
        let value: serde_json::Value = serde_json::from_str(&status_json(InputGain::ZERO)).unwrap();
        assert_eq!(value["gain_db"], 0);
    }

    #[test]
    fn errors_read_as_plain_sentences() {
        assert_eq!(
            GainError::NotAStep(13).to_string(),
            "gain 13 dB is not a multiple of 3 dB"
        );
        assert_eq!(
            GainError::OutOfRange(30).to_string(),
            "gain 30 dB is above 24 dB"
        );
        assert!(GainError::Malformed.to_string().contains("gain_db"));
    }
}
