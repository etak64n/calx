use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use unicode_width::UnicodeWidthStr;

use super::events::DisplayOpts;

pub fn run(
    store: &CalendarStore,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    let calendars = store.calendars();
    print_output(format, &calendars, |cals, out| {
        if cals.is_empty() {
            writeln!(out, "No calendars found.")?;
            return Ok(());
        }

        let (bold, dim, reset) = if !opts.no_color {
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
        let id_w = cals
            .iter()
            .map(|c| UnicodeWidthStr::width(c.id.as_str()))
            .max()
            .unwrap_or(2)
            .max(2);

        if !opts.no_header {
            let title_pad = title_w - 5; // "TITLE".len()
            if opts.verbose {
                let id_pad = id_w - 2; // "ID".len()
                writeln!(
                    out,
                    "{dim}  TITLE{}  SOURCE  ID{}{reset}",
                    " ".repeat(title_pad),
                    " ".repeat(id_pad)
                )?;
            } else {
                writeln!(out, "{dim}  TITLE{}  SOURCE{reset}", " ".repeat(title_pad))?;
            }
        }
        for cal in cals {
            let pad = title_w - UnicodeWidthStr::width(cal.title.as_str());
            let title_p = format!("{}{}", cal.title, " ".repeat(pad));
            if opts.verbose {
                let id_pad = id_w - UnicodeWidthStr::width(cal.id.as_str());
                let id_p = format!("{}{}", cal.id, " ".repeat(id_pad));
                writeln!(
                    out,
                    "  {bold}{title_p}{reset}  {dim}{}{reset}  {dim}{}{reset}",
                    cal.source, id_p
                )?;
            } else {
                writeln!(out, "  {bold}{title_p}{reset}  {dim}{}{reset}", cal.source)?;
            }
        }
        Ok(())
    })?;
    Ok(())
}
