// ABOUTME: Exercises production radio readback validation without pretending to run ESP-IDF.
// ABOUTME: Rejects stale diagnostic modes and enabled modem sleep before reporting a baseline.

#[path = "../../firmware/src/net_policy.rs"]
mod net_policy;

use net_policy::{PolicyMismatch, RadioPolicy};

const STREAMING: RadioPolicy = RadioPolicy {
    protocol: 7,
    power_save: 0,
};

#[test]
fn accepts_the_requested_streaming_radio_readback() {
    assert_eq!(STREAMING.verify(STREAMING), Ok(()));
}

#[test]
fn rejects_persisted_bg_diagnostic_mode() {
    let effective = RadioPolicy {
        protocol: 3,
        power_save: 0,
    };
    assert_eq!(
        STREAMING.verify(effective),
        Err(PolicyMismatch {
            expected: STREAMING,
            effective,
        })
    );
}

#[test]
fn rejects_extra_protocols_instead_of_treating_bgn_as_a_subset() {
    let effective = RadioPolicy {
        protocol: 15,
        power_save: 0,
    };
    assert!(STREAMING.verify(effective).is_err());
}

#[test]
fn rejects_modem_sleep_even_with_the_requested_protocols() {
    for power_save in [1, 2] {
        let effective = RadioPolicy {
            protocol: 7,
            power_save,
        };
        assert_eq!(
            STREAMING.verify(effective),
            Err(PolicyMismatch {
                expected: STREAMING,
                effective,
            })
        );
    }
}

#[test]
fn reports_both_effective_fields_when_both_differ() {
    let effective = RadioPolicy {
        protocol: 3,
        power_save: 1,
    };
    let mismatch = STREAMING.verify(effective).unwrap_err();
    assert_eq!(mismatch.expected, STREAMING);
    assert_eq!(mismatch.effective, effective);
}
