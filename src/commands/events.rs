use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, EventInfo};
use chrono::{Duration, Local};
use unicode_width::UnicodeWidthStr;

const TIME_W: usize = 15;

pub fn run(
    store: &CalendarStore,
    from: Option<String>,
    to: Option<String>,
    calendar: Option<String>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    validate_opts(opts)?;
    let today = Local::now().date_naive();
    let from_date = match from {
        Some(s) => dateparse::parse_date(&s).ok_or(AppError::InvalidDate(s))?,
        None => today,
    };
    let to_date = match to {
        Some(s) => dateparse::parse_date(&s).ok_or(AppError::InvalidDate(s))?,
        None => from_date + Duration::days(7),
    };

    let events = store.events(from_date, to_date, calendar.as_deref())?;
    print_events(events, format, opts);
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

#[allow(clippy::too_many_arguments)]
#[derive(Default)]
pub struct DisplayOpts<'a> {
    pub verbose: bool,
    pub fields: Option<&'a str>,
    pub no_color: bool,
    pub no_header: bool,
    pub sort: Option<&'a str>,
    pub limit: Option<usize>,
    pub after: Option<&'a str>,
    pub before: Option<&'a str>,
}

/// Validate DisplayOpts values. Call before print_events.
pub fn validate_opts(opts: &DisplayOpts) -> Result<(), AppError> {
    if let Some(after) = opts.after {
        if parse_hhmm(after).is_none() {
            return Err(AppError::InvalidDate(format!(
                "{after} (--after expects HH:MM)"
            )));
        }
    }
    if let Some(before) = opts.before {
        if parse_hhmm(before).is_none() {
            return Err(AppError::InvalidDate(format!(
                "{before} (--before expects HH:MM)"
            )));
        }
    }
    if let Some(sort_key) = opts.sort {
        match sort_key {
            "date" | "start" | "title" | "calendar" | "duration" => {}
            _ => {
                return Err(AppError::InvalidDate(format!(
                    "Unknown sort key: {sort_key}. Use date, title, calendar, or duration."
                )));
            }
        }
    }
    Ok(())
}

