use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::{Duration, Local};
use serde::Serialize;

#[derive(Serialize)]
struct NextEvent {
    title: String,
    date: String,
    start: String,
    end: String,
    calendar: String,
    minutes_until: i64,
    is_now: bool,
}

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let now = Local::now();
    let today = now.date_naive();
    let lookahead = today + Duration::days(30);
    let events = store.events(today, lookahead, calendar.as_deref())?;

    // Find current or next upcoming event
    let current = events
        .iter()
        .find(|e| !e.all_day && e.start <= now && e.end > now);
    let next = events.iter().find(|e| !e.all_day && e.start > now);

    let ev = current.or(next);

    match ev {
        Some(ev) => {
            let is_now = ev.start <= now && ev.end > now;
            let until = ev.start.signed_duration_since(now);
            let result = NextEvent {
                title: ev.title.clone(),
                date: ev.start.format("%Y-%m-%d").to_string(),
                start: ev.start.format("%H:%M").to_string(),
                end: ev.end.format("%H:%M").to_string(),
                calendar: ev.calendar.clone(),
                minutes_until: if is_now { 0 } else { until.num_minutes() },
                is_now,
            };
            print_output(format, &result, |r| {
                if r.is_now {
                    println!(
                        "{} (now, until {}) {} {}",
                        r.title, r.end, r.date, r.calendar
                    );
                } else {
                    let h = r.minutes_until / 60;
                    let m = r.minutes_until % 60;
                    let time_str = if h >= 24 {
                        let d = h / 24;
                        format!("{d}d {h}h", h = h % 24)
                    } else if h > 0 {
                        format!("{h}h {m}m")
                    } else {
                        format!("{m}m")
                    };
                    println!(
                        "{} in {} ({} - {}) {} {}",
                        r.title, time_str, r.start, r.end, r.date, r.calendar
                    );
                }
            });
        }
        None => {
            if matches!(format, OutputFormat::Human) {
                println!("No upcoming events.");
            } else {
                println!("null");
            }
        }
    }
    Ok(())
}
