use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, EventInfo};
use chrono::{DateTime, Duration, Local, NaiveDate, Timelike};
use unicode_width::UnicodeWidthStr;

const TIME_W: usize = 15;
pub const EVENT_FIELD_NAMES: &[&str] = &[
    "id",
    "title",
    "start",
    "end",
    "calendar",
    "calendar_id",
    "location",
    "url",
    "notes",
    "all_day",
    "status",
    "availability",
    "organizer",
    "created",
    "modified",
    "recurring",
    "recurrence",
    "recurrence_rule",
    "alerts",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventDisplayState {
    Past,
    Now,
    AllDay,
    Upcoming,
}

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
    validate_date_range(from_date, to_date)?;

    let events = store.events(from_date, to_date, calendar.as_deref())?;
    print_events(events, format, opts)
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
            return Err(AppError::InvalidArgument(format!(
                "{after} (--after expects HH:MM)"
            )));
        }
    }
    if let Some(before) = opts.before {
        if parse_hhmm(before).is_none() {
            return Err(AppError::InvalidArgument(format!(
                "{before} (--before expects HH:MM)"
            )));
        }
    }
    if let Some(sort_key) = opts.sort {
        match sort_key {
            "date" | "start" | "title" | "calendar" | "duration" => {}
            _ => {
                return Err(AppError::InvalidArgument(format!(
                    "Unknown sort key: {sort_key}. Use date, start, title, calendar, or duration."
                )));
            }
        }
    }
    Ok(())
}

