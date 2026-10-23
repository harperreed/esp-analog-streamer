// ABOUTME: Checks bounded boot and build identity serialization with real values.
// ABOUTME: Verifies unknown digests and once-only initialization without device mocks.
#[allow(dead_code)]
#[path = "../../firmware/src/device_metrics_identity.rs"]
mod device_metrics_identity;

use device_metrics_identity::{Hex, Identity};
use std::sync::OnceLock;

#[test]
fn identity_serializes_fixed_lowercase_hex() {
    let token = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 254, 255];
    let identity = Identity::new(token, Some([0xab; 32]));
    assert_eq!(
        serde_json::to_value(identity).unwrap(),
        serde_json::json!({
            "boot_token": "000102030405060708090a0b0c0dfeff",
            "build_elf_sha256": "ab".repeat(32)
        })
    );
    assert_eq!(Hex([0u8; 16]).to_string(), "0".repeat(32));
}

#[test]
fn unknown_digest_is_null() {
    for digest in [None, Some([0; 32]), Some([255; 32])] {
        let identity = Identity::new([7; 16], digest);
        assert_eq!(identity.build_elf_sha256, None);
        assert!(serde_json::to_value(identity).unwrap()["build_elf_sha256"].is_null());
    }
    let mut digest = [0; 32];
    digest[31] = 1;
    assert_eq!(
        Identity::new([7; 16], Some(digest)).build_elf_sha256,
        Some(Hex(digest))
    );
}

#[test]
fn initialization_preserves_original_identity() {
    let cell = OnceLock::new();
    let original = Identity::new([1; 16], Some([2; 32]));
    assert_eq!(cell.get_or_init(|| original), &original);
    assert_eq!(
        cell.get_or_init(|| panic!("identity must not be generated again")),
        &original
    );
}
