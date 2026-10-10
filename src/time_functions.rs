//! Time support for Grule's time built-ins: `MakeTime` with Go's normalization of
//! out-of-range values, and `TimeFormat` with Go's reference-time layouts
//! (`"2006-01-02T15:04:05Z07:00"`).

use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, Months, NaiveDate, Offset, TimeZone, Timelike,
};

pub fn now() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

/// Builds a time in the local time zone, like Go's `time.Date`: values outside their
/// usual range carry over, so month 13 is January of the next year and day 0 is the
/// last day of the previous month.
pub fn make_time(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
) -> Result<DateTime<FixedOffset>, String> {
    let invalid = || "time out of range".to_string();
    let year = i32::try_from(year).map_err(|_| invalid())?;
    let start = NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(invalid)?;

    let months = month - 1;
    let date = if months >= 0 {
        start.checked_add_months(Months::new(u32::try_from(months).map_err(|_| invalid())?))
    } else {
        start.checked_sub_months(Months::new(u32::try_from(-months).map_err(|_| invalid())?))
    }
    .ok_or_else(invalid)?;

    let naive = date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .checked_add_signed(Duration::try_days(day - 1).ok_or_else(invalid)?)
        .and_then(|t| t.checked_add_signed(Duration::try_hours(hour)?))
        .and_then(|t| t.checked_add_signed(Duration::try_minutes(minute)?))
        .and_then(|t| t.checked_add_signed(Duration::try_seconds(second)?))
        .ok_or_else(invalid)?;

    // A local time skipped by a daylight-saving change has no mapping; use the
    // instant just after the gap
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| {
            Local
                .from_local_datetime(&(naive + Duration::hours(1)))
                .earliest()
        })
        .map(|t| t.fixed_offset())
        .ok_or_else(invalid)
}

