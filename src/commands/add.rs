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

    // Check for conflicts if requested
    if check_conflicts && !all_day {
        let conflicts = store.conflicts(start_dt, end_dt, calendar)?;
        if !conflicts.is_empty() {
            eprintln!("Warning: {} conflicting event(s) found:", conflicts.len());
            for c in &conflicts {
                eprintln!(
                    "  {} - {} | {}",
                    c.start.format("%H:%M"),
                    c.end.format("%H:%M"),
                    c.title
                );
            }
            eprintln!();
        }
    }

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
    print_output(format, &event, |ev| {
        println!("Event created: {}", ev.id);
    });
    Ok(())
}
