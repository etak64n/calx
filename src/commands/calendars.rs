use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use std::io::IsTerminal;
use unicode_width::UnicodeWidthStr;

pub fn run(store: &CalendarStore, format: OutputFormat) -> Result<(), AppError> {
    let calendars = store.calendars();
    print_output(format, &calendars, |cals| {
        if cals.is_empty() {
            println!("No calendars found.");
            return;
        }

        let tty = std::io::stdout().is_terminal();
        let (bold, dim, reset) = if tty {
            ("\x1b[1m", "\x1b[2m", "\x1b[0m")
        } else {
            ("", "", "")
        };

        let title_w = cals
            .iter()
            .map(|c| UnicodeWidthStr::width(c.title.as_str()))
            .max()
            .unwrap_or(5)
            .max(5);

        if tty {
            println!("{dim}  {:<title_w$}  SOURCE{reset}", "TITLE",);
        }
        for cal in cals {
            let pad = title_w - UnicodeWidthStr::width(cal.title.as_str());
            let title_p = format!("{}{}", cal.title, " ".repeat(pad));
            println!("  {bold}{title_p}{reset}  {dim}{}{reset}", cal.source);
        }
    });
    Ok(())
}
