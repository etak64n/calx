use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::{NaiveDateTime, TimeZone};
use ical::parser::ical::component::IcalEvent;
use ical::property::Property;
use serde::Serialize;
use std::io::{BufReader, Read};

#[derive(Serialize)]
struct ImportResult {
    imported: usize,
}

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

// --- ICS import via `ical` crate ---

fn import_ics(store: &CalendarStore, content: &str) -> Result<usize, AppError> {
    let reader = BufReader::new(content.as_bytes());
    let parser = ical::IcalParser::new(reader);

    let mut count = 0;

    for cal_result in parser {
        let cal = cal_result.map_err(|e| AppError::EventKit(format!("ICS parse error: {e}")))?;

        // Build TZID mapping from VTIMEZONE components
        let tz_map = build_tz_map_from_timezones(&cal.timezones);

        for event in &cal.events {
            if let Some(imported) = import_single_event(store, event, &tz_map)? {
                count += imported;
            }
        }
    }

    Ok(count)
}

fn import_single_event(
    store: &CalendarStore,
    event: &IcalEvent,
    tz_map: &std::collections::HashMap<String, String>,
) -> Result<Option<usize>, AppError> {
    let title = get_prop_value(event, "SUMMARY").unwrap_or_default();
    if title.is_empty() {
        return Ok(None);
    }

    let start_prop = get_prop(event, "DTSTART");
    let (start_val, start_tzid) = match &start_prop {
        Some(p) => (p.value.clone().unwrap_or_default(), get_param(p, "TZID")),
        None => return Ok(None),
    };

    let is_all_day = start_prop
        .as_ref()
        .is_some_and(|p| get_param(p, "VALUE").as_deref() == Some("DATE"));

    let s_tz = resolve_tzid(start_tzid.as_deref(), tz_map);
    let start_dt = parse_ics_datetime_with_tz(&start_val, s_tz.as_deref())
        .ok_or_else(|| AppError::InvalidDate(start_val.clone()))?;

    // End time: DTEND, or DTSTART + DURATION, or DTSTART + 1h default
    let end_dt = if let Some(end_prop) = get_prop(event, "DTEND") {
        let end_val = end_prop.value.clone().unwrap_or_default();
        let end_tzid = get_param(end_prop, "TZID");
        let e_tz = resolve_tzid(end_tzid.as_deref(), tz_map);
        parse_ics_datetime_with_tz(&end_val, e_tz.as_deref())
            .ok_or(AppError::InvalidDate(end_val))?
    } else if let Some(dur_str) = get_prop_value(event, "DURATION") {
        let dur = parse_ics_duration(&dur_str).unwrap_or(chrono::Duration::hours(1));
        start_dt + dur
    } else if is_all_day {
        // All-day with no DTEND: same day
        start_dt + chrono::Duration::days(1)
    } else {
        // No DTEND, no DURATION: default 1 hour
        start_dt + chrono::Duration::hours(1)
    };

    let notes = get_prop_value(event, "DESCRIPTION");
    let notes_unescaped = notes.as_deref().map(ics_unescape);
    let title_unescaped = ics_unescape(&title);
    let location = get_prop_value(event, "LOCATION").map(|s| ics_unescape(&s));
    let url = get_prop_value(event, "URL");

    // Parse RRULE for recurrence
    let rrule = parse_rrule(event);

    store.add_event(
        &title_unescaped,
        start_dt,
        end_dt,
        None,
        location.as_deref(),
        url.as_deref(),
        notes_unescaped.as_deref(),
        is_all_day,
        rrule.freq.as_deref(),
        rrule.count,
        rrule.interval,
    )?;

    Ok(Some(1))
}

struct RRuleInfo {
    freq: Option<String>,
    count: Option<u32>,
    interval: Option<u32>,
}

