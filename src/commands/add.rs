use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use serde::Serialize;

#[derive(Serialize)]
struct AddResult {
    event_id: String,
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
    let start_dt =
        dateparse::parse_datetime(start).ok_or_else(|| AppError::InvalidDate(start.to_string()))?;
    let end_dt =
        dateparse::parse_datetime(end).ok_or_else(|| AppError::InvalidDate(end.to_string()))?;
    let event_id = store.add_event(title, start_dt, end_dt, calendar, notes, all_day)?;

    let result = AddResult { event_id };
    print_output(format, &result, |r| {
        println!("Event created: {}", r.event_id);
    });
    Ok(())
}
