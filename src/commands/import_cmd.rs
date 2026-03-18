use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::NaiveDateTime;
use serde::Serialize;
use std::io::Read;

#[derive(Serialize)]
struct ImportResult {
    imported: usize,
}

/// Validates input first, then creates store only if needed.
pub fn run(file: &str, format: OutputFormat) -> Result<(), AppError> {
    // 1. Read content (validates file existence / stdin)
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

    // 2. Determine format (validates extension)
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

    // 3. Now request calendar access
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

fn import_ics(store: &CalendarStore, content: &str) -> Result<usize, AppError> {
    let mut count = 0;
    let mut title = String::new();
    let mut start = String::new();
    let mut end = String::new();
    let mut notes = String::new();
    let mut is_all_day = false;
    let mut in_event = false;

    for line in content.lines() {
        let line = line.trim();
        match line {
            "BEGIN:VEVENT" => {
                in_event = true;
                title.clear();
                start.clear();
                end.clear();
                notes.clear();
                is_all_day = false;
            }
            "END:VEVENT" if in_event => {
                in_event = false;
                if !title.is_empty() && !start.is_empty() && !end.is_empty() {
                    let start_dt = parse_ics_datetime(&start)
                        .ok_or_else(|| AppError::InvalidDate(start.clone()))?;
                    let end_dt = parse_ics_datetime(&end)
                        .ok_or_else(|| AppError::InvalidDate(end.clone()))?;
                    let notes_opt = if notes.is_empty() {
                        None
                    } else {
                        Some(notes.replace("\\n", "\n"))
                    };
                    store.add_event(
                        &title,
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
                    let (val, all_day) = parse_ics_dt_line(line);
                    start = val;
                    if all_day {
                        is_all_day = true;
                    }
                } else if line.starts_with("DTEND") {
                    let (val, _) = parse_ics_dt_line(line);
                    end = val;
                }
            }
            _ => {}
        }
    }
    Ok(count)
}

/// Parse a DTSTART or DTEND line, handling various ICS formats:
/// DTSTART:20260320T140000
/// DTSTART:20260320T140000Z
/// DTSTART;VALUE=DATE:20260320
/// DTSTART;TZID=Asia/Tokyo:20260320T140000
fn parse_ics_dt_line(line: &str) -> (String, bool) {
    let is_all_day = line.contains("VALUE=DATE");
    // Extract the value after the last ':'
    let val = line.rsplit(':').next().unwrap_or("").to_string();
    (val, is_all_day)
}

fn import_csv(store: &CalendarStore, content: &str) -> Result<usize, AppError> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut count = 0;

    for result in rdr.records() {
        let record = result.map_err(|e| AppError::EventKit(e.to_string()))?;
        let title = record.get(1).unwrap_or_default();
        let start_str = record.get(2).unwrap_or_default();
        let end_str = record.get(3).unwrap_or_default();
        let notes = record.get(6).filter(|s| !s.is_empty());

        let start_dt = parse_csv_datetime(start_str)
            .ok_or_else(|| AppError::InvalidDate(start_str.to_string()))?;
        let end_dt = parse_csv_datetime(end_str)
            .ok_or_else(|| AppError::InvalidDate(end_str.to_string()))?;

        store.add_event(
            title, start_dt, end_dt, None, None, None, notes, false, None, None,
        )?;
        count += 1;
    }
    Ok(count)
}

pub(crate) fn parse_ics_datetime(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim_end_matches('Z'); // Handle UTC suffix
    NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S")
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(s, "%Y%m%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .ok()
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

    #[test]
    fn test_ics_datetime_full() {
        let dt = parse_ics_datetime("20260320T140000").unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 20);
        assert_eq!(dt.hour(), 14);
    }

    #[test]
    fn test_ics_datetime_with_z() {
        let dt = parse_ics_datetime("20260320T140000Z").unwrap();
        assert_eq!(dt.hour(), 14);
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
    fn test_ics_dt_line_basic() {
        let (val, all_day) = parse_ics_dt_line("DTSTART:20260320T140000");
        assert_eq!(val, "20260320T140000");
        assert!(!all_day);
    }

    #[test]
    fn test_ics_dt_line_utc() {
        let (val, all_day) = parse_ics_dt_line("DTSTART:20260320T140000Z");
        assert_eq!(val, "20260320T140000Z");
        assert!(!all_day);
    }

    #[test]
    fn test_ics_dt_line_value_date() {
        let (val, all_day) = parse_ics_dt_line("DTSTART;VALUE=DATE:20260320");
        assert_eq!(val, "20260320");
        assert!(all_day);
    }

    #[test]
    fn test_ics_dt_line_tzid() {
        let (val, all_day) = parse_ics_dt_line("DTSTART;TZID=Asia/Tokyo:20260320T140000");
        assert_eq!(val, "20260320T140000");
        assert!(!all_day);
    }

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
