use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::store::CalendarStore;

use super::events::DisplayOpts;

pub fn run(
    store: &CalendarStore,
    event_id: &str,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    let event = store.get_event(event_id)?;
    let show_opts = DisplayOpts {
        verbose: true,
        ..*opts
    };
    super::events::print_events(vec![event], format, &show_opts);
    Ok(())
}
