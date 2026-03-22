use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::state::{self, UndoAction};
use crate::store::{CalendarStore, EventDraft, RecurrenceScope};
use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone};

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
    calendar: Option<&str>,
    keep_recurrence: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
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
    let draft = store.event_draft(&event.id, event.start)?;
    let (start_dt, end_dt) = resolve_instantiation_times(&draft, start, end)?;
    state::ensure_no_pending_undo()?;
    let created = store.create_event_from_draft(
        &draft,
        title,
        start_dt,
        end_dt,
        calendar,
        keep_recurrence,
    )?;
    super::save_undo_best_effort(
        UndoAction::DeleteCreated {
            event_id: created.id.clone(),
            selected_start: created.start,
            scope: created.recurring.then_some(RecurrenceScope::Future),
        },
        format,
    );

    print_output(format, &created, |ev, out| {
        writeln!(out, "Duplicated: {} ({})", ev.title, ev.id)
    })?;
    Ok(())
}

pub(crate) fn resolve_instantiation_times(
    draft: &EventDraft,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<(DateTime<Local>, DateTime<Local>), AppError> {
    if draft.all_day {
        let span_days = (draft.end.date_naive() - draft.start.date_naive()).num_days();
        let start_dt = match start {
            Some(value) => {
                let date = dateparse::parse_all_day_date(value)
                    .ok_or_else(|| AppError::InvalidDate(value.to_string()))?;
                localize(date.and_hms_opt(0, 0, 0).unwrap())?
            }
            None => draft.start,
        };
        let end_dt = match end {
            Some(value) => {
                let date = dateparse::parse_all_day_date(value)
                    .ok_or_else(|| AppError::InvalidDate(value.to_string()))?;
                localize(
                    (date + chrono::Duration::days(1))
                        .and_hms_opt(0, 0, 0)
                        .unwrap(),
                )?
            }
            None => start_dt + Duration::days(span_days),
        };
        if end_dt <= start_dt {
            return Err(AppError::InvalidDate(
                "end date must be on or after start date".to_string(),
            ));
        }
        Ok((start_dt, end_dt))
    } else {
        let start_dt = match start {
            Some(value) => parse_local_datetime(value)?,
            None => draft.start,
        };
        let end_dt = match end {
            Some(value) => parse_local_datetime(value)?,
            None if start.is_some() => start_dt + (draft.end - draft.start),
            None => draft.end,
        };
        if end_dt <= start_dt {
            return Err(AppError::InvalidDate(
                "end time must be after start time".to_string(),
            ));
        }
        Ok((start_dt, end_dt))
    }
}

fn parse_local_datetime(value: &str) -> Result<DateTime<Local>, AppError> {
    let dt =
        dateparse::parse_datetime(value).ok_or_else(|| AppError::InvalidDate(value.to_string()))?;
    localize(dt)
}

fn localize(dt: NaiveDateTime) -> Result<DateTime<Local>, AppError> {
    Local
        .from_local_datetime(&dt)
        .earliest()
        .ok_or_else(|| AppError::InvalidDate(dt.to_string()))
}

#[cfg(test)]
mod tests {
    use super::resolve_instantiation_times;
    use crate::store::EventDraft;
    use chrono::{Local, TimeZone};

    fn timed_draft() -> EventDraft {
        EventDraft {
            title: "Focus".to_string(),
            start: Local.with_ymd_and_hms(2026, 3, 20, 9, 0, 0).unwrap(),
            end: Local.with_ymd_and_hms(2026, 3, 20, 10, 30, 0).unwrap(),
            calendar: "Work".to_string(),
            calendar_id: "cal-1".to_string(),
            location: None,
            url: None,
            notes: None,
            all_day: false,
            alerts: vec![10],
            recurrence_rule: None,
        }
    }

    fn all_day_draft() -> EventDraft {
        EventDraft {
            title: "Trip".to_string(),
            start: Local.with_ymd_and_hms(2026, 3, 20, 0, 0, 0).unwrap(),
            end: Local.with_ymd_and_hms(2026, 3, 22, 0, 0, 0).unwrap(),
            calendar: "Work".to_string(),
            calendar_id: "cal-1".to_string(),
            location: None,
            url: None,
            notes: None,
            all_day: true,
            alerts: vec![],
            recurrence_rule: None,
        }
    }

    #[test]
    fn test_resolve_instantiation_times_preserves_duration_for_timed_start_override() {
        let draft = timed_draft();
        let (start, end) =
            resolve_instantiation_times(&draft, Some("2026-03-21 13:00"), None).unwrap();
        assert_eq!(
            start,
            Local.with_ymd_and_hms(2026, 3, 21, 13, 0, 0).unwrap()
        );
        assert_eq!(end, Local.with_ymd_and_hms(2026, 3, 21, 14, 30, 0).unwrap());
    }

    #[test]
    fn test_resolve_instantiation_times_preserves_all_day_span() {
        let draft = all_day_draft();
        let (start, end) = resolve_instantiation_times(&draft, Some("2026-03-25"), None).unwrap();
        assert_eq!(start, Local.with_ymd_and_hms(2026, 3, 25, 0, 0, 0).unwrap());
        assert_eq!(end, Local.with_ymd_and_hms(2026, 3, 27, 0, 0, 0).unwrap());
    }
}