/// Parse RRULE property into frequency, count, and interval.
/// UNTIL is converted to an approximate COUNT based on frequency.
fn parse_rrule(event: &IcalEvent) -> RRuleInfo {
    let rrule_val = match get_prop_value(event, "RRULE") {
        Some(v) => v,
        None => {
            return RRuleInfo {
                freq: None,
                count: None,
                interval: None,
            };
        }
    };

    let mut freq = None;
    let mut count = None;
    let mut interval = None;
    let mut until = None;

    for part in rrule_val.split(';') {
        if let Some(f) = part.strip_prefix("FREQ=") {
            freq = Some(f.to_lowercase());
        } else if let Some(c) = part.strip_prefix("COUNT=") {
            count = c.parse().ok();
        } else if let Some(i) = part.strip_prefix("INTERVAL=") {
            interval = i.parse().ok();
        } else if let Some(u) = part.strip_prefix("UNTIL=") {
            until = Some(u.to_string());
        }
    }

    // If UNTIL is set but COUNT is not, approximate COUNT from UNTIL
    if count.is_none() {
        if let (Some(until_str), Some(f)) = (&until, &freq) {
            if let Some(start_prop) = get_prop(event, "DTSTART") {
                if let Some(ref start_val) = start_prop.value {
                    count = approximate_count_from_until(start_val, until_str, f, interval);
                }
            }
        }
    }

    RRuleInfo {
        freq,
        count,
        interval,
    }
}

/// Approximate COUNT from UNTIL date by computing the number of occurrences.
fn approximate_count_from_until(
    start: &str,
    until: &str,
    freq: &str,
    interval: Option<u32>,
) -> Option<u32> {
    let start_dt = parse_ics_datetime_with_tz(start, None)?;
    let until_str = until.trim_end_matches('Z');
    let until_dt = NaiveDateTime::parse_from_str(until_str, "%Y%m%dT%H%M%S")
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(until_str, "%Y%m%d")
                .map(|d| d.and_hms_opt(23, 59, 59).unwrap())
        })
        .ok()?;

    let days = (until_dt - start_dt).num_days().max(0) as u32;
    let interval = interval.unwrap_or(1).max(1);

    let count = match freq {
        "daily" => days / interval,
        "weekly" => days / (7 * interval),
        "monthly" => days / (30 * interval),
        "yearly" => days / (365 * interval),
        _ => return None,
    };

    Some(count.max(1))
}

/// Parse ICS DURATION value (e.g. "PT1H", "PT30M", "P1D", "PT1H30M").
fn parse_ics_duration(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    let s = s.strip_prefix('P')?;

    let mut total_mins: i64 = 0;

    // Handle date part (days)
    if let Some((days_str, rest)) = s.split_once('D') {
        let days: i64 = days_str.parse().ok()?;
        total_mins += days * 24 * 60;
        // Rest may have time part after 'T'
        if let Some(time_part) = rest.strip_prefix('T') {
            total_mins += parse_ics_time_duration(time_part)?;
        }
    } else if let Some(time_part) = s.strip_prefix('T') {
        total_mins += parse_ics_time_duration(time_part)?;
    } else {
        // Could be weeks: "P1W"
        if let Some(weeks_str) = s.strip_suffix('W') {
            let weeks: i64 = weeks_str.parse().ok()?;
            total_mins += weeks * 7 * 24 * 60;
        }
    }

    Some(chrono::Duration::minutes(total_mins))
}

fn parse_ics_time_duration(s: &str) -> Option<i64> {
    let mut total = 0i64;
    let mut num_buf = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            num_buf.push(c);
        } else {
            let n: i64 = num_buf.parse().ok()?;
            num_buf.clear();
            match c {
                'H' => total += n * 60,
                'M' => total += n,
                'S' => {} // ignore seconds
                _ => return None,
            }
        }
    }
    Some(total)
}

fn get_prop<'a>(event: &'a IcalEvent, name: &str) -> Option<&'a Property> {
    event.properties.iter().find(|p| p.name == name)
}

fn get_prop_value(event: &IcalEvent, name: &str) -> Option<String> {
    get_prop(event, name).and_then(|p| p.value.clone())
}

