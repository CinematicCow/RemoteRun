use time::OffsetDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

/// Fixed-width RFC 3339 (UTC, always 3 subsecond digits) so timestamps sort
/// lexicographically and log lines can be split at the first space.
const TS_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");

/// Current unix timestamp in seconds.
pub fn now_ts() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// Current time as e.g. `2026-08-01T10:15:30.123Z`.
pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(TS_FORMAT)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_is_fixed_width() {
        let ts = now_rfc3339();
        assert_eq!(ts.len(), "2026-08-01T10:15:30.123Z".len());
        assert!(ts.ends_with('Z'));
    }
}
