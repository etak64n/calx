use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::state::{self, UndoAction};
use crate::store::{AlertUpdate, CalendarStore, FieldUpdate, RecurrenceScope};
use chrono::{DateTime, Local};
use serde::Serialize;

#[derive(Serialize)]
struct UpdateResult {
    updated: bool,
    event_id: String,
    title: String,
    start: DateTime<Local>,
    end: DateTime<Local>,
    calendar: String,
    all_day: bool,
    alerts: Vec<i64>,
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
    clear_location: bool,
    url: Option<&str>,
    clear_url: bool,
    notes: Option<&str>,
    clear_notes: bool,
    alerts: &[i64],
    clear_alerts: bool,
    calendar: Option<&str>,
    all_day: Option<bool>,
    scope: Option<RecurrenceScope>,
    format: OutputFormat,
) -> Result<(), AppError> {
    if let Some(title) = title {
        if title.trim().is_empty() {
            return Err(AppError::InvalidArgument(
                "--title must not be empty".to_string(),
            ));
        }
    }

    if !has_requested_changes(
        title,
        start,
        end,
        location,
        clear_location,
        url,
        clear_url,
        notes,
        clear_notes,
        !alerts.is_empty(),
        clear_alerts,
        calendar,
        all_day,
    ) {
        return Err(AppError::InvalidArgument(
            "No changes specified for update.".to_string(),
        ));
    }

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

    if event.all_day && all_day == Some(false) && (start_dt.is_none() || end_dt.is_none()) {
        return Err(AppError::InvalidArgument(
            "Converting an all-day event to a timed event requires both --start and --end."
                .to_string(),
        ));
    }

    let before_draft = store.event_draft(&event.id, event.start)?;

    let location_update = field_update(location, clear_location);
    let url_update = field_update(url, clear_url);
    let notes_update = field_update(notes, clear_notes);
    let alert_update = alert_update(alerts, clear_alerts);

    state::ensure_no_pending_undo()?;

    let updated_event = store.update_event(
        &event.id,
        event.start,
        title,
        start_dt,
        end_dt,
        location_update,
        url_update,
        notes_update,
        alert_update,
        calendar,
        all_day,
        scope,
    )?;

    super::save_undo_best_effort(
        if event.recurring && scope == Some(RecurrenceScope::This) {
            UndoAction::Unavailable {
                reason: "single recurring occurrences cannot be restored safely".to_string(),
            }
        } else {
            UndoAction::ReplaceWithDraft {
                current_event_id: updated_event.id.clone(),
                current_start: updated_event.start,
                current_scope: updated_event.recurring.then_some(RecurrenceScope::Future),
                draft: before_draft,
            }
        },
        format,
    );

    let result = UpdateResult {
        updated: true,
        event_id: updated_event.id.clone(),
        title: updated_event.title.clone(),
        start: updated_event.start,
        end: updated_event.end,
        calendar: updated_event.calendar.clone(),
        all_day: updated_event.all_day,
        alerts: updated_event.alerts.clone(),
    };
    print_output(format, &result, |_, out| {
        writeln!(
            out,
            "Event updated: {} ({})",
            updated_event.title, updated_event.id
        )
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn has_requested_changes(
    title: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
    location: Option<&str>,
    clear_location: bool,
    url: Option<&str>,
    clear_url: bool,
    notes: Option<&str>,
    clear_notes: bool,
    alerts: bool,
    clear_alerts: bool,
    calendar: Option<&str>,
    all_day: Option<bool>,
) -> bool {
    title.is_some()
        || start.is_some()
        || end.is_some()
        || location.is_some()
        || clear_location
        || url.is_some()
        || clear_url
        || notes.is_some()
        || clear_notes
        || alerts
        || clear_alerts
        || calendar.is_some()
        || all_day.is_some()
}

fn field_update<'a>(value: Option<&'a str>, clear: bool) -> FieldUpdate<'a> {
    if clear {
        FieldUpdate::Clear
    } else if let Some(value) = value {
        FieldUpdate::Set(value)
    } else {
        FieldUpdate::Keep
    }
}

fn alert_update<'a>(values: &'a [i64], clear: bool) -> AlertUpdate<'a> {
    if clear {
        AlertUpdate::Clear
    } else if !values.is_empty() {
        AlertUpdate::Set(values)
    } else {
        AlertUpdate::Keep
    }
}