fn get_param(prop: &Property, name: &str) -> Option<String> {
    prop.params.as_ref().and_then(|params| {
        params
            .iter()
            .find(|(k, _)| k == name)
            .and_then(|(_, values)| values.first().cloned())
    })
}

/// Build TZID -> IANA name mapping from VTIMEZONE components.
fn build_tz_map_from_timezones(
    timezones: &[ical::parser::ical::component::IcalTimeZone],
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();

    for tz in timezones {
        let tzid = tz
            .properties
            .iter()
            .find(|p| p.name == "TZID")
            .and_then(|p| p.value.clone())
            .unwrap_or_default();

        if tzid.is_empty() {
            continue;
        }

        // Already a valid IANA name?
        if tzid.parse::<chrono_tz::Tz>().is_ok() {
            continue;
        }

        // Try X-LIC-LOCATION
        let x_lic = tz
            .properties
            .iter()
            .find(|p| p.name == "X-LIC-LOCATION")
            .and_then(|p| p.value.clone());

        if let Some(ref loc) = x_lic {
            if loc.parse::<chrono_tz::Tz>().is_ok() {
                map.insert(tzid.clone(), loc.clone());
                continue;
            }
        }

        // Try Windows TZ name
        if let Some(iana) = windows_tz_to_iana(&tzid) {
            map.insert(tzid, iana.to_string());
        }
    }

    map
}

fn resolve_tzid(
    tzid: Option<&str>,
    tz_map: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let name = tzid?;
    if name.parse::<chrono_tz::Tz>().is_ok() {
        return Some(name.to_string());
    }
    if let Some(mapped) = tz_map.get(name) {
        return Some(mapped.clone());
    }
    if let Some(iana) = windows_tz_to_iana(name) {
        return Some(iana.to_string());
    }
    Some(name.to_string())
}

