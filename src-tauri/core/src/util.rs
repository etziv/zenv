use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Compact UTC timestamp safe for file names, e.g. `20261016T104455Z`.
pub fn file_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub fn relative_to_now(iso: &str) -> String {
    relative_time(iso, OffsetDateTime::now_utc())
}

pub fn seconds_ago(iso: &str) -> Option<i64> {
    OffsetDateTime::parse(iso, &Rfc3339).ok().map(|then| (OffsetDateTime::now_utc() - then).whole_seconds())
}

pub fn relative_time(iso: &str, now: OffsetDateTime) -> String {
    let Ok(then) = OffsetDateTime::parse(iso, &Rfc3339) else { return iso.to_owned() };
    let seconds = (now - then).whole_seconds();
    match seconds {
        ..5 => "just now".to_owned(),
        5..60 => format!("{seconds}s ago"),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86400 => format!("{}h ago", seconds / 3600),
        _ => {
            let local = then.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
            format!("{:04}-{:02}-{:02} {:02}:{:02}", local.year(), u8::from(local.month()), local.day(), local.hour(), local.minute())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_uses_short_units_and_keeps_bad_input() {
        let now = OffsetDateTime::parse("2026-10-16T12:00:00Z", &Rfc3339).unwrap();
        assert_eq!(relative_time("2026-10-16T11:59:58Z", now), "just now");
        assert_eq!(relative_time("2026-10-16T11:59:30Z", now), "30s ago");
        assert_eq!(relative_time("2026-10-16T11:45:00Z", now), "15m ago");
        assert_eq!(relative_time("2026-10-16T09:00:00Z", now), "3h ago");
        assert_eq!(relative_time("not a date", now), "not a date");
        assert!(relative_time("2026-10-14T09:00:00Z", now).starts_with("2026-10-1"));
    }
}