/// Go's zero time (`time.Time{}`): January 1, year 1, 00:00:00 UTC.
pub fn is_zero(time: &DateTime<FixedOffset>) -> bool {
    time.naive_utc()
        == NaiveDate::from_ymd_opt(1, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
}

/// Layout elements, longest first so that e.g. `2006` wins over `2`.
const TOKENS: &[&str] = &[
    "January",
    "Monday",
    "-07:00:00",
    "Z07:00:00",
    "-070000",
    "Z070000",
    "2006",
    "-07:00",
    "Z07:00",
    "-0700",
    "Z0700",
    "-07",
    "Z07",
    "Jan",
    "Mon",
    "MST",
    "002",
    "__2",
    "_2",
    "01",
    "02",
    "03",
    "04",
    "05",
    "06",
    "15",
    "PM",
    "pm",
    "1",
    "2",
    "3",
    "4",
    "5",
];

/// Formats `time` using a Go layout, where the reference time
/// `Mon Jan 2 15:04:05 MST 2006` shows how each element should look.
pub fn format(time: &DateTime<FixedOffset>, layout: &str) -> String {
    let mut out = String::new();
    let mut rest = layout;
    while !rest.is_empty() {
        if let Some((text, len)) = fractional_seconds(time, rest) {
            out.push_str(&text);
            rest = &rest[len..];
        } else if let Some(token) = TOKENS.iter().find(|t| rest.starts_with(**t)) {
            out.push_str(&format_token(time, token));
            rest = &rest[token.len()..];
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

fn format_token(time: &DateTime<FixedOffset>, token: &str) -> String {
    let offset = time.offset().fix().local_minus_utc();
    match token {
        "January" => time.format("%B").to_string(),
        "Jan" => time.format("%b").to_string(),
        "Monday" => time.format("%A").to_string(),
        "Mon" => time.format("%a").to_string(),
        "MST" => time.format("%:z").to_string(),
        "2006" => format!("{:04}", time.year()),
        "06" => format!("{:02}", time.year().rem_euclid(100)),
        "01" => format!("{:02}", time.month()),
        "1" => time.month().to_string(),
        "02" => format!("{:02}", time.day()),
        "_2" => format!("{:>2}", time.day()),
        "2" => time.day().to_string(),
        "002" => format!("{:03}", time.ordinal()),
        "__2" => format!("{:>3}", time.ordinal()),
        "15" => format!("{:02}", time.hour()),
        "03" => format!("{:02}", time.hour12().1),
        "3" => time.hour12().1.to_string(),
        "04" => format!("{:02}", time.minute()),
        "4" => time.minute().to_string(),
        "05" => format!("{:02}", time.second()),
        "5" => time.second().to_string(),
        "PM" => if time.hour12().0 { "PM" } else { "AM" }.to_string(),
        "pm" => if time.hour12().0 { "pm" } else { "am" }.to_string(),
        zone => format_zone(offset, zone),
    }
}

/// Formats a UTC offset for the `-0700` / `Z07:00` family of layout elements.
/// The `Z` forms print `Z` for UTC.
fn format_zone(offset_seconds: i32, token: &str) -> String {
    if token.starts_with('Z') && offset_seconds == 0 {
        return "Z".to_string();
    }
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let total = offset_seconds.abs();
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    match token.trim_start_matches(['-', 'Z']) {
        "07:00:00" => format!("{}{:02}:{:02}:{:02}", sign, h, m, s),
        "070000" => format!("{}{:02}{:02}{:02}", sign, h, m, s),
        "07:00" => format!("{}{:02}:{:02}", sign, h, m),
        "0700" => format!("{}{:02}{:02}", sign, h, m),
        _ => format!("{}{:02}", sign, h),
    }
}

/// Handles `.000` (fixed digits) and `.999` (trailing zeros trimmed) fractional
/// seconds, with `.` or `,` as the separator. Returns the text and layout length used.
fn fractional_seconds(time: &DateTime<FixedOffset>, layout: &str) -> Option<(String, usize)> {
    let mut chars = layout.chars();
    let separator = chars.next().filter(|c| *c == '.' || *c == ',')?;
    let digit = chars.next().filter(|c| *c == '0' || *c == '9')?;
    let digits = 1 + layout[2..].chars().take_while(|c| *c == digit).count();
    // Go only treats the run as fractional seconds when no other digit follows
    if layout[1 + digits..].starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let digits = digits.min(9);
    let nanos = time.nanosecond() % 1_000_000_000;
    let mut fraction = format!("{:09}", nanos)[..digits].to_string();
    if digit == '9' {
        fraction = fraction.trim_end_matches('0').to_string();
        if fraction.is_empty() {
            return Some((String::new(), 1 + digits));
        }
    }
    Some((format!("{}{}", separator, fraction), 1 + digits))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn formats_go_layouts() {
        let t = at("2023-03-04T05:06:07.120+07:00");
        assert_eq!(
            format(&t, "2006-01-02T15:04:05Z07:00"),
            "2023-03-04T05:06:07+07:00"
        );
        assert_eq!(
            format(&t, "2006-01-02T15:04:05-0700"),
            "2023-03-04T05:06:07+0700"
        );
        assert_eq!(
            format(&t, "Mon Jan _2 3:4:5 PM 06"),
            "Sat Mar  4 5:6:7 AM 23"
        );
        assert_eq!(
            format(&t, "Monday, January 2 2006"),
            "Saturday, March 4 2023"
        );
        assert_eq!(format(&t, "15:04:05.000"), "05:06:07.120");
        assert_eq!(format(&t, "15:04:05.999"), "05:06:07.12");
        assert_eq!(format(&t, "002"), "063");
        assert_eq!(format(&at("2023-03-04T17:00:00Z"), "3PM Z07:00"), "5PM Z");
    }

    #[test]
    fn make_time_normalizes_like_go() {
        let t = make_time(2020, 13, 1, 0, 0, 0).unwrap();
        assert_eq!((t.year(), t.month(), t.day()), (2021, 1, 1));
        let t = make_time(2021, 3, 0, 0, 0, 0).unwrap();
        assert_eq!((t.year(), t.month(), t.day()), (2021, 2, 28));
        let t = make_time(2021, 1, 1, 25, 61, 0).unwrap();
        assert_eq!((t.day(), t.hour(), t.minute()), (2, 2, 1));
    }
}
