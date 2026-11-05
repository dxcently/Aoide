//! ISO-8601 UTC time formatting/parsing (pure; hand-rolled civil-days
//! conversion so the offline lock never grows a chrono just to stamp/read one
//! timestamp).
//!
//! `now_iso_utc`/`iso_utc_from_epoch` moved from `graph/session_store.rs`;
//! `parse_iso_utc` (their exact inverse) moved from `conductor/theme.rs`
//! (Phase 3a restructure, docs/architecture/PACKAGE-LAYOUT.md). Both are
//! re-exported at their old paths so every existing caller is untouched.

/// UTC wall-clock now as ISO-8601 `YYYY-MM-DDTHH:MM:SSZ`.
///
/// The same `SystemTime`→epoch-seconds idiom daemon.rs stamps audit records
/// with, formatted for the `startedAt` field the stage shapes carry. The civil
/// date is hand-rolled (Howard Hinnant's `civil_from_days`, the exact inverse of
/// [`parse_iso_utc`], the reader) so the offline lock never grows a chrono just
/// to write one timestamp — and a stamp we write always round-trips back
/// through the reader `conductor/theme` ships.
pub fn now_iso_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    iso_utc_from_epoch(secs)
}

/// Format Unix epoch seconds as ISO-8601 UTC (pure; unit-tested).
pub fn iso_utc_from_epoch(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // civil_from_days: the inverse of parse_iso_utc's days computation.
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// `stamp` moved by `secs` (negative moves back), in the same ISO-8601 UTC
/// shape. An unparsable `stamp` is returned unchanged rather than replaced
/// with a fabricated time, which makes a window built from it close
/// immediately — fail-closed, never widened. Used for the age binding's
/// validity window, where a wrong `now` must never open a window wider than
/// intended.
pub fn shift_iso_utc(stamp: &str, secs: i64) -> String {
    match parse_iso_utc(stamp) {
        Some(epoch) => iso_utc_from_epoch(epoch + secs),
        None => stamp.to_string(),
    }
}

/// Parse `YYYY-MM-DDTHH:MM:SS` (a trailing `Z` tolerated) to UTC epoch seconds,/// or `None` when the shape doesn't hold — a hand-rolled civil-days conversion
/// so the lock never grows a chrono just to subtract two timestamps.
pub fn parse_iso_utc(s: &str) -> Option<i64> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let num = |a: usize, b: usize| -> Option<i64> { s.get(a..b)?.parse().ok() };
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

/// `YYYY-MM-DDTHH:MM:SS`, an optional `.` and 1 to 9 digits, then `Z`, as UTC
/// epoch milliseconds. The fraction is cut to milliseconds. Any other shape,
/// or a clock field out of range, is `None`: an offset such as `+02:00` is
/// refused, never read as UTC.
pub fn parse_iso_utc_ms(s: &str) -> Option<i64> {
    const SHAPE: &[u8] = b"dddd-dd-ddTdd:dd:dd";
    let body = s.strip_suffix('Z')?;
    let (whole, millis) = match body.split_once('.') {
        None => (body, 0),
        Some((whole, digits)) => {
            if !(1..=9).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            (whole, format!("{digits:0<3}")[..3].parse::<i64>().ok()?)
        }
    };
    let shaped = whole.len() == SHAPE.len()
        && whole.bytes().zip(SHAPE.iter()).all(|(b, &want)| match want {
            b'd' => b.is_ascii_digit(),
            _ => b == want,
        });
    if !shaped {
        return None;
    }
    let clock = |at: usize| whole[at..at + 2].parse::<u8>().ok();
    if clock(11)? > 23 || clock(14)? > 59 || clock(17)? > 59 {
        return None;
    }
    Some(parse_iso_utc(whole)? * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "2026-03-01T12:34:56";

    fn base_ms() -> i64 {
        parse_iso_utc(BASE).unwrap() * 1000
    }

    #[test]
    fn a_stamp_without_a_fraction_is_its_whole_second_in_milliseconds() {
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}Z")), Some(base_ms()));
    }

    #[test]
    fn a_fraction_of_one_to_nine_digits_is_cut_to_milliseconds() {
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}.5Z")), Some(base_ms() + 500));
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}.05Z")), Some(base_ms() + 50));
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}.123Z")), Some(base_ms() + 123));
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}.1234Z")), Some(base_ms() + 123));
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}.123456789Z")), Some(base_ms() + 123));
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}.000000001Z")), Some(base_ms()));
    }

    #[test]
    fn an_offset_is_refused_and_never_read_as_utc() {
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}+02:00")), None);
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}-05:00")), None);
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}.5+02:00")), None);
        assert_eq!(parse_iso_utc_ms(&format!("{BASE}+02:00Z")), None);
    }

    #[test]
    fn garbage_and_a_stamp_without_its_z_are_refused() {
        for bad in [
            "",
            "not a time",
            "2026-03-01T12:34:56",
            "2026-03-01T12:34:56.5",
            "2026-03-01T12:34:56.Z",
            "2026-03-01T12:34:56.1234567890Z",
            "2026-03-01T12:34:56.12x Z",
            "2026-03-01 12:34:56Z",
            "2026-03-01T25:00:00Z",
            "2026-03-01T12:60:00Z",
            "2026-03-01T+1:34:56Z",
        ] {
            assert_eq!(parse_iso_utc_ms(bad), None, "{bad:?} must be refused");
        }
    }
}
