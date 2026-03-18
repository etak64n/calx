use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use serde::Serialize;

#[derive(Serialize)]
struct DeleteResult {
    deleted: bool,
}

pub fn run(store: &CalendarStore, event_id: &str, format: OutputFormat) -> Result<(), AppError> {
    store.delete_event(event_id)?;

    let result = DeleteResult { deleted: true };
    print_output(format, &result, |_| {
        println!("Event deleted.");
    });
    Ok(())
}
