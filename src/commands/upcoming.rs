use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::{Duration, Local};

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    days: u32,
    calendar: Option<String>,
    format: OutputFormat,
    verbose: bool,
    fields: Option<&str>,
    no_color: bool,
    no_header: bool,
) -> Result<(), AppError> {
    let today = Local::now().date_naive();
    let end = today + Duration::days(days as i64);
    let events = store.events(today, end, calendar.as_deref())?;
    super::events::print_events(events, format, verbose, fields, no_color, no_header);
    Ok(())
}
