use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::Local;

use super::events::DisplayOpts;

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    super::events::validate_opts(opts)?;
    let today = Local::now().date_naive();
    let events = store.events(today, today, calendar.as_deref())?;
    super::events::print_events(events, format, opts)
}
