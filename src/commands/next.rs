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

    // Find currently in-progress event (earliest start among active)
    let current = events
        .iter()
        .filter(|e| !e.all_day && e.start <= now && e.end > now)
        .min_by_key(|e| e.start);

    // Find next future event (earliest start > now)
    let next = events
        .iter()
        .filter(|e| !e.all_day && e.start > now)
        .min_by_key(|e| e.start);

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
