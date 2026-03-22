use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::{DateTime, Duration, Local};
use std::io::{self, Write};

use super::events::DisplayOpts;
use crate::store::EventInfo;

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    let now = Local::now();
    let today = now.date_naive();
    let lookahead = today + Duration::days(30);
    let events = store.events(today, lookahead, calendar.as_deref())?;
    let ev = select_next_event(&events, now);

    match ev {
        Some(ev) => {
            super::events::print_events(vec![ev.clone()], format, opts)?;
        }
        None => {
            if matches!(format, OutputFormat::Human) {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                writeln!(out, "No upcoming events.").map_err(|e| AppError::Io(e.to_string()))?;
            } else {
                let empty: Vec<EventInfo> = Vec::new();
                print_output(format, &empty, |_, _| Ok(()))?;
            }
        }
    }
    Ok(())
}

fn select_next_event(events: &[EventInfo], now: DateTime<Local>) -> Option<&EventInfo> {
    events
        .iter()
        .filter(|e| e.status != "canceled" && !e.all_day && e.start > now)
        .min_by_key(|e| e.start)
}

#[cfg(test)]
mod tests {
    use super::select_next_event;
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
            all_day: false,
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
    fn test_select_next_event_skips_canceled() {
        let now = local_dt(2026, 3, 20, 10, 0);
        let mut canceled = make_event(
            "Canceled",
            local_dt(2026, 3, 20, 10, 30),
            local_dt(2026, 3, 20, 11, 0),
        );
        canceled.status = "canceled".to_string();

        let active = make_event(
            "Confirmed",
            local_dt(2026, 3, 20, 11, 0),
            local_dt(2026, 3, 20, 12, 0),
        );

        let events = [canceled, active.clone()];
        let selected = select_next_event(&events, now).unwrap();
        assert_eq!(selected.title, active.title);
    }

    #[test]
    fn test_select_next_event_ignores_current_event() {
        let now = local_dt(2026, 3, 20, 10, 0);
        let current = make_event(
            "Current",
            local_dt(2026, 3, 20, 9, 30),
            local_dt(2026, 3, 20, 10, 30),
        );
        let upcoming = make_event(
            "Upcoming",
            local_dt(2026, 3, 20, 11, 0),
            local_dt(2026, 3, 20, 12, 0),
        );

        let events = [current, upcoming.clone()];
        let selected = select_next_event(&events, now).unwrap();
        assert_eq!(selected.title, upcoming.title);
    }
}
