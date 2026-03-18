use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use serde::Serialize;

#[derive(Serialize)]
struct UpdateResult {
    updated: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    event_id: &str,
    title: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
    location: Option<&str>,
    url: Option<&str>,
    notes: Option<&str>,
    calendar: Option<&str>,
    all_day: Option<bool>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let start_dt = start
        .map(|s| dateparse::parse_datetime(s).ok_or_else(|| AppError::InvalidDate(s.to_string())))
        .transpose()?;
    let end_dt = end
        .map(|s| dateparse::parse_datetime(s).ok_or_else(|| AppError::InvalidDate(s.to_string())))
        .transpose()?;

    store.update_event(
        event_id, title, start_dt, end_dt, location, url, notes, calendar, all_day,
    )?;

    let result = UpdateResult { updated: true };
    print_output(format, &result, |_| {
        println!("Event updated.");
    });
    Ok(())
}
