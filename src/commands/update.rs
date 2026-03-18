use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, RecurrenceScope};
use serde::Serialize;

#[derive(Serialize)]
struct UpdateResult {
    updated: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    event_id: Option<&str>,
    query: Option<&str>,
    exact: bool,
    interactive: bool,
    in_calendar: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    title: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
    location: Option<&str>,
    url: Option<&str>,
    notes: Option<&str>,
    calendar: Option<&str>,
    all_day: Option<bool>,
    scope: Option<RecurrenceScope>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let parse_input = |value: &str| {
        if all_day == Some(true) {
            dateparse::parse_all_day_date(value)
                .map(|date| date.and_hms_opt(0, 0, 0).unwrap())
                .ok_or_else(|| AppError::InvalidDate(value.to_string()))
        } else {
            dateparse::parse_datetime(value).ok_or_else(|| AppError::InvalidDate(value.to_string()))
        }
    };
    let start_dt = start.map(parse_input).transpose()?;
    let end_dt = end.map(parse_input).transpose()?;
    let event = super::select::resolve_event(
        store,
        event_id,
        query,
        exact,
        in_calendar,
        from,
        to,
        interactive,
    )?;

    store.update_event(
        &event.id,
        event.start,
        title,
        start_dt,
        end_dt,
        location,
        url,
        notes,
        calendar,
        all_day,
        scope,
    )?;

    let result = UpdateResult { updated: true };
    print_output(format, &result, |_| {
        println!("Event updated.");
    });
    Ok(())
}
