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

    // First pass: collect VTIMEZONE TZID mappings for non-IANA IDs
    let tz_map = build_vtimezone_map(&lines);

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
                    let s_tzid = resolve_tzid(start_tzid.as_deref(), &tz_map);
                    let e_tzid = resolve_tzid(end_tzid.as_deref(), &tz_map);
                    let start_dt = parse_ics_datetime_with_tz(&start_raw, s_tzid.as_deref())
                        .ok_or_else(|| AppError::InvalidDate(start_raw.clone()))?;
                    let end_dt = parse_ics_datetime_with_tz(&end_raw, e_tzid.as_deref())
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
                if line.starts_with("SUMMARY") {
                    title = extract_ics_value(line);
                } else if line.starts_with("DESCRIPTION") {
                    notes = extract_ics_value(line);
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
    // Stop at ';' or ':' (whichever comes first) to handle extra params like ;X-FOO=bar
    let tzid = if let Some(tzid_start) = line.find("TZID=") {
        let after = &line[tzid_start + 5..];
        let end = after.find([':', ';']).unwrap_or(after.len());
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

/// Extract the value from an ICS property line, handling optional parameters.
/// e.g. "SUMMARY;LANGUAGE=en:Meeting" -> "Meeting"
/// e.g. "SUMMARY:Meeting" -> "Meeting"
fn extract_ics_value(line: &str) -> String {
    // Value is everything after the first ':' that follows the property name
    if let Some(colon_pos) = line.find(':') {
        line[colon_pos + 1..].to_string()
    } else {
        String::new()
    }
}

/// Build a mapping from custom/Windows TZID to IANA name by reading VTIMEZONE blocks.
fn build_vtimezone_map(lines: &[String]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut in_vtimezone = false;
    let mut current_tzid = String::new();
    let mut x_lic_location = String::new();

    for line in lines {
        match line.as_str() {
            "BEGIN:VTIMEZONE" => {
                in_vtimezone = true;
                current_tzid.clear();
                x_lic_location.clear();
            }
            "END:VTIMEZONE" => {
                in_vtimezone = false;
                if !current_tzid.is_empty() {
                    // If the TZID isn't already a valid IANA name, try mapping it
                    if current_tzid.parse::<chrono_tz::Tz>().is_err() {
                        // Try X-LIC-LOCATION first (common in Outlook exports)
                        if !x_lic_location.is_empty()
                            && x_lic_location.parse::<chrono_tz::Tz>().is_ok()
                        {
                            map.insert(current_tzid.clone(), x_lic_location.clone());
                        }
                        // Try Windows timezone mapping
                        if !map.contains_key(&current_tzid) {
                            if let Some(iana) = windows_tz_to_iana(&current_tzid) {
                                map.insert(current_tzid.clone(), iana.to_string());
                            }
                        }
                    }
                }
            }
            _ if in_vtimezone => {
                if line.starts_with("TZID") {
                    current_tzid = extract_ics_value(line);
                } else if line.starts_with("X-LIC-LOCATION") {
                    x_lic_location = extract_ics_value(line);
                }
            }
            _ => {}
        }
    }
    map
}

/// Resolve a TZID: try the raw name first (IANA), then check the VTIMEZONE map,
/// then try Windows TZ mapping as a last resort.
fn resolve_tzid(
    tzid: Option<&str>,
    tz_map: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let name = tzid?;
    // Already valid IANA?
    if name.parse::<chrono_tz::Tz>().is_ok() {
        return Some(name.to_string());
    }
    // Check VTIMEZONE mapping
    if let Some(mapped) = tz_map.get(name) {
        return Some(mapped.clone());
    }
    // Try Windows TZ name
    if let Some(iana) = windows_tz_to_iana(name) {
        return Some(iana.to_string());
    }
    // Return as-is (will fail at chrono-tz parse, treated as local)
    Some(name.to_string())
}

/// Map common Windows timezone names to IANA names.
fn windows_tz_to_iana(name: &str) -> Option<&'static str> {
    Some(match name {
        "Eastern Standard Time" => "America/New_York",
        "Central Standard Time" => "America/Chicago",
        "Mountain Standard Time" => "America/Denver",
        "Pacific Standard Time" => "America/Los_Angeles",
        "GMT Standard Time" => "Europe/London",
        "W. Europe Standard Time" => "Europe/Berlin",
        "Romance Standard Time" => "Europe/Paris",
        "Russian Standard Time" => "Europe/Moscow",
        "China Standard Time" => "Asia/Shanghai",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "Korea Standard Time" => "Asia/Seoul",
        "India Standard Time" => "Asia/Kolkata",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "New Zealand Standard Time" => "Pacific/Auckland",
        "Singapore Standard Time" => "Asia/Singapore",
        "SE Asia Standard Time" => "Asia/Bangkok",
        "Arabian Standard Time" => "Asia/Dubai",
        "UTC" => "UTC",
        _ => return None,
    })
}

/// Parse ICS datetime with optional IANA timezone (DST-aware via chrono-tz).
pub(crate) fn parse_ics_datetime_with_tz(s: &str, tzid: Option<&str>) -> Option<NaiveDateTime> {
    if s.ends_with('Z') {
        // UTC
        let s = s.trim_end_matches('Z');
        let utc_dt = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
        let utc = chrono::Utc.from_utc_datetime(&utc_dt);
        Some(utc.with_timezone(&chrono::Local).naive_local())
    } else if let Some(tz_name) = tzid {
        // IANA timezone: DST-aware conversion via chrono-tz
        let naive = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
        let tz: chrono_tz::Tz = tz_name.parse().ok()?;
        let dt = tz.from_local_datetime(&naive).earliest()?;
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
    fn test_ics_datetime_tzid_ny_dst() {
        // 2026-03-20: New York is in EDT (UTC-4)
        // 09:00 EDT = 13:00 UTC
        let dt = parse_ics_datetime_with_tz("20260320T090000", Some("America/New_York")).unwrap();
        let utc_dt = chrono::NaiveDate::from_ymd_opt(2026, 3, 20)
            .unwrap()
            .and_hms_opt(13, 0, 0)
            .unwrap();
        let expected = chrono::Utc
            .from_utc_datetime(&utc_dt)
            .with_timezone(&chrono::Local)
            .naive_local();
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_ics_datetime_tzid_ny_est() {
        // 2026-01-15: New York is in EST (UTC-5)
        // 09:00 EST = 14:00 UTC
        let dt = parse_ics_datetime_with_tz("20260115T090000", Some("America/New_York")).unwrap();
        let utc_dt = chrono::NaiveDate::from_ymd_opt(2026, 1, 15)
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
        // Asia/Tokyo: no DST, always UTC+9
        // 14:00 JST = 05:00 UTC
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
    fn test_ics_datetime_tzid_sydney_dst() {
        // 2026-03-20: Sydney is in AEDT (UTC+11)
        // 10:00 AEDT = 23:00 UTC (previous day)
        let dt = parse_ics_datetime_with_tz("20260320T100000", Some("Australia/Sydney")).unwrap();
        let utc_dt = chrono::NaiveDate::from_ymd_opt(2026, 3, 19)
            .unwrap()
            .and_hms_opt(23, 0, 0)
            .unwrap();
        let expected = chrono::Utc
            .from_utc_datetime(&utc_dt)
            .with_timezone(&chrono::Local)
            .naive_local();
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_ics_datetime_tzid_unknown_returns_none() {
        assert!(parse_ics_datetime_with_tz("20260320T090000", Some("Fake/Zone")).is_none());
    }

    #[test]
    fn test_ics_dt_line_tzid_with_extra_params() {
        let p = parse_ics_dt_line("DTSTART;TZID=America/New_York;X-FOO=bar:20260320T090000");
        assert_eq!(p.value, "20260320T090000");
        assert_eq!(p.tzid.as_deref(), Some("America/New_York"));
    }

    // --- Property value extraction ---

    #[test]
    fn test_extract_ics_value_simple() {
        assert_eq!(extract_ics_value("SUMMARY:Meeting"), "Meeting");
    }

    #[test]
    fn test_extract_ics_value_with_params() {
        assert_eq!(
            extract_ics_value("SUMMARY;LANGUAGE=en:Meeting with team"),
            "Meeting with team"
        );
    }

    #[test]
    fn test_extract_ics_value_description_with_param() {
        assert_eq!(
            extract_ics_value("DESCRIPTION;ENCODING=BASE64:Notes here"),
            "Notes here"
        );
    }

    // --- Windows TZ mapping ---

    #[test]
    fn test_windows_tz_mapping() {
        assert_eq!(
            windows_tz_to_iana("Eastern Standard Time"),
            Some("America/New_York")
        );
        assert_eq!(
            windows_tz_to_iana("Tokyo Standard Time"),
            Some("Asia/Tokyo")
        );
        assert_eq!(windows_tz_to_iana("Unknown TZ"), None);
    }

    // --- VTIMEZONE map ---

    #[test]
    fn test_build_vtimezone_map_with_x_lic_location() {
        let lines = vec![
            "BEGIN:VTIMEZONE".to_string(),
            "TZID:Custom/Eastern".to_string(),
            "X-LIC-LOCATION:America/New_York".to_string(),
            "END:VTIMEZONE".to_string(),
        ];
        let map = build_vtimezone_map(&lines);
        assert_eq!(
            map.get("Custom/Eastern").map(|s| s.as_str()),
            Some("America/New_York")
        );
    }

    #[test]
    fn test_build_vtimezone_map_windows_tz() {
        let lines = vec![
            "BEGIN:VTIMEZONE".to_string(),
            "TZID:Eastern Standard Time".to_string(),
            "END:VTIMEZONE".to_string(),
        ];
        let map = build_vtimezone_map(&lines);
        assert_eq!(
            map.get("Eastern Standard Time").map(|s| s.as_str()),
            Some("America/New_York")
        );
    }

    #[test]
    fn test_resolve_tzid_iana() {
        let map = std::collections::HashMap::new();
        assert_eq!(
            resolve_tzid(Some("Asia/Tokyo"), &map).as_deref(),
            Some("Asia/Tokyo")
        );
    }

    #[test]
    fn test_resolve_tzid_windows() {
        let map = std::collections::HashMap::new();
        assert_eq!(
            resolve_tzid(Some("Eastern Standard Time"), &map).as_deref(),
            Some("America/New_York")
        );
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
