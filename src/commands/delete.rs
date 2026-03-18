use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, RecurrenceScope};
use serde::Serialize;

#[derive(Serialize)]
struct DeleteResult {
    deleted: bool,
    event_id: String,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    event_id: Option<&str>,
    query: Option<&str>,
    exact: bool,
    in_calendar: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    dry_run: bool,
    scope: Option<RecurrenceScope>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let event = super::select::resolve_event(store, event_id, query, exact, in_calendar, from, to)?;

    if dry_run {
        print_output(format, &event, |ev| {
            println!(
                "Would delete: {} ({} - {})",
                ev.title,
                ev.start.format("%Y-%m-%d %H:%M"),
                ev.end.format("%H:%M")
            );
        });
        return Ok(());
    }

    store.delete_event(&event.id, event.start, scope)?;

    let result = DeleteResult {
        deleted: true,
        event_id: event.id.clone(),
    };
    print_output(format, &result, |_| {
        println!("Deleted: {} ({})", event.title, event.id);
    });
    Ok(())
}
