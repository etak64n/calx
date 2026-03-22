use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::{Duration, Local, NaiveDate, NaiveTime};

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    from: Option<String>,
    to: Option<String>,
    calendar: Option<String>,
    duration: u32,
    after: Option<&str>,
    before: Option<&str>,
    limit: Option<usize>,
    format: OutputFormat,
    no_color: bool,
    no_header: bool,
) -> Result<(), AppError> {
    let today = Local::now().date_naive();
    let from_date = match from {
        Some(s) => dateparse::parse_date(&s).ok_or(AppError::InvalidDate(s))?,
        None => today,
    };
    let to_date = match to {
        Some(s) => dateparse::parse_date(&s).ok_or(AppError::InvalidDate(s))?,
        None => from_date + Duration::days(5),
    };
    super::events::validate_date_range(from_date, to_date)?;

    let after_time = after.map(parse_hhmm).transpose()?;
    let before_time = before.map(parse_hhmm).transpose()?;
    validate_time_window(after_time, before_time)?;

    let mut slots = store.free_slots(
        from_date,
        to_date,
        after_time,
        before_time,
        duration,
        calendar.as_deref(),
    )?;

    if let Some(limit) = limit {
        slots.truncate(limit);
    }

    print_output(format, &slots, |slots, out| {
        if slots.is_empty() {
            writeln!(out, "No free slots found.")?;
            return Ok(());
        }

        let (bold, dim, reset) = if !no_color {
            ("\x1b[1m", "\x1b[2m", "\x1b[0m")
        } else {
            ("", "", "")
        };

        if !no_header {
            writeln!(out, "{dim}    #  DATE        START  END    DURATION{reset}")?;
        }

        for (i, slot) in slots.iter().enumerate() {
            let dur = format_slot_duration(slot.duration_mins);
            writeln!(
                out,
                "  {row:>3}  {date}  {bold}{start} - {end}{reset}  {dim}{dur}{reset}",
                row = i + 1,
                date = slot.start.format("%Y-%m-%d"),
                start = slot.start.format("%H:%M"),
                end = slot.end.format("%H:%M"),
            )?;
        }
        Ok(())
    })?;
    Ok(())
}

pub fn parse_hhmm_validate(s: &str) -> Result<chrono::NaiveTime, AppError> {
    parse_hhmm(s)
}

pub fn validate_range(from: NaiveDate, to: NaiveDate) -> Result<(), AppError> {
    super::events::validate_date_range(from, to)
}

pub fn validate_time_window(
    after: Option<NaiveTime>,
    before: Option<NaiveTime>,
) -> Result<(), AppError> {
    if let (Some(after), Some(before)) = (after, before) {
        if after >= before {
            return Err(AppError::InvalidArgument(
                "--after must be earlier than --before".to_string(),
            ));
        }
    }
    Ok(())
}

fn parse_hhmm(s: &str) -> Result<chrono::NaiveTime, AppError> {
    let (h_str, m_str) = s
        .split_once(':')
        .ok_or_else(|| AppError::InvalidDate(format!("{s} (expects HH:MM)")))?;
    let h: u32 = h_str
        .parse()
        .map_err(|_| AppError::InvalidDate(format!("{s} (expects HH:MM)")))?;
    let m: u32 = m_str
        .parse()
        .map_err(|_| AppError::InvalidDate(format!("{s} (expects HH:MM)")))?;
    chrono::NaiveTime::from_hms_opt(h, m, 0)
        .ok_or_else(|| AppError::InvalidDate(format!("{s} (invalid time)")))
}

fn format_slot_duration(mins: u32) -> String {
    if mins < 60 {
        format!("{mins}m")
    } else if mins % 60 == 0 {
        format!("{}h", mins / 60)
    } else {
        format!("{}h {}m", mins / 60, mins % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::validate_time_window;

    #[test]
    fn test_validate_time_window_rejects_reversed_bounds() {
        let after = chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap();
        let before = chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        assert!(validate_time_window(Some(after), Some(before)).is_err());
    }

    #[test]
    fn test_validate_time_window_rejects_equal_bounds() {
        let same = chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        assert!(validate_time_window(Some(same), Some(same)).is_err());
    }
}
