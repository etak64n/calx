use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::store::CalendarStore;

pub fn run(
    store: &CalendarStore,
    event_id: &str,
    format: OutputFormat,
    _verbose: bool,
    fields: Option<&str>,
    no_color: bool,
    no_header: bool,
) -> Result<(), AppError> {
    let event = store.get_event(event_id)?;
    // Always show verbose for `show` (it's a detail view)
    super::events::print_events(vec![event], format, true, fields, no_color, no_header);
    Ok(())
}
