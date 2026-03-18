use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, EventInfo};
use chrono::{Duration, Local, NaiveDate};

pub fn run(
    store: &CalendarStore,
    from: Option<String>,
    to: Option<String>,
    calendar: Option<String>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let today = Local::now().date_naive();
    let from_date = match from {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|_| AppError::InvalidDate(s))?
        }
        None => today,
    };
    let to_date = match to {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|_| AppError::InvalidDate(s))?
        }
        None => from_date + Duration::days(7),
    };

    let events = store.events(from_date, to_date, calendar.as_deref())?;
    print_events(events, format);
    Ok(())
}

pub fn print_events(events: Vec<EventInfo>, format: OutputFormat) {
    print_output(format, &events, |evts| {
        if evts.is_empty() {
            println!("No events found.");
            return;
        }

        let mut current_date = String::new();
        for ev in evts {
            let date_str = ev.start.format("%A, %B %-d, %Y").to_string();
            if date_str != current_date {
                if !current_date.is_empty() {
                    println!();
                }
                println!("{date_str}");
                current_date = date_str;
            }

            if ev.all_day {
                println!("  [All Day]        {} ({})", ev.title, ev.calendar);
            } else {
                println!(
                    "  {} - {}  {} ({})",
                    ev.start.format("%H:%M"),
                    ev.end.format("%H:%M"),
                    ev.title,
                    ev.calendar,
                );
            }

            if let Some(notes) = &ev.notes {
                let first_line = notes.lines().next().unwrap_or("");
                if !first_line.is_empty() {
                    println!("                   {first_line}");
                }
            }
        }
    });
}
