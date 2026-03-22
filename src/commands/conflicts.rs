use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;

use super::events::DisplayOpts;

pub fn run(
    store: &CalendarStore,
    start: &str,
    end: &str,
    calendar: Option<&str>,
    all_day: bool,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    super::events::validate_opts(opts)?;

    let (start_dt, end_dt) = if all_day {
        let start_date = dateparse::parse_all_day_date(start)
            .ok_or_else(|| AppError::InvalidDate(start.to_string()))?;
        let end_date = dateparse::parse_all_day_date(end)
            .ok_or_else(|| AppError::InvalidDate(end.to_string()))?;
        if end_date < start_date {
            return Err(AppError::InvalidDate(
                "end date must be on or after start date".to_string(),
            ));
        }
        (
            start_date.and_hms_opt(0, 0, 0).unwrap(),
            (end_date + chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
    } else {
        let start_dt = dateparse::parse_datetime(start)
            .ok_or_else(|| AppError::InvalidDate(start.to_string()))?;
        let end_dt =
            dateparse::parse_datetime(end).ok_or_else(|| AppError::InvalidDate(end.to_string()))?;
        if end_dt <= start_dt {
            return Err(AppError::InvalidDate(
                "end time must be after start time".to_string(),
            ));
        }
        (start_dt, end_dt)
    };

    let conflicts = store.conflicts(start_dt, end_dt, calendar)?;
    if conflicts.is_empty() && matches!(format, OutputFormat::Human) {
        return print_output(format, &conflicts, |_, out| {
            writeln!(out, "No conflicts found.")
        });
    }

    super::events::print_events(conflicts, format, opts)
}
