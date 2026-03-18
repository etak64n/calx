use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::{NaiveDateTime, TimeZone};
use serde::Serialize;
use std::io::Read;

#[derive(Serialize)]
struct ImportResult {
    imported: usize,
}

/// Validates input first, then creates store only if needed.
pub fn run(file: &str, format: OutputFormat) -> Result<(), AppError> {
    let content = if file == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| AppError::EventKit(format!("Failed to read stdin: {e}")))?;
        buf
    } else {
        std::fs::read_to_string(file)
            .map_err(|e| AppError::EventKit(format!("Failed to read file: {e}")))?
    };

    let is_ics = if file == "-" {
        content.trim_start().starts_with("BEGIN:VCALENDAR")
    } else if file.ends_with(".ics") {
        true
    } else if file.ends_with(".csv") {
        false
    } else {
        return Err(AppError::EventKit(
            "Unknown file format. Use .ics or .csv, or pipe via stdin.".to_string(),
        ));
    };

    let store = CalendarStore::new()?;

    let count = if is_ics {
        import_ics(&store, &content)?
    } else {
        import_csv(&store, &content)?
    };

    let result = ImportResult { imported: count };
    print_output(format, &result, |r| {
        println!("{} event(s) imported.", r.imported);
    });
    Ok(())
}

/// Unfold ICS content lines per RFC 5545 section 3.1.
/// Lines starting with a space or tab are continuations of the previous line.
fn unfold_ics(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in content.lines() {
        if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
            // Continuation: append to previous line (strip leading whitespace)
            if let Some(last) = lines.last_mut() {
                let cont: &mut String = last;
                cont.push_str(raw_line[1..].trim_end());
            }
        } else {
            lines.push(raw_line.trim_end().to_string());
        }
    }
    lines
}

/// Unescape RFC 5545 TEXT values (section 3.3.11).
fn ics_unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => result.push('\n'),
                Some('\\') => result.push('\\'),
                Some(';') => result.push(';'),
                Some(',') => result.push(','),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn import_ics(store: &CalendarStore, content: &str) -> Result<usize, AppError> {
    let lines = unfold_ics(content);
    let mut count = 0;
    let mut title = String::new();
    let mut start_raw = String::new();
    let mut end_raw = String::new();
    let mut start_tzid: Option<String> = None;
    let mut end_tzid: Option<String> = None;
    let mut notes = String::new();
    let mut is_all_day = false;
    let mut in_event = false;

    for line in &lines {
        match line.as_str() {
            "BEGIN:VEVENT" => {
                in_event = true;
                title.clear();
                start_raw.clear();
                end_raw.clear();
                start_tzid = None;
                end_tzid = None;
                notes.clear();
                is_all_day = false;
            }
            "END:VEVENT" if in_event => {
                in_event = false;
                if !title.is_empty() && !start_raw.is_empty() && !end_raw.is_empty() {
                    let start_dt = parse_ics_datetime_with_tz(&start_raw, start_tzid.as_deref())
                        .ok_or_else(|| AppError::InvalidDate(start_raw.clone()))?;
                    let end_dt = parse_ics_datetime_with_tz(&end_raw, end_tzid.as_deref())
                        .ok_or_else(|| AppError::InvalidDate(end_raw.clone()))?;
                    let notes_opt = if notes.is_empty() {
                        None
                    } else {
                        Some(ics_unescape(&notes))
                    };
                    let title_unescaped = ics_unescape(&title);
                    store.add_event(
                        &title_unescaped,
                        start_dt,
                        end_dt,
                        None,
                        None,
                        None,
                        notes_opt.as_deref(),
                        is_all_day,
                        None,
                        None,
                    )?;
                    count += 1;
                }
            }
            _ if in_event => {
                if let Some(v) = line.strip_prefix("SUMMARY:") {
                    title = v.to_string();
                } else if let Some(v) = line.strip_prefix("DESCRIPTION:") {
                    notes = v.to_string();
                } else if line.starts_with("DTSTART") {
                    let parsed = parse_ics_dt_line(line);
                    start_raw = parsed.value;
                    start_tzid = parsed.tzid;
                    if parsed.all_day {
                        is_all_day = true;
                    }
                } else if line.starts_with("DTEND") {
                    let parsed = parse_ics_dt_line(line);
                    end_raw = parsed.value;
                    end_tzid = parsed.tzid;
                }
            }
            _ => {}
        }
    }
    Ok(count)
}

