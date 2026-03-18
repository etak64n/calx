use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use serde::Serialize;

#[derive(Serialize)]
struct DeleteResult {
    deleted: bool,
    event_id: String,
}

pub fn run(
    store: &CalendarStore,
    event_id: &str,
    dry_run: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
    // Show what would be deleted
    let event = store.get_event(event_id)?;

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

    store.delete_event(event_id)?;

    let result = DeleteResult {
        deleted: true,
        event_id: event_id.to_string(),
    };
    print_output(format, &result, |_| {
        println!("Deleted: {} ({})", event.title, event_id);
    });
    Ok(())
}