pub fn validate_date_range(from: NaiveDate, to: NaiveDate) -> Result<(), AppError> {
    if to < from {
        return Err(AppError::InvalidDate(
            "to date must be on or after from date".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_field_list(fields: &str) -> Result<(), AppError> {
    let field_list: Vec<&str> = fields.split(',').map(|field| field.trim()).collect();
    if field_list.is_empty() || field_list.iter().any(|field| field.is_empty()) {
        return Err(AppError::InvalidArgument(
            "--fields must be a comma-separated list of field names".to_string(),
        ));
    }

    let invalid: Vec<&str> = field_list
        .iter()
        .copied()
        .filter(|field| !EVENT_FIELD_NAMES.contains(field))
        .collect();
    if !invalid.is_empty() {
        return Err(AppError::InvalidArgument(format!(
            "Unknown field(s): {}. Available fields: {}",
            invalid.join(", "),
            EVENT_FIELD_NAMES.join(", ")
        )));
    }

    Ok(())
}

pub fn print_events(
    events: Vec<EventInfo>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    let mut events = events;
    events = visible_events(events);

    let after_time = opts.after.and_then(parse_hhmm);
    let before_time = opts.before.and_then(parse_hhmm);
    if after_time.is_some() || before_time.is_some() {
        events.retain(|e| event_matches_time_filters(e, after_time, before_time));
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
        return print_output(format, &filtered, |_, _| Ok(()));
    }

    print_output(format, &events, |evts, out| {
        if evts.is_empty() {
            writeln!(out, "No events found.")?;
            return Ok(());
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
                writeln!(
                    out,
                    "{dim}  {:>3}  {:<date_w$}  {:<TIME_W$}  {:<title_w$}  {:<cal_w$}  {:<dur_w$}  {:<notes_w$}  ID{reset}",
                    "#", "DATE", "TIME", "TITLE", "CALENDAR", "DURATION", "NOTES",
                )?;
            } else {
                writeln!(
                    out,
                    "{dim}  {:>3}  {:<date_w$}  {:<TIME_W$}  {:<title_w$}  {:<cal_w$}  DURATION{reset}",
                    "#", "DATE", "TIME", "TITLE", "CALENDAR",
                )?;
            }
        }

        for ev in evts {
            let date_p = ev.start.format("%Y-%m-%d").to_string();

            let display_state = event_display_state(ev, now);
            let duration = format_event_duration(ev);
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

            match display_state {
                EventDisplayState::Past => {
                    writeln!(
                        out,
                        "{dim}  {row:>3}  {date_p}  {time_p}  {title_p}  {cal_p}  {dur_p}{verbose_suffix}{reset}"
                    )?;
                }
                EventDisplayState::Now => {
                    writeln!(
                        out,
                        "  {row:>3}  {date_p}  {green}{bold}{time_p}{reset}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{dur_p}{verbose_suffix}{reset}"
                    )?;
                }
                EventDisplayState::AllDay => {
                    writeln!(
                        out,
                        "  {row:>3}  {date_p}  {cyan}{time_p}{reset}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{dur_p}{verbose_suffix}{reset}"
                    )?;
                }
                EventDisplayState::Upcoming => {
                    writeln!(
                        out,
                        "  {row:>3}  {date_p}  {time_p}  {bold}{title_p}{reset}  {dim}{cal_p}{reset}  {dim}{dur_p}{verbose_suffix}{reset}"
                    )?;
                }
            }

            row += 1;
        }
        Ok(())
    })
}

fn visible_events(events: Vec<EventInfo>) -> Vec<EventInfo> {
    events
        .into_iter()
        .filter(|event| event.status != "canceled")
        .collect()
}

fn event_display_state(event: &EventInfo, now: DateTime<Local>) -> EventDisplayState {
    if event.all_day {
        EventDisplayState::AllDay
    } else if event.end < now {
        EventDisplayState::Past
    } else if event.start <= now && event.end > now {
        EventDisplayState::Now
    } else {
        EventDisplayState::Upcoming
    }
}

fn event_matches_time_filters(
    event: &EventInfo,
    after: Option<chrono::NaiveTime>,
    before: Option<chrono::NaiveTime>,
) -> bool {
    if event.all_day || event.end.date_naive() > event.start.date_naive() {
        return multi_day_event_matches_time_filters(event, after, before);
    }

    after.is_none_or(|time| event.end.time() > time)
        && before.is_none_or(|time| event.start.time() < time)
}

fn multi_day_event_matches_time_filters(
    event: &EventInfo,
    after: Option<chrono::NaiveTime>,
    before: Option<chrono::NaiveTime>,
) -> bool {
    if event.all_day {
        return true;
    }

    let filter_start = after.map(seconds_since_midnight).unwrap_or(0);
    let filter_end = before.map(seconds_since_midnight).unwrap_or(24 * 60 * 60);

    if filter_start >= filter_end {
        return false;
    }

    if time_segment_overlaps(
        seconds_since_midnight(event.start.time()),
        24 * 60 * 60,
        filter_start,
        filter_end,
    ) {
        return true;
    }

    let day_span = (event.end.date_naive() - event.start.date_naive()).num_days();
    if day_span > 1 {
        return true;
    }

    time_segment_overlaps(
        0,
        seconds_since_midnight(event.end.time()),
        filter_start,
        filter_end,
    )
}

fn time_segment_overlaps(
    segment_start: u32,
    segment_end: u32,
    filter_start: u32,
    filter_end: u32,
) -> bool {
    segment_start < filter_end && segment_end > filter_start
}

fn seconds_since_midnight(time: chrono::NaiveTime) -> u32 {
    time.num_seconds_from_midnight()
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

fn format_event_duration(event: &EventInfo) -> String {
    if event.all_day {
        let days = (event.end.date_naive() - event.start.date_naive())
            .num_days()
            .max(1);
        return format!("{days}d");
    }
    format_duration(event.end.signed_duration_since(event.start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EventInfo;
    use chrono::{Local, NaiveDate, NaiveTime, TimeZone};

    fn make_event(title: &str, calendar: &str, notes: Option<&str>) -> EventInfo {
        let start = Local.with_ymd_and_hms(2026, 3, 20, 14, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 3, 20, 15, 0, 0).unwrap();
        EventInfo {
            id: "test-id".to_string(),
            title: title.to_string(),
            start,
            end,
            calendar: calendar.to_string(),
            calendar_id: "cal-1".to_string(),
            location: None,
            url: None,
            notes: notes.map(|s| s.to_string()),
            all_day: false,
            status: "confirmed".to_string(),
            availability: "busy".to_string(),
            organizer: None,
            created: None,
            modified: None,
            recurring: false,
            recurrence: None,
            recurrence_rule: None,
            alerts: Vec::new(),
        }
    }

    fn local_dt(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> chrono::DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap();
        Local.from_local_datetime(&naive).earliest().unwrap()
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
    fn test_validate_field_list_rejects_unknown_field() {
        let err = validate_field_list("title,titel").unwrap_err();
        assert!(err.to_string().contains("Unknown field(s): titel"));
    }

    #[test]
    fn test_validate_field_list_rejects_empty_field_name() {
        let err = validate_field_list("title,,start").unwrap_err();
        assert!(
            err.to_string()
                .contains("--fields must be a comma-separated list of field names")
        );
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

    #[test]
    fn test_visible_events_filters_canceled() {
        let mut canceled = make_event("Canceled", "Work", None);
        canceled.status = "canceled".to_string();
        let visible = make_event("Visible", "Work", None);

        let events = visible_events(vec![canceled, visible.clone()]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, visible.title);
    }

    #[test]
    fn test_event_display_state_prefers_all_day_over_now() {
        let now = local_dt(2026, 3, 20, 12, 0);
        let mut event = make_event("Holiday", "Work", None);
        event.all_day = true;
        event.start = local_dt(2026, 3, 20, 0, 0);
        event.end = local_dt(2026, 3, 21, 0, 0);

        assert_eq!(event_display_state(&event, now), EventDisplayState::AllDay);
    }

    #[test]
    fn test_event_matches_time_filters_uses_overlap_after() {
        let mut event = make_event("Overlap", "Work", None);
        event.start = local_dt(2026, 3, 20, 8, 30);
        event.end = local_dt(2026, 3, 20, 10, 0);

        assert!(event_matches_time_filters(
            &event,
            Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            None
        ));
    }

    #[test]
    fn test_event_matches_time_filters_uses_overlap_before() {
        let mut event = make_event("Overlap", "Work", None);
        event.start = local_dt(2026, 3, 20, 16, 30);
        event.end = local_dt(2026, 3, 20, 18, 30);

        assert!(event_matches_time_filters(
            &event,
            None,
            Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap())
        ));
    }

    #[test]
    fn test_event_matches_time_filters_excludes_non_overlapping_event() {
        let mut event = make_event("Late", "Work", None);
        event.start = local_dt(2026, 3, 20, 18, 0);
        event.end = local_dt(2026, 3, 20, 19, 0);

        assert!(!event_matches_time_filters(
            &event,
            Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap())
        ));
    }

    #[test]
    fn test_event_matches_time_filters_handles_cross_midnight_overlap() {
        let mut event = make_event("Overnight", "Work", None);
        event.start = local_dt(2026, 3, 20, 23, 30);
        event.end = local_dt(2026, 3, 21, 0, 15);

        assert!(event_matches_time_filters(
            &event,
            None,
            Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap())
        ));
        assert!(event_matches_time_filters(
            &event,
            Some(NaiveTime::from_hms_opt(22, 0, 0).unwrap()),
            None
        ));
        assert!(!event_matches_time_filters(
            &event,
            Some(NaiveTime::from_hms_opt(1, 0, 0).unwrap()),
            Some(NaiveTime::from_hms_opt(22, 0, 0).unwrap())
        ));
    }
}
