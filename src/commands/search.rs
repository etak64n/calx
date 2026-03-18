use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::{Duration, Local};

use super::events::DisplayOpts;

pub fn run(
    store: &CalendarStore,
    query: &str,
    from: Option<String>,
    to: Option<String>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    let today = Local::now().date_naive();
    let from_date = match from {
        Some(s) => dateparse::parse_date(&s).ok_or(AppError::InvalidDate(s))?,
        None => today,
    };
    let to_date = match to {
        Some(s) => dateparse::parse_date(&s).ok_or(AppError::InvalidDate(s))?,
        None => from_date + Duration::days(90),
    };

    let events = store.search_events(query, from_date, to_date)?;
    super::events::print_events(events, format, opts);
    Ok(())
}
