use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::{Duration, Local};

use super::events::DisplayOpts;

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    let now = Local::now();
    let today = now.date_naive();
    let lookahead = today + Duration::days(30);
    let events = store.events(today, lookahead, calendar.as_deref())?;

    let current = events
        .iter()
        .find(|e| !e.all_day && e.start <= now && e.end > now);
    let next = events.iter().find(|e| !e.all_day && e.start > now);

    let ev = current.or(next);

    match ev {
        Some(ev) => {
            super::events::print_events(vec![ev.clone()], format, opts);
        }
        None => {
            if matches!(format, OutputFormat::Human) {
                println!("No upcoming events.");
            } else {
                println!("null");
            }
        }
    }
    Ok(())
}
