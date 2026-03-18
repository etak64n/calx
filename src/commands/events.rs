use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, EventInfo};
use chrono::{Duration, Local, NaiveDate};
use std::io::IsTerminal;
use unicode_width::UnicodeWidthStr;

const TIME_W: usize = 15;

pub fn run(
    store: &CalendarStore,
    from: Option<String>,
    to: Option<String>,
    calendar: Option<String>,
    format: OutputFormat,
    verbose: bool,
    fields: Option<&str>,
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
    print_events(events, format, verbose, fields);
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

pub fn print_events(
    events: Vec<EventInfo>,
    format: OutputFormat,
    verbose: bool,
    fields: Option<&str>,
) {
    // For structured formats, filter fields if specified
    if fields.is_some() && !matches!(format, OutputFormat::Human) {
        let field_list: Vec<&str> = fields.unwrap().split(',').map(|s| s.trim()).collect();
        let filtered = filter_fields(&events, &field_list);
        match format {
            OutputFormat::Human => {}
            _ => {
                // Re-serialize filtered data
                println!(
                    "{}",
                    serde_json::to_string_pretty(&filtered).unwrap_or_default()
                );
                return;
            }
        }
    }

    print_output(format, &events, |evts| {
        let tty = std::io::stdout().is_terminal();

        if evts.is_empty() {
            if tty {
                println!("No events found.");
            }
            return;
        }

        let (bold, dim, reset, green, cyan) = if tty {
            ("\x1b[1m", "\x1b[2m", "\x1b[0m", "\x1b[32m", "\x1b[36m")
        } else {
            ("", "", "", "", "")
        };

        let now = Local::now();

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
        let notes_indent = " ".repeat(2 + 3 + 2 + TIME_W + 2);

        for ev in evts {
            let date_str = ev.start.format("%A, %B %-d, %Y").to_string();
            if date_str != current_date {
                if !current_date.is_empty() {
                    println!();
                }
                println!("{bold}{date_str}{reset}");
                if tty {
                    println!(
                        "{dim}  {:>3}  {:<TIME_W$}  {:<title_w$}  {:<cal_w$}  DURATION{reset}",
                        "#", "TIME", "TITLE", "CALENDAR",
                    );
                }
                current_date = date_str;
            }

            let is_past = ev.end < now;
            let is_now = ev.start <= now && ev.end > now;
            let duration = format_duration(ev.end.signed_duration_since(ev.start));
            let title_p = pad_right(&ev.title, title_w);
            let cal_p = pad_right(&ev.calendar, cal_w);

            if ev.all_day {
                let time_str = if tty {
                    "┄┄┄ all day ┄┄┄".to_string()
                } else {
                    pad_right("all day", TIME_W)
                };
                if is_past {
                    println!("{dim}  {row:>3}  {time_str}  {title_p}  {cal_p}  {duration}{reset}");
                } else {
                    println!(
                        "  {row:>3}  {cyan}{time_str}{reset}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{duration}{reset}"
                    );
                }
            } else {
                let time_str = format!(
                    "{} {} {}",
                    ev.start.format("%H:%M"),
                    if tty { "\u{2013}" } else { "-" },
                    ev.end.format("%H:%M"),
                );
                let time_p = pad_right(&time_str, TIME_W);

                if is_past {
                    println!("{dim}  {row:>3}  {time_p}  {title_p}  {cal_p}  {duration}{reset}");
                } else if is_now {
                    println!(
                        "  {row:>3}  {green}{bold}{time_p}{reset}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{duration}{reset}"
                    );
                } else {
                    println!(
                        "  {row:>3}  {time_p}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{duration}{reset}"
                    );
                }
            }

            // Notes: always show in verbose, first line only otherwise
            if let Some(notes) = &ev.notes {
                if verbose {
                    for line in notes.lines() {
                        println!("{notes_indent}{dim}{line}{reset}");
                    }
                } else {
                    let first_line = notes.lines().next().unwrap_or("");
                    if !first_line.is_empty() {
                        println!("{notes_indent}{dim}{first_line}{reset}");
                    }
                }
            }

            // Verbose: show ID
            if verbose {
                println!("{notes_indent}{dim}ID: {}{reset}", ev.id);
            }

            row += 1;
        }
    });
}

fn filter_fields(
    events: &[EventInfo],
    fields: &[&str],
) -> Vec<serde_json::Map<String, serde_json::Value>> {
    events
        .iter()
        .filter_map(|ev| {
            let val = serde_json::to_value(ev).ok()?;
            let obj = val.as_object()?;
            let mut filtered = serde_json::Map::new();
            for &f in fields {
                if let Some(v) = obj.get(f) {
                    filtered.insert(f.to_string(), v.clone());
                }
            }
            Some(filtered)
        })
        .collect()
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
