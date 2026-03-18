use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, EventInfo};
use chrono::{Duration, Local, NaiveDate};
use unicode_width::UnicodeWidthStr;

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";

const TIME_W: usize = 15; // "HH:MM – HH:MM  " or "┄┄┄ all day ┄┄┄"

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

fn pad_right(s: &str, width: usize) -> String {
    let display_w = UnicodeWidthStr::width(s);
    if display_w >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - display_w))
    }
}

pub fn print_events(events: Vec<EventInfo>, format: OutputFormat) {
    print_output(format, &events, |evts| {
        if evts.is_empty() {
            println!("{DIM}No events found.{RESET}");
            return;
        }

        let now = Local::now();

        // Calculate column widths
        let title_w = evts
            .iter()
            .map(|e| UnicodeWidthStr::width(e.title.as_str()))
            .max()
            .unwrap_or(5)
            .clamp(5, 50);
        let cal_w = evts
            .iter()
            .map(|e| UnicodeWidthStr::width(e.calendar.as_str()))
            .max()
            .unwrap_or(8)
            .clamp(8, 30);

        let mut current_date = String::new();
        let mut row = 1;

        // notes indent = "  " + ### + "  " + TIME_W + "  " = 2+3+2+15+2 = 24
        let notes_indent = " ".repeat(2 + 3 + 2 + TIME_W + 2);

        for ev in evts {
            let date_str = ev.start.format("%A, %B %-d, %Y").to_string();
            if date_str != current_date {
                if !current_date.is_empty() {
                    println!();
                }
                println!("{BOLD}{date_str}{RESET}");
                println!(
                    "{DIM}  {:>3}  {:<TIME_W$}  {:<title_w$}  {:<cal_w$}  DURATION{RESET}",
                    "#", "TIME", "TITLE", "CALENDAR",
                );
                current_date = date_str;
            }

            let is_past = ev.end < now;
            let is_now = ev.start <= now && ev.end > now;

            let duration = format_duration(ev.end.signed_duration_since(ev.start));
            let title_p = pad_right(&ev.title, title_w);
            let cal_p = pad_right(&ev.calendar, cal_w);

            if ev.all_day {
                let time_str = "┄┄┄ all day ┄┄┄";
                if is_past {
                    println!("{DIM}  {row:>3}  {time_str}  {title_p}  {cal_p}  {duration}{RESET}");
                } else {
                    println!(
                        "  {row:>3}  {CYAN}{time_str}{RESET}  {BOLD}{title_p}{RESET}  {DIM}{cal_p}{RESET}  {DIM}{duration}{RESET}"
                    );
                }
            } else {
                let time_str = format!("{} – {}", ev.start.format("%H:%M"), ev.end.format("%H:%M"));
                let time_p = pad_right(&time_str, TIME_W);

                if is_past {
                    println!("{DIM}  {row:>3}  {time_p}  {title_p}  {cal_p}  {duration}{RESET}");
                } else if is_now {
                    println!(
                        "  {row:>3}  {GREEN}{BOLD}{time_p}{RESET}  {BOLD}{title_p}{RESET}  {DIM}{cal_p}{RESET}  {DIM}{duration}{RESET}"
                    );
                } else {
                    println!(
                        "  {row:>3}  {time_p}  {BOLD}{title_p}{RESET}  {DIM}{cal_p}{RESET}  {DIM}{duration}{RESET}"
                    );
                }
            }

            if let Some(notes) = &ev.notes {
                let first_line = notes.lines().next().unwrap_or("");
                if !first_line.is_empty() {
                    println!("{notes_indent}{DIM}{first_line}{RESET}");
                }
            }

            row += 1;
        }
    });
}

fn format_duration(d: chrono::Duration) -> String {
    let mins = d.num_minutes();
    if mins < 60 {
        format!("{mins}m")
    } else if mins % 60 == 0 {
        format!("{}h", mins / 60)
    } else {
        format!("{}h {}m", mins / 60, mins % 60)
    }
}
