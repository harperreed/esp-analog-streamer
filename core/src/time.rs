// ABOUTME: Formats Unix time as RFC 3339 UTC without pulling in a date library.
// ABOUTME: Used for `identified_at` and `rate_limited_until` in /api/state.

pub fn rfc3339_utc(unix_secs: u64) -> String {
    let (year, month, day) = civil_from_days(unix_secs / 86_400);
    let secs_of_day = unix_secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3_600,
        secs_of_day / 60 % 60,
        secs_of_day % 60
    )
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's
/// `civil_from_days` (public domain), restricted to dates from 1970 on.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097; // day of era, [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // year of era, [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year counted from March 1
    let mp = (5 * doy + 2) / 153; // month counted from March, [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::rfc3339_utc;

    // Expected values computed with Python's datetime (UTC).
    #[test]
    fn formats_known_instants() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_791_062_521), "2026-10-03T21:22:01Z");
        assert_eq!(rfc3339_utc(4_102_444_800), "2100-01-01T00:00:00Z");
    }
}
