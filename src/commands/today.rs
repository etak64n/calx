use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::Local;

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let today = Local::now().date_naive();
    let events = store.events(today, today, calendar.as_deref())?;
    super::events::print_events(events, format);
    Ok(())
}
