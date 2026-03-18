use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::NaiveDateTime;
use serde::Serialize;
use std::fs;

#[derive(Serialize)]
struct ImportResult {
    imported: usize,
}

pub fn run(store: &CalendarStore, file: &str, format: OutputFormat) -> Result<(), AppError> {
    let content = fs::read_to_string(file)
        .map_err(|e| AppError::EventKit(format!("Failed to read file: {e}")))?;

    let count = if file.ends_with(".ics") {
        import_ics(store, &content)?
    } else if file.ends_with(".csv") {
        import_csv(store, &content)?
    } else {
        return Err(AppError::EventKit(
            "Unknown file format. Use .ics or .csv".to_string(),
        ));
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
                    store.add_event(&title, start_dt, end_dt, None, notes_opt.as_deref(), false)?;
                    count += 1;
                }
            }
            _ if in_event => {
                if let Some(v) = line.strip_prefix("SUMMARY:") {
                    title = v.to_string();
                } else if let Some(v) = line.strip_prefix("DTSTART:") {
                    start = v.to_string();
                } else if let Some(v) = line.strip_prefix("DTSTART;VALUE=DATE:") {
                    start = v.to_string();
                } else if let Some(v) = line.strip_prefix("DTEND:") {
                    end = v.to_string();
                } else if let Some(v) = line.strip_prefix("DTEND;VALUE=DATE:") {
                    end = v.to_string();
                } else if let Some(v) = line.strip_prefix("DESCRIPTION:") {
                    notes = v.to_string();
                }
            }
            _ => {}
        }
    }
    Ok(count)
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

        store.add_event(title, start_dt, end_dt, None, notes, false)?;
        count += 1;
    }
    Ok(count)
}

fn parse_ics_datetime(s: &str) -> Option<NaiveDateTime> {
    // 20260320T140000
    NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S")
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(s, "%Y%m%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .ok()
}

fn parse_csv_datetime(s: &str) -> Option<NaiveDateTime> {
    // Try RFC3339 first, then fallback
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
