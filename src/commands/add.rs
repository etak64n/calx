use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    title: &str,
    start: &str,
    end: &str,
    calendar: Option<&str>,
    location: Option<&str>,
    url: Option<&str>,
    notes: Option<&str>,
    all_day: bool,
    repeat: Option<&str>,
    repeat_count: Option<u32>,
    repeat_interval: Option<u32>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let start_dt =
        dateparse::parse_datetime(start).ok_or_else(|| AppError::InvalidDate(start.to_string()))?;
    let end_dt =
        dateparse::parse_datetime(end).ok_or_else(|| AppError::InvalidDate(end.to_string()))?;
    let event_id = store.add_event(
        title,
        start_dt,
        end_dt,
        calendar,
        location,
        url,
        notes,
        all_day,
        repeat,
        repeat_count,
        repeat_interval,
    )?;

    // Return the created event so AI agents can verify the result
    let event = store.get_event(&event_id)?;
    print_output(format, &event, |ev| {
        println!("Event created: {}", ev.id);
    });
    Ok(())
}