struct IcsDtParsed {
    value: String,
    tzid: Option<String>,
    all_day: bool,
}

/// Parse a DTSTART or DTEND line, extracting value, TZID, and VALUE=DATE flag.
fn parse_ics_dt_line(line: &str) -> IcsDtParsed {
    let all_day = line.contains("VALUE=DATE");

    // Extract TZID if present: DTSTART;TZID=America/New_York:20260320T090000
    let tzid = if let Some(tzid_start) = line.find("TZID=") {
        let after = &line[tzid_start + 5..];
        let end = after.find(':').unwrap_or(after.len());
        Some(after[..end].to_string())
    } else {
        None
    };

    // Value is everything after the last ':'
    let value = line.rsplit(':').next().unwrap_or("").to_string();

    IcsDtParsed {
        value,
        tzid,
        all_day,
    }
}

/// Parse ICS datetime with optional timezone.
pub(crate) fn parse_ics_datetime_with_tz(s: &str, tzid: Option<&str>) -> Option<NaiveDateTime> {
    if s.ends_with('Z') {
        // UTC
        let s = s.trim_end_matches('Z');
        let utc_dt = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
        let utc = chrono::Utc.from_utc_datetime(&utc_dt);
        Some(utc.with_timezone(&chrono::Local).naive_local())
    } else if let Some(tz_name) = tzid {
        // Known timezone: apply UTC offset
        let naive = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
        let offset_secs = tz_name_to_offset_secs(tz_name)?;
        let fixed = chrono::FixedOffset::east_opt(offset_secs)?;
        let dt = fixed.from_local_datetime(&naive).earliest()?;
        Some(dt.with_timezone(&chrono::Local).naive_local())
    } else {
        // Local time or date-only
        NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S")
            .or_else(|_| {
                chrono::NaiveDate::parse_from_str(s, "%Y%m%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            })
            .ok()
    }
}

/// Wrapper for parsing without TZID (used in tests).
#[cfg(test)]
fn parse_ics_datetime(s: &str) -> Option<NaiveDateTime> {
    parse_ics_datetime_with_tz(s, None)
}

/// Map common IANA timezone names to UTC offsets in seconds.
/// This avoids requiring chrono-tz as a dependency.
fn tz_name_to_offset_secs(name: &str) -> Option<i32> {
    // Standard offsets (non-DST). For DST-aware conversion,
    // chrono-tz would be needed. This covers the most common cases.
    Some(match name {
        "UTC" | "GMT" => 0,
        "US/Eastern" | "America/New_York" => -5 * 3600,
        "US/Central" | "America/Chicago" => -6 * 3600,
        "US/Mountain" | "America/Denver" => -7 * 3600,
        "US/Pacific" | "America/Los_Angeles" => -8 * 3600,
        "Europe/London" => 0,
        "Europe/Paris" | "Europe/Berlin" | "Europe/Amsterdam" => 3600,
        "Europe/Helsinki" | "Europe/Athens" => 2 * 3600,
        "Europe/Moscow" => 3 * 3600,
        "Asia/Dubai" => 4 * 3600,
        "Asia/Kolkata" | "Asia/Calcutta" => 5 * 3600 + 1800,
        "Asia/Bangkok" | "Asia/Jakarta" => 7 * 3600,
        "Asia/Shanghai" | "Asia/Hong_Kong" | "Asia/Singapore" => 8 * 3600,
        "Asia/Tokyo" => 9 * 3600,
        "Australia/Sydney" => 10 * 3600,
        "Pacific/Auckland" => 12 * 3600,
        _ => return None,
    })
}

fn import_csv(store: &CalendarStore, content: &str) -> Result<usize, AppError> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let headers = rdr
        .headers()
        .map_err(|e| AppError::EventKit(e.to_string()))?
        .clone();

    let idx = |name: &str| headers.iter().position(|h| h == name);
    let title_i = idx("title")
        .ok_or_else(|| AppError::EventKit("CSV missing 'title' column header".to_string()))?;
    let start_i = idx("start")
        .ok_or_else(|| AppError::EventKit("CSV missing 'start' column header".to_string()))?;
    let end_i = idx("end")
        .ok_or_else(|| AppError::EventKit("CSV missing 'end' column header".to_string()))?;
    let notes_i = idx("notes");
    let all_day_i = idx("all_day");
    let location_i = idx("location");
    let url_i = idx("url");
    let calendar_i = idx("calendar");

    let mut count = 0;
    for result in rdr.records() {
        let record = result.map_err(|e| AppError::EventKit(e.to_string()))?;
        let title = record.get(title_i).unwrap_or_default();
        let start_str = record.get(start_i).unwrap_or_default();
        let end_str = record.get(end_i).unwrap_or_default();
        let notes = notes_i
            .and_then(|i| record.get(i))
            .filter(|s| !s.is_empty());
        let all_day = all_day_i
            .and_then(|i| record.get(i))
            .is_some_and(|v| v == "true");
        let location = location_i
            .and_then(|i| record.get(i))
            .filter(|s| !s.is_empty());
        let url = url_i.and_then(|i| record.get(i)).filter(|s| !s.is_empty());
        let calendar = calendar_i
            .and_then(|i| record.get(i))
            .filter(|s| !s.is_empty());

        let start_dt = parse_csv_datetime(start_str)
            .ok_or_else(|| AppError::InvalidDate(start_str.to_string()))?;
        let end_dt = parse_csv_datetime(end_str)
            .ok_or_else(|| AppError::InvalidDate(end_str.to_string()))?;

        store.add_event(
            title, start_dt, end_dt, calendar, location, url, notes, all_day, None, None,
        )?;
        count += 1;
    }
    Ok(count)
}

pub(crate) fn parse_csv_datetime(s: &str) -> Option<NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.naive_local())
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok())
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    // --- ICS datetime parsing ---

    #[test]
    fn test_ics_datetime_local() {
        let dt = parse_ics_datetime("20260320T140000").unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 20);
        assert_eq!(dt.hour(), 14);
    }

    #[test]
    fn test_ics_datetime_utc_converts_to_local() {
        let dt = parse_ics_datetime("20260320T140000Z").unwrap();
        let expected_utc = chrono::NaiveDate::from_ymd_opt(2026, 3, 20)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        let expected_local = chrono::Utc
            .from_utc_datetime(&expected_utc)
            .with_timezone(&chrono::Local)
            .naive_local();
        assert_eq!(dt, expected_local);
    }

    #[test]
    fn test_ics_datetime_tzid_converts() {
        // 09:00 America/New_York (UTC-5) = 14:00 UTC
        let dt = parse_ics_datetime_with_tz("20260320T090000", Some("America/New_York")).unwrap();
        let utc_dt = chrono::NaiveDate::from_ymd_opt(2026, 3, 20)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        let expected = chrono::Utc
            .from_utc_datetime(&utc_dt)
            .with_timezone(&chrono::Local)
            .naive_local();
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_ics_datetime_tzid_tokyo() {
        // 14:00 Asia/Tokyo (UTC+9) = 05:00 UTC
        let dt = parse_ics_datetime_with_tz("20260320T140000", Some("Asia/Tokyo")).unwrap();
        let utc_dt = chrono::NaiveDate::from_ymd_opt(2026, 3, 20)
            .unwrap()
            .and_hms_opt(5, 0, 0)
            .unwrap();
        let expected = chrono::Utc
            .from_utc_datetime(&utc_dt)
            .with_timezone(&chrono::Local)
            .naive_local();
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_ics_datetime_date_only() {
        let dt = parse_ics_datetime("20260320").unwrap();
        assert_eq!(dt.day(), 20);
        assert_eq!(dt.hour(), 0);
    }

    #[test]
    fn test_ics_datetime_invalid() {
        assert!(parse_ics_datetime("not-a-date").is_none());
        assert!(parse_ics_datetime("").is_none());
    }

    // --- ICS dt line parsing ---

    #[test]
    fn test_ics_dt_line_basic() {
        let p = parse_ics_dt_line("DTSTART:20260320T140000");
        assert_eq!(p.value, "20260320T140000");
        assert!(p.tzid.is_none());
        assert!(!p.all_day);
    }

    #[test]
    fn test_ics_dt_line_utc() {
        let p = parse_ics_dt_line("DTSTART:20260320T140000Z");
        assert_eq!(p.value, "20260320T140000Z");
        assert!(p.tzid.is_none());
    }

    #[test]
    fn test_ics_dt_line_value_date() {
        let p = parse_ics_dt_line("DTSTART;VALUE=DATE:20260320");
        assert_eq!(p.value, "20260320");
        assert!(p.all_day);
    }

    #[test]
    fn test_ics_dt_line_tzid() {
        let p = parse_ics_dt_line("DTSTART;TZID=Asia/Tokyo:20260320T140000");
        assert_eq!(p.value, "20260320T140000");
        assert_eq!(p.tzid.as_deref(), Some("Asia/Tokyo"));
        assert!(!p.all_day);
    }

    #[test]
    fn test_ics_dt_line_tzid_ny() {
        let p = parse_ics_dt_line("DTSTART;TZID=America/New_York:20260320T090000");
        assert_eq!(p.value, "20260320T090000");
        assert_eq!(p.tzid.as_deref(), Some("America/New_York"));
    }

    // --- ICS text escaping round-trip ---

    #[test]
    fn test_ics_unescape_basic() {
        assert_eq!(ics_unescape("hello\\, world"), "hello, world");
        assert_eq!(ics_unescape("a\\;b\\\\c"), "a;b\\c");
        assert_eq!(ics_unescape("line1\\nline2"), "line1\nline2");
        assert_eq!(ics_unescape("no escapes"), "no escapes");
    }

    #[test]
    fn test_ics_escape_unescape_roundtrip() {
        let original = "Meeting, with; special\\chars\nand newlines";
        let escaped = crate::output::ics_escape(original);
        let unescaped = ics_unescape(&escaped);
        assert_eq!(unescaped, original);
    }

    // --- ICS line folding ---

    #[test]
    fn test_unfold_ics() {
        let input = "SUMMARY:This is a long\r\n title that wraps";
        // After unfolding: "SUMMARY:This is a longtitle that wraps"
        let lines = unfold_ics(input);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("SUMMARY:This is a long"));
        assert!(lines[0].contains("title that wraps"));
    }

    #[test]
    fn test_unfold_ics_tab() {
        let input = "DESCRIPTION:line1\n\tcontinued";
        let lines = unfold_ics(input);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("line1"));
        assert!(lines[0].contains("continued"));
    }

    #[test]
    fn test_unfold_ics_no_folding() {
        let input = "SUMMARY:Short\nDTSTART:20260320T140000";
        let lines = unfold_ics(input);
        assert_eq!(lines.len(), 2);
    }

    // --- CSV datetime parsing ---

    #[test]
    fn test_csv_datetime_rfc3339() {
        let dt = parse_csv_datetime("2026-03-18T11:00:00+09:00").unwrap();
        assert_eq!(dt.hour(), 11);
        assert_eq!(dt.day(), 18);
    }

    #[test]
    fn test_csv_datetime_simple() {
        let dt = parse_csv_datetime("2026-03-20 14:00").unwrap();
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.day(), 20);
    }

    #[test]
    fn test_csv_datetime_date_only() {
        let dt = parse_csv_datetime("2026-03-20").unwrap();
        assert_eq!(dt.hour(), 0);
    }

    #[test]
    fn test_csv_datetime_invalid() {
        assert!(parse_csv_datetime("garbage").is_none());
    }
}
