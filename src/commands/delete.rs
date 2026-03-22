use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::state::{self, UndoAction};
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
    interactive: bool,
    in_calendar: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    dry_run: bool,
    scope: Option<RecurrenceScope>,
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

    if dry_run {
        print_output(format, &event, |ev, out| {
            writeln!(out, "{}", dry_run_description(ev))
        })?;
        return Ok(());
    }

    let draft = store.event_draft(&event.id, event.start)?;
    state::ensure_no_pending_undo()?;
    store.delete_event(&event.id, event.start, scope)?;
    super::save_undo_best_effort(
        if event.recurring && scope == Some(RecurrenceScope::This) {
            UndoAction::Unavailable {
                reason: "single recurring occurrences cannot be restored safely".to_string(),
            }
        } else {
            UndoAction::RestoreDeleted { draft }
        },
        format,
    );

    let result = DeleteResult {
        deleted: true,
        event_id: event.id.clone(),
    };
    print_output(format, &result, |_, out| {
        writeln!(out, "Deleted: {} ({})", event.title, event.id)
    })?;
    Ok(())
}

fn dry_run_description(event: &crate::store::EventInfo) -> String {
    if event.all_day {
        let end_date = if event.end.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
            && event.end.date_naive() > event.start.date_naive()
        {
            event.end.date_naive() - chrono::Duration::days(1)
        } else {
            event.end.date_naive()
        };

        if end_date > event.start.date_naive() {
            format!(
                "Would delete: {} ({} to {} 00:00 - 24:00)",
                event.title,
                event.start.format("%Y-%m-%d"),
                end_date.format("%Y-%m-%d")
            )
        } else {
            format!(
                "Would delete: {} ({} 00:00 - 24:00)",
                event.title,
                event.start.format("%Y-%m-%d")
            )
        }
    } else {
        format!(
            "Would delete: {} ({} - {})",
            event.title,
            event.start.format("%Y-%m-%d %H:%M"),
            event.end.format("%H:%M")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::dry_run_description;
    use crate::store::EventInfo;
    use chrono::{Local, NaiveDate, TimeZone};

    fn local_dt(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> chrono::DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap();
        Local.from_local_datetime(&naive).earliest().unwrap()
    }

    fn make_event(
        title: &str,
        start: chrono::DateTime<Local>,
        end: chrono::DateTime<Local>,
        all_day: bool,
    ) -> EventInfo {
        EventInfo {
            id: title.to_string(),
            title: title.to_string(),
            start,
            end,
            calendar: "Work".to_string(),
            calendar_id: "cal-1".to_string(),
            location: None,
            url: None,
            notes: None,
            all_day,
            status: "confirmed".to_string(),
            availability: "busy".to_string(),
            organizer: None,
            created: None,
            modified: None,
            recurring: false,
            recurrence: None,
            recurrence_rule: None,
            alerts: Vec::new(),
        }
    }

    #[test]
    fn test_dry_run_description_formats_single_day_all_day_event() {
        let event = make_event(
            "Holiday",
            local_dt(2026, 3, 20, 0, 0),
            local_dt(2026, 3, 21, 0, 0),
            true,
        );
        assert_eq!(
            dry_run_description(&event),
            "Would delete: Holiday (2026-03-20 00:00 - 24:00)"
        );
    }

    #[test]
    fn test_dry_run_description_formats_multi_day_all_day_event() {
        let event = make_event(
            "Trip",
            local_dt(2026, 3, 20, 0, 0),
            local_dt(2026, 3, 23, 0, 0),
            true,
        );
        assert_eq!(
            dry_run_description(&event),
            "Would delete: Trip (2026-03-20 to 2026-03-22 00:00 - 24:00)"
        );
    }
}