pub fn print_events(events: Vec<EventInfo>, format: OutputFormat, opts: &DisplayOpts) {
    let mut events = events;

    if let Some(after) = opts.after {
        if let Some(t) = parse_hhmm(after) {
            events.retain(|e| e.start.time() >= t || e.all_day);
        }
    }
    if let Some(before) = opts.before {
        if let Some(t) = parse_hhmm(before) {
            events.retain(|e| e.start.time() < t || e.all_day);
        }
    }

    if let Some(sort_key) = opts.sort {
        match sort_key {
            "date" | "start" => events.sort_by_key(|e| e.start),
            "title" => events.sort_by(|a, b| a.title.cmp(&b.title)),
            "calendar" => events.sort_by(|a, b| a.calendar.cmp(&b.calendar)),
            "duration" => events.sort_by_key(|e| e.end.signed_duration_since(e.start)),
            _ => {}
        }
    }

    // Apply limit
    if let Some(limit) = opts.limit {
        events.truncate(limit);
    }

    // For structured formats, filter fields if specified
    if opts.fields.is_some() && !matches!(format, OutputFormat::Human) {
        let field_list: Vec<&str> = opts.fields.unwrap().split(',').map(|s| s.trim()).collect();
        let filtered = filter_fields(&events, &field_list);
        print_output(format, &filtered, |_| {});
        return;
    }

    print_output(format, &events, |evts| {
        if evts.is_empty() {
            println!("No events found.");
            return;
        }

        let color = !opts.no_color;
        let (bold, dim, reset, green, cyan) = if color {
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
        let dur_w = 8;
        let verbose = opts.verbose;
        let notes_w = if verbose {
            evts.iter()
                .filter_map(|e| e.notes.as_ref())
                .map(|n| UnicodeWidthStr::width(n.lines().next().unwrap_or("")))
                .max()
                .unwrap_or(5)
                .clamp(5, 40)
        } else {
            0
        };

        let date_w = 10; // "YYYY-MM-DD"
        let mut row = 1;

        if !opts.no_header {
            if verbose {
                println!(
                    "{dim}  {:>3}  {:<date_w$}  {:<TIME_W$}  {:<title_w$}  {:<cal_w$}  {:<dur_w$}  {:<notes_w$}  ID{reset}",
                    "#", "DATE", "TIME", "TITLE", "CALENDAR", "DURATION", "NOTES",
                );
            } else {
                println!(
                    "{dim}  {:>3}  {:<date_w$}  {:<TIME_W$}  {:<title_w$}  {:<cal_w$}  DURATION{reset}",
                    "#", "DATE", "TIME", "TITLE", "CALENDAR",
                );
            }
        }

        for ev in evts {
            let date_p = ev.start.format("%Y-%m-%d").to_string();

            let is_past = ev.end < now;
            let is_now = ev.start <= now && ev.end > now;
            let duration = format_duration(ev.end.signed_duration_since(ev.start));
            let title_p = pad_right(&ev.title, title_w);
            let cal_p = pad_right(&ev.calendar, cal_w);

            let time_str = if ev.all_day {
                if color {
                    "\u{2504}\u{2504}\u{2504} all day \u{2504}\u{2504}\u{2504}".to_string()
                } else {
                    "all day".to_string()
                }
            } else {
                format!(
                    "{} {} {}",
                    ev.start.format("%H:%M"),
                    if color { "\u{2013}" } else { "-" },
                    ev.end.format("%H:%M"),
                )
            };
            let time_p = pad_right(&time_str, TIME_W);

            let verbose_suffix = if verbose {
                let notes_str = ev
                    .notes
                    .as_ref()
                    .and_then(|n| n.lines().next())
                    .unwrap_or("");
                let notes_p = pad_right(notes_str, notes_w);
                format!("  {notes_p}  {}", ev.id)
            } else {
                String::new()
            };

            let dur_p = if verbose {
                pad_right(&duration, dur_w)
            } else {
                duration.clone()
            };

            if is_past {
                println!(
                    "{dim}  {row:>3}  {date_p}  {time_p}  {title_p}  {cal_p}  {dur_p}{verbose_suffix}{reset}"
                );
            } else if is_now {
                println!(
                    "  {row:>3}  {date_p}  {green}{bold}{time_p}{reset}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{dur_p}{verbose_suffix}{reset}"
                );
            } else if ev.all_day {
                println!(
                    "  {row:>3}  {date_p}  {cyan}{time_p}{reset}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{dur_p}{verbose_suffix}{reset}"
                );
            } else {
                println!(
                    "  {row:>3}  {date_p}  {time_p}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{dur_p}{verbose_suffix}{reset}"
                );
            }

            row += 1;
        }
    });
}

fn parse_hhmm(s: &str) -> Option<chrono::NaiveTime> {
    let (h_str, m_str) = s.split_once(':')?;
    let h: u32 = h_str.parse().ok()?;
    let m: u32 = m_str.parse().ok()?;
    chrono::NaiveTime::from_hms_opt(h, m, 0)
}

pub(crate) fn filter_fields(
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

pub(crate) fn format_duration(d: chrono::Duration) -> String {
    let mins = d.num_minutes();
    if mins < 60 {
        format!("{mins}m")
    } else if mins % 60 == 0 {
        format!("{}h", mins / 60)
    } else {
        format!("{}h {}m", mins / 60, mins % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EventInfo;
    use chrono::{Local, TimeZone};

    fn make_event(title: &str, calendar: &str, notes: Option<&str>) -> EventInfo {
        let start = Local.with_ymd_and_hms(2026, 3, 20, 14, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 3, 20, 15, 0, 0).unwrap();
        EventInfo {
            id: "test-id".to_string(),
            title: title.to_string(),
            start,
            end,
            calendar: calendar.to_string(),
            location: None,
            url: None,
            notes: notes.map(|s| s.to_string()),
            all_day: false,
            status: "confirmed".to_string(),
            availability: "busy".to_string(),
            organizer: None,
            created: None,
            modified: None,
        }
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(chrono::Duration::minutes(30)), "30m");
        assert_eq!(format_duration(chrono::Duration::minutes(5)), "5m");
        assert_eq!(format_duration(chrono::Duration::minutes(0)), "0m");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(chrono::Duration::hours(1)), "1h");
        assert_eq!(format_duration(chrono::Duration::hours(3)), "3h");
    }

    #[test]
    fn test_format_duration_mixed() {
        assert_eq!(format_duration(chrono::Duration::minutes(90)), "1h 30m");
        assert_eq!(format_duration(chrono::Duration::minutes(150)), "2h 30m");
    }

    #[test]
    fn test_filter_fields_subset() {
        let events = vec![make_event("Meeting", "Work", None)];
        let result = filter_fields(&events, &["title", "calendar"]);
        assert_eq!(result.len(), 1);
        assert!(result[0].contains_key("title"));
        assert!(result[0].contains_key("calendar"));
        assert!(!result[0].contains_key("id"));
        assert!(!result[0].contains_key("start"));
    }

    #[test]
    fn test_filter_fields_all() {
        let events = vec![make_event("Meeting", "Work", Some("notes"))];
        let result = filter_fields(
            &events,
            &[
                "id", "title", "start", "end", "calendar", "notes", "all_day",
            ],
        );
        assert_eq!(result[0].len(), 7);
    }

    #[test]
    fn test_filter_fields_nonexistent() {
        let events = vec![make_event("Meeting", "Work", None)];
        let result = filter_fields(&events, &["nonexistent"]);
        assert!(result[0].is_empty());
    }

    #[test]
    fn test_filter_fields_empty_events() {
        let events: Vec<EventInfo> = vec![];
        let result = filter_fields(&events, &["title"]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_pad_right_ascii() {
        assert_eq!(pad_right("abc", 6), "abc   ");
        assert_eq!(pad_right("abc", 3), "abc");
        assert_eq!(pad_right("abc", 1), "abc");
    }

    #[test]
    fn test_pad_right_cjk() {
        assert_eq!(pad_right("会議", 6), "会議  ");
        assert_eq!(pad_right("会議", 4), "会議");
    }
}
