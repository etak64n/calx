use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::state::{self, UndoAction};
use crate::store::CalendarStore;
use crate::store::RecurrenceScope;
use serde::Serialize;

#[derive(Serialize)]
struct ConflictWarning<'a> {
    warning: &'a str,
    conflict_count: usize,
    conflicts: &'a [crate::store::EventInfo],
}

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
    alerts: &[i64],
    check_conflicts: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
    let (start_dt, end_dt) = if all_day {
        let start_date = dateparse::parse_all_day_date(start)
            .ok_or_else(|| AppError::InvalidDate(start.to_string()))?;
        let end_date = dateparse::parse_all_day_date(end)
            .ok_or_else(|| AppError::InvalidDate(end.to_string()))?;
        (
            start_date.and_hms_opt(0, 0, 0).unwrap(),
            (end_date + chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
    } else {
        (
            dateparse::parse_datetime(start)
                .ok_or_else(|| AppError::InvalidDate(start.to_string()))?,
            dateparse::parse_datetime(end).ok_or_else(|| AppError::InvalidDate(end.to_string()))?,
        )
    };

    if !all_day && end_dt <= start_dt {
        return Err(AppError::InvalidDate(
            "end time must be after start time".to_string(),
        ));
    }

    // Check for conflicts if requested
    if check_conflicts {
        let conflicts = store.conflicts(start_dt, end_dt, calendar)?;
        if !conflicts.is_empty() {
            if matches!(format.resolve_for_stdout(), OutputFormat::Human) {
                eprintln!("Warning: {} conflicting event(s) found:", conflicts.len());
                for c in &conflicts {
                    if c.all_day {
                        let end_date = c.end.date_naive() - chrono::Duration::days(1);
                        if end_date > c.start.date_naive() {
                            eprintln!(
                                "  {} to {} | {}",
                                c.start.format("%Y-%m-%d"),
                                end_date.format("%Y-%m-%d"),
                                c.title
                            );
                        } else {
                            eprintln!("  {} | {}", c.start.format("%Y-%m-%d"), c.title);
                        }
                    } else {
                        eprintln!(
                            "  {} - {} | {}",
                            c.start.format("%H:%M"),
                            c.end.format("%H:%M"),
                            c.title
                        );
                    }
                }
                eprintln!();
            } else {
                super::emit_warning(
                    format,
                    "Conflicting events found.",
                    &ConflictWarning {
                        warning: "Conflicting events found.",
                        conflict_count: conflicts.len(),
                        conflicts: &conflicts,
                    },
                );
            }
        }
    }

    state::ensure_no_pending_undo()?;

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
        alerts,
    )?;

    let event = store.get_event(&event_id)?;
    super::save_undo_best_effort(
        UndoAction::DeleteCreated {
            event_id: event.id.clone(),
            selected_start: event.start,
            scope: event.recurring.then_some(RecurrenceScope::Future),
        },
        format,
    );
    print_output(format, &event, |ev, out| {
        writeln!(out, "Event created: {}", ev.id)
    })?;
    Ok(())
}
