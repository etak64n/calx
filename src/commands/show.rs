use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;

pub fn run(store: &CalendarStore, event_id: &str, format: OutputFormat) -> Result<(), AppError> {
    let event = store.get_event(event_id)?;
    print_output(format, &event, |ev| {
        println!("Title:    {}", ev.title);
        println!("Calendar: {}", ev.calendar);
        if ev.all_day {
            println!("Date:     {} (All Day)", ev.start.format("%Y-%m-%d"));
        } else {
            println!("Start:    {}", ev.start.format("%Y-%m-%d %H:%M"));
            println!("End:      {}", ev.end.format("%Y-%m-%d %H:%M"));
        }
        if let Some(notes) = &ev.notes {
            println!("Notes:    {notes}");
        }
        println!("ID:       {}", ev.id);
    });
    Ok(())
}
