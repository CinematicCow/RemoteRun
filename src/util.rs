use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Current unix timestamp in seconds.
pub fn now_ts() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// Current time as an RFC 3339 string with millisecond precision,
/// e.g. `2026-08-01T10:15:30.123Z`.
pub fn now_rfc3339() -> String {
    let now = OffsetDateTime::now_utc();
    now.replace_nanosecond(now.nanosecond() / 1_000_000 * 1_000_000)
        .unwrap_or(now)
        .format(&Rfc3339)
        .unwrap_or_default()
}
