use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use chrono::Local;
use serde::Serialize;

#[derive(Serialize)]
struct NextEvent {
    title: String,
    start: String,
    end: String,
    calendar: String,
    minutes_until: i64,
}

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let now = Local::now();
    let today = now.date_naive();
    let events = store.events(today, today, calendar.as_deref())?;

    let next = events.iter().find(|e| !e.all_day && e.start > now);

    match next {
        Some(ev) => {
            let until = ev.start.signed_duration_since(now);
            let result = NextEvent {
                title: ev.title.clone(),
                start: ev.start.format("%H:%M").to_string(),
                end: ev.end.format("%H:%M").to_string(),
                calendar: ev.calendar.clone(),
                minutes_until: until.num_minutes(),
            };
            print_output(format, &result, |r| {
                let h = r.minutes_until / 60;
                let m = r.minutes_until % 60;
                let time_str = if h > 0 {
                    format!("{h}h {m}m")
                } else {
                    format!("{m}m")
                };
                println!("{} in {} ({} - {})", r.title, time_str, r.start, r.end);
            });
        }
        None => {
            if matches!(format, OutputFormat::Human) {
                println!("No more events today.");
            } else {
                println!("null");
            }
        }
    }
    Ok(())
}