/// Unescape RFC 5545 TEXT values.
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
        let s = s.trim_end_matches('Z');
        let utc_dt = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
        let utc = chrono::Utc.from_utc_datetime(&utc_dt);
        Some(utc.with_timezone(&chrono::Local).naive_local())
    } else if let Some(tz_name) = tzid {
        let naive = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
        // Try chrono-tz; if unknown TZID, fall back to treating as local time
        if let Ok(tz) = tz_name.parse::<chrono_tz::Tz>() {
            let dt = tz.from_local_datetime(&naive).earliest()?;
            Some(dt.with_timezone(&chrono::Local).naive_local())
        } else {
            // Unknown TZID: treat as local time (best effort) with warning
            eprintln!("Warning: unknown timezone '{tz_name}', treating as local time");
            Some(naive)
        }
    } else {
        NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S")
            .or_else(|_| {
                chrono::NaiveDate::parse_from_str(s, "%Y%m%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            })
            .ok()
    }
}

#[cfg(test)]
fn parse_ics_datetime(s: &str) -> Option<NaiveDateTime> {
    parse_ics_datetime_with_tz(s, None)
}

// --- CSV import ---

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
            title, start_dt, end_dt, calendar, location, url, notes, all_day, None, None, None,
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
        let expected = chrono::Utc
            .from_utc_datetime(&expected_utc)
            .with_timezone(&chrono::Local)
            .naive_local();
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_ics_datetime_tzid_ny_dst() {
        // 2026-03-20: EDT (UTC-4). 09:00 EDT = 13:00 UTC
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
        // 2026-01-15: EST (UTC-5). 09:00 EST = 14:00 UTC
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

    #[test]
    fn test_ics_datetime_tzid_unknown_falls_back_to_local() {
        // Unknown TZID should return the naive time as-is (local fallback)
        let dt = parse_ics_datetime_with_tz("20260320T090000", Some("Fake/Zone")).unwrap();
        assert_eq!(dt.hour(), 9);
        assert_eq!(dt.day(), 20);
    }

    // --- Text escaping ---

    #[test]
    fn test_ics_unescape_basic() {
        assert_eq!(ics_unescape("hello\\, world"), "hello, world");
        assert_eq!(ics_unescape("a\\;b\\\\c"), "a;b\\c");
        assert_eq!(ics_unescape("line1\\nline2"), "line1\nline2");
    }

    #[test]
    fn test_ics_escape_unescape_roundtrip() {
        let original = "Meeting, with; special\\chars\nand newlines";
        let escaped = crate::output::ics_escape(original);
        let unescaped = ics_unescape(&escaped);
        assert_eq!(unescaped, original);
    }

    // --- Windows TZ ---

    #[test]
    fn test_windows_tz_mapping() {
        assert_eq!(
            windows_tz_to_iana("Eastern Standard Time"),
            Some("America/New_York")
        );
        assert_eq!(windows_tz_to_iana("Unknown TZ"), None);
    }

    // --- Resolve TZID ---

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

    // --- CSV datetime ---

    // --- DURATION parsing ---

    #[test]
    fn test_parse_ics_duration_hours() {
        let d = parse_ics_duration("PT1H").unwrap();
        assert_eq!(d.num_minutes(), 60);
    }

    #[test]
    fn test_parse_ics_duration_minutes() {
        let d = parse_ics_duration("PT30M").unwrap();
        assert_eq!(d.num_minutes(), 30);
    }

    #[test]
    fn test_parse_ics_duration_hours_minutes() {
        let d = parse_ics_duration("PT1H30M").unwrap();
        assert_eq!(d.num_minutes(), 90);
    }

    #[test]
    fn test_parse_ics_duration_days() {
        let d = parse_ics_duration("P1D").unwrap();
        assert_eq!(d.num_hours(), 24);
    }

    #[test]
    fn test_parse_ics_duration_weeks() {
        let d = parse_ics_duration("P1W").unwrap();
        assert_eq!(d.num_days(), 7);
    }

    #[test]
    fn test_parse_ics_duration_days_and_time() {
        let d = parse_ics_duration("P1DT2H30M").unwrap();
        assert_eq!(d.num_minutes(), 24 * 60 + 2 * 60 + 30);
    }

    // --- RRULE parsing ---

    #[test]
    fn test_parse_rrule_weekly_with_count() {
        let mut event = IcalEvent::new();
        event.properties.push(Property {
            name: "RRULE".to_string(),
            value: Some("FREQ=WEEKLY;COUNT=10".to_string()),
            params: None,
        });
        let r = parse_rrule(&event);
        assert_eq!(r.freq.as_deref(), Some("weekly"));
        assert_eq!(r.count, Some(10));
        assert_eq!(r.interval, None);
    }

    #[test]
    fn test_parse_rrule_with_interval() {
        let mut event = IcalEvent::new();
        event.properties.push(Property {
            name: "RRULE".to_string(),
            value: Some("FREQ=WEEKLY;INTERVAL=2".to_string()),
            params: None,
        });
        let r = parse_rrule(&event);
        assert_eq!(r.freq.as_deref(), Some("weekly"));
        assert_eq!(r.interval, Some(2));
        assert_eq!(r.count, None);
    }

    #[test]
    fn test_parse_rrule_daily_no_count() {
        let mut event = IcalEvent::new();
        event.properties.push(Property {
            name: "RRULE".to_string(),
            value: Some("FREQ=DAILY".to_string()),
            params: None,
        });
        let r = parse_rrule(&event);
        assert_eq!(r.freq.as_deref(), Some("daily"));
        assert_eq!(r.count, None);
        assert_eq!(r.interval, None);
    }

    #[test]
    fn test_parse_rrule_none() {
        let event = IcalEvent::new();
        let r = parse_rrule(&event);
        assert!(r.freq.is_none());
        assert!(r.count.is_none());
    }

    // --- CSV datetime ---

    #[test]
    fn test_csv_datetime_rfc3339() {
        let dt = parse_csv_datetime("2026-03-18T11:00:00+09:00").unwrap();
        assert_eq!(dt.hour(), 11);
    }

    #[test]
    fn test_csv_datetime_simple() {
        let dt = parse_csv_datetime("2026-03-20 14:00").unwrap();
        assert_eq!(dt.hour(), 14);
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
