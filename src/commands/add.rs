use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;

#[derive(Serialize)]
struct AddResult {
    event_id: String,
}

fn parse_datetime(s: &str) -> Result<NaiveDateTime, AppError> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return Ok(dt);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d.and_hms_opt(0, 0, 0).unwrap());
    }
    Err(AppError::InvalidDate(s.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    title: &str,
    start: &str,
    end: &str,
    calendar: Option<&str>,
    notes: Option<&str>,
    all_day: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
    let start_dt = parse_datetime(start)?;
    let end_dt = parse_datetime(end)?;
    let event_id = store.add_event(title, start_dt, end_dt, calendar, notes, all_day)?;

    let result = AddResult { event_id };
    print_output(format, &result, |r| {
        println!("Event created: {}", r.event_id);
    });
    Ok(())
}
