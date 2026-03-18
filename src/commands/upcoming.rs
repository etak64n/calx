use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::{Duration, Local};

use super::events::DisplayOpts;

pub fn run(
    store: &CalendarStore,
    days: u32,
    calendar: Option<String>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    super::events::validate_opts(opts)?;
    let today = Local::now().date_naive();
    let end = today + Duration::days(days as i64);
    let events = store.events(today, end, calendar.as_deref())?;
    super::events::print_events(events, format, opts);
    Ok(())
}
