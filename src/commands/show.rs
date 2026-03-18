use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    event_id: Option<&str>,
    query: Option<&str>,
    exact: bool,
    in_calendar: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    format: OutputFormat,
    no_color: bool,
) -> Result<(), AppError> {
    let event = super::select::resolve_event(store, event_id, query, exact, in_calendar, from, to)?;
    print_output(format, &event, |ev| {
        let (bold, dim, reset) = if !no_color {
            ("\x1b[1m", "\x1b[2m", "\x1b[0m")
        } else {
            ("", "", "")
        };

        let label_w = 14;
        let print_field = |label: &str, value: &str| {
            println!("{dim}{label:<label_w$}{reset}{bold}{value}{reset}");
        };

        print_field("Title:", &ev.title);
        print_field("Calendar:", &ev.calendar);
        if ev.all_day {
            let end_date = if ev.end.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
                && ev.end.date_naive() > ev.start.date_naive()
            {
                ev.end.date_naive() - chrono::Duration::days(1)
            } else {
                ev.end.date_naive()
            };
            let date = if end_date > ev.start.date_naive() {
                format!(
                    "{} to {} (All Day)",
                    ev.start.format("%Y-%m-%d"),
                    end_date.format("%Y-%m-%d")
                )
            } else {
                format!("{} (All Day)", ev.start.format("%Y-%m-%d"))
            };
            print_field("Date:", &date);
        } else {
            print_field("Start:", &ev.start.format("%Y-%m-%d %H:%M").to_string());
            print_field("End:", &ev.end.format("%Y-%m-%d %H:%M").to_string());
        }
        if ev.recurring {
            print_field("Recurring:", "yes");
        }
        if let Some(recurrence) = &ev.recurrence {
            print_field("Repeat:", recurrence);
        }
        if let Some(loc) = &ev.location {
            if !loc.is_empty() {
                print_field("Location:", loc);
            }
        }
        if let Some(url) = &ev.url {
            if !url.is_empty() {
                print_field("URL:", url);
            }
        }
        print_field("Status:", &ev.status);
        print_field("Availability:", &ev.availability);
        if let Some(org) = &ev.organizer {
            if !org.is_empty() {
                print_field("Organizer:", org);
            }
        }
        if let Some(notes) = &ev.notes {
            if !notes.is_empty() {
                print_field("Notes:", notes);
            }
        }
        if let Some(c) = &ev.created {
            print_field("Created:", &c.format("%Y-%m-%d %H:%M").to_string());
        }
        if let Some(m) = &ev.modified {
            print_field("Modified:", &m.format("%Y-%m-%d %H:%M").to_string());
        }
        print_field("ID:", &ev.id);
    });
    Ok(())
}
