// ABOUTME: Parser tests against real Shazam responses captured by scripts/e2e-host.
// ABOUTME: match.json must yield the expected song; miss.json must yield no song.

use esp_shazam_core::shazam::parse_response;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../test/fixtures/shazam/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e} (scripts/e2e-host creates it)"))
}

#[test]
fn real_match_response_yields_the_expected_song() {
    let expected: serde_json::Value =
        serde_json::from_slice(&fixture("match.expected.json")).unwrap();
    let expected_title = expected["title"].as_str().unwrap().to_lowercase();
    let song = parse_response(&fixture("match.json"))
        .expect("parses")
        .expect("is a match");
    assert!(
        song.title.to_lowercase().contains(&expected_title),
        "title {:?}",
        song.title
    );
    assert!(!song.key.is_empty() && !song.artist.is_empty(), "{song:?}");
    assert!(
        song.cover_url
            .as_deref()
            .is_some_and(|u| u.starts_with("https://")),
        "{song:?}"
    );
    // The spec marks track.url "(verify)": this assertion is that check.
    assert!(
        song.shazam_url
            .as_deref()
            .is_some_and(|u| u.starts_with("https://")),
        "{song:?}"
    );
}

#[test]
fn real_miss_response_yields_no_song() {
    assert_eq!(parse_response(&fixture("miss.json")), Ok(None));
}
