use crate::dateparse;
use crate::error::AppError;
use crate::store::{CalendarStore, EventInfo};
use chrono::{Duration, Local};

pub fn run(
    store: &CalendarStore,
    format: &str,
    from: Option<String>,
    to: Option<String>,
    calendar: Option<String>,
) -> Result<(), AppError> {
    let today = Local::now().date_naive();
    let from_date = from
        .and_then(|s| dateparse::parse_date(&s))
        .unwrap_or(today);
    let to_date = to
        .and_then(|s| dateparse::parse_date(&s))
        .unwrap_or(from_date + Duration::days(30));

    let events = store.events(from_date, to_date, calendar.as_deref())?;

    match format {
        "ics" => print_ics(&events),
        "csv" => print_csv(&events)?,
        _ => {
            return Err(AppError::EventKit(format!(
                "Unknown format: {format}. Use ics or csv."
            )));
        }
    }
    Ok(())
}

fn print_ics(events: &[EventInfo]) {
    println!("BEGIN:VCALENDAR");
    println!("VERSION:2.0");
    println!("PRODID:-//calx//EN");
    for ev in events {
        println!("BEGIN:VEVENT");
        println!("UID:{}", ev.id);
        if ev.all_day {
            println!("DTSTART;VALUE=DATE:{}", ev.start.format("%Y%m%d"));
            println!("DTEND;VALUE=DATE:{}", ev.end.format("%Y%m%d"));
        } else {
            println!("DTSTART:{}", ev.start.format("%Y%m%dT%H%M%S"));
            println!("DTEND:{}", ev.end.format("%Y%m%dT%H%M%S"));
        }
        println!("SUMMARY:{}", ev.title);
        if let Some(notes) = &ev.notes {
            let escaped = notes.replace('\n', "\\n");
            println!("DESCRIPTION:{escaped}");
        }
        println!("END:VEVENT");
    }
    println!("END:VCALENDAR");
}

fn print_csv(events: &[EventInfo]) -> Result<(), AppError> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    wtr.write_record([
        "id", "title", "start", "end", "calendar", "all_day", "notes",
    ])
    .map_err(|e| AppError::EventKit(e.to_string()))?;
    for ev in events {
        wtr.write_record([
            &ev.id,
            &ev.title,
            &ev.start.to_rfc3339(),
            &ev.end.to_rfc3339(),
            &ev.calendar,
            &ev.all_day.to_string(),
            &ev.notes.clone().unwrap_or_default(),
        ])
        .map_err(|e| AppError::EventKit(e.to_string()))?;
    }
    wtr.flush().map_err(|e| AppError::EventKit(e.to_string()))?;
    Ok(())
}
