use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;

pub fn run(store: &CalendarStore, format: OutputFormat) -> Result<(), AppError> {
    let calendars = store.calendars();
    print_output(format, &calendars, |cals| {
        if cals.is_empty() {
            println!("No calendars found.");
            return;
        }
        for cal in cals {
            println!("{} ({})", cal.title, cal.source);
        }
    });
    Ok(())
}
