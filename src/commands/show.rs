use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;

pub fn run(store: &CalendarStore, event_id: &str, format: OutputFormat) -> Result<(), AppError> {
    let event = store.get_event(event_id)?;
    print_output(format, &event, |ev| {
        println!("Title:        {}", ev.title);
        println!("Calendar:     {}", ev.calendar);
        if ev.all_day {
            println!("Date:         {} (All Day)", ev.start.format("%Y-%m-%d"));
        } else {
            println!("Start:        {}", ev.start.format("%Y-%m-%d %H:%M"));
            println!("End:          {}", ev.end.format("%Y-%m-%d %H:%M"));
        }
        if let Some(loc) = &ev.location {
            println!("Location:     {loc}");
        }
        if let Some(url) = &ev.url {
            println!("URL:          {url}");
        }
        println!("Status:       {}", ev.status);
        println!("Availability: {}", ev.availability);
        if let Some(org) = &ev.organizer {
            println!("Organizer:    {org}");
        }
        if let Some(notes) = &ev.notes {
            println!("Notes:        {notes}");
        }
        if let Some(c) = &ev.created {
            println!("Created:      {}", c.format("%Y-%m-%d %H:%M"));
        }
        if let Some(m) = &ev.modified {
            println!("Modified:     {}", m.format("%Y-%m-%d %H:%M"));
        }
        println!("ID:           {}", ev.id);
    });
    Ok(())
}
