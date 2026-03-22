use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, EventInfo};
use chrono::Local;
use std::io::{self, Write};

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
    no_color: bool,
) -> Result<(), AppError> {
    let now = Local::now();
    let today = now.date_naive();
    let events: Vec<EventInfo> = store
        .events(today, today, calendar.as_deref())?
        .into_iter()
        .filter(|event| event.status != "canceled")
        .collect();
    let ordered_events = ordered_agenda_events(&events);

    // For structured formats, use standard serialization
    if !matches!(format, OutputFormat::Human) {
        print_output(format, &ordered_events, |_, _| Ok(()))?;
        return Ok(());
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if events.is_empty() {
        writeln!(out, "No events today.").map_err(|e| AppError::Io(e.to_string()))?;
        return Ok(());
    }

    let (bold, dim, reset, green, cyan) = if !no_color {
        ("\x1b[1m", "\x1b[2m", "\x1b[0m", "\x1b[32m", "\x1b[36m")
    } else {
        ("", "", "", "", "")
    };

    writeln!(out, "{bold}{}{reset}", now.format("%A, %B %-d, %Y"))
        .map_err(|e| AppError::Io(e.to_string()))?;
    writeln!(out).map_err(|e| AppError::Io(e.to_string()))?;

    let (all_day, timed) = split_agenda_events(&events);

    // All-day events first
    for ev in &all_day {
        writeln!(
            out,
            "  {cyan}┄┄┄ all day ┄┄┄{reset}  {bold}{}{reset}  {dim}{}{reset}",
            ev.title, ev.calendar
        )
        .map_err(|e| AppError::Io(e.to_string()))?;
    }
    if !all_day.is_empty() && !timed.is_empty() {
        writeln!(out).map_err(|e| AppError::Io(e.to_string()))?;
    }

    // Timed events with gaps and status
    let mut prev_end: Option<chrono::DateTime<Local>> = None;

    for ev in &timed {
        let is_past = ev.end <= now;
        let is_now = ev.start <= now && ev.end > now;

        // Show gap between events
        if let Some(pe) = prev_end {
            if ev.start > pe {
                let gap = ev.start.signed_duration_since(pe);
                let gap_str = format_relative(gap.num_minutes());
                writeln!(out, "  {dim}  ── {gap_str} gap ──{reset}")
                    .map_err(|e| AppError::Io(e.to_string()))?;
            }
        }

        // Status marker and time info
        let time_range = format!("{} - {}", ev.start.format("%H:%M"), ev.end.format("%H:%M"));

        if is_past {
            writeln!(out, "  {dim}✓ {time_range}  {}{reset}", ev.title)
                .map_err(|e| AppError::Io(e.to_string()))?;
        } else if is_now {
            let remaining = ev.end.signed_duration_since(now);
            let remaining_str = format_relative(remaining.num_minutes());
            writeln!(
                out,
                "  {green}▶ {bold}{time_range}{reset}  {bold}{}{reset}  {dim}{}{reset}",
                ev.title, ev.calendar,
            )
            .map_err(|e| AppError::Io(e.to_string()))?;
            writeln!(out, "  {green}  ← now ({remaining_str} left){reset}")
                .map_err(|e| AppError::Io(e.to_string()))?;
            if let Some(loc) = &ev.location {
                if !loc.is_empty() {
                    // Show first line of location
                    let first_line = loc.lines().next().unwrap_or("");
                    writeln!(out, "  {dim}  📍 {first_line}{reset}")
                        .map_err(|e| AppError::Io(e.to_string()))?;
                }
            }
        } else {
            let until = ev.start.signed_duration_since(now);
            let until_str = format_relative(until.num_minutes());
            writeln!(
                out,
                "  · {time_range}  {bold}{}{reset}  {dim}{}  in {until_str}{reset}",
                ev.title, ev.calendar,
            )
            .map_err(|e| AppError::Io(e.to_string()))?;
            if let Some(loc) = &ev.location {
                if !loc.is_empty() {
                    let first_line = loc.lines().next().unwrap_or("");
                    writeln!(out, "  {dim}  📍 {first_line}{reset}")
                        .map_err(|e| AppError::Io(e.to_string()))?;
                }
            }
        }

        prev_end = Some(ev.end);
    }

    // Summary
    let (all_day_count, done, remaining) = summary_counts(&all_day, &timed, now);
    writeln!(out).map_err(|e| AppError::Io(e.to_string()))?;
    writeln!(
        out,
        "{dim}{all_day_count} all-day, {done} timed done, {remaining} timed remaining{reset}"
    )
    .map_err(|e| AppError::Io(e.to_string()))?;

    Ok(())
}

fn split_agenda_events(events: &[EventInfo]) -> (Vec<&EventInfo>, Vec<&EventInfo>) {
    let mut all_day: Vec<&EventInfo> = events.iter().filter(|e| e.all_day).collect();
    all_day.sort_by(|a, b| {
        a.title
            .cmp(&b.title)
            .then_with(|| a.calendar.cmp(&b.calendar))
    });

    let mut timed: Vec<&EventInfo> = events.iter().filter(|e| !e.all_day).collect();
    timed.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.title.cmp(&b.title)));

    (all_day, timed)
}

fn ordered_agenda_events(events: &[EventInfo]) -> Vec<EventInfo> {
    let (all_day, timed) = split_agenda_events(events);
    all_day.into_iter().chain(timed).cloned().collect()
}

fn summary_counts(
    all_day: &[&EventInfo],
    timed: &[&EventInfo],
    now: chrono::DateTime<Local>,
) -> (usize, usize, usize) {
    let done = timed.iter().filter(|event| event.end <= now).count();
    let remaining = timed.len() - done;
    (all_day.len(), done, remaining)
}

fn format_relative(minutes: i64) -> String {
    if minutes < 60 {
        format!("{minutes}m")
    } else if minutes % 60 == 0 {
        format!("{}h", minutes / 60)
    } else {
        format!("{}h {}m", minutes / 60, minutes % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::{split_agenda_events, summary_counts};
    use crate::store::EventInfo;
    use chrono::{Local, NaiveDate, TimeZone};

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

    fn make_event(
        title: &str,
        start: chrono::DateTime<Local>,
        end: chrono::DateTime<Local>,
        all_day: bool,
    ) -> EventInfo {
        EventInfo {
            id: title.to_string(),
            title: title.to_string(),
            start,
            end,
            calendar: "Work".to_string(),
            calendar_id: "cal-1".to_string(),
            location: None,
            url: None,
            notes: None,
            all_day,
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

    #[test]
    fn test_split_agenda_events_sorts_timed_events_by_start() {
        let late = make_event(
            "Late",
            local_dt(2026, 3, 20, 15, 0),
            local_dt(2026, 3, 20, 16, 0),
            false,
        );
        let early = make_event(
            "Early",
            local_dt(2026, 3, 20, 9, 0),
            local_dt(2026, 3, 20, 10, 0),
            false,
        );
        let all_day = make_event(
            "Holiday",
            local_dt(2026, 3, 20, 0, 0),
            local_dt(2026, 3, 21, 0, 0),
            true,
        );

        let events = [late, all_day.clone(), early.clone()];
        let (all_day_events, timed) = split_agenda_events(&events);
        assert_eq!(all_day_events[0].title, all_day.title);
        assert_eq!(timed[0].title, early.title);
        assert_eq!(timed[1].title, "Late");
    }

    #[test]
    fn test_split_agenda_events_keeps_non_canceled_events_orderable() {
        let mut canceled = make_event(
            "Canceled",
            local_dt(2026, 3, 20, 8, 0),
            local_dt(2026, 3, 20, 9, 0),
            false,
        );
        canceled.status = "canceled".to_string();

        let visible = make_event(
            "Visible",
            local_dt(2026, 3, 20, 10, 0),
            local_dt(2026, 3, 20, 11, 0),
            false,
        );

        let events: Vec<EventInfo> = vec![canceled, visible.clone()]
            .into_iter()
            .filter(|event| event.status != "canceled")
            .collect();
        let (_, timed) = split_agenda_events(&events);
        assert_eq!(timed.len(), 1);
        assert_eq!(timed[0].title, visible.title);
    }

    #[test]
    fn test_ordered_agenda_events_matches_human_order() {
        let late = make_event(
            "Late",
            local_dt(2026, 3, 20, 15, 0),
            local_dt(2026, 3, 20, 16, 0),
            false,
        );
        let early = make_event(
            "Early",
            local_dt(2026, 3, 20, 9, 0),
            local_dt(2026, 3, 20, 10, 0),
            false,
        );
        let all_day = make_event(
            "Holiday",
            local_dt(2026, 3, 20, 0, 0),
            local_dt(2026, 3, 21, 0, 0),
            true,
        );

        let ordered = super::ordered_agenda_events(&[late, all_day, early]);
        let titles: Vec<&str> = ordered.iter().map(|event| event.title.as_str()).collect();
        assert_eq!(titles, vec!["Holiday", "Early", "Late"]);
    }

    #[test]
    fn test_summary_counts_include_all_day_events() {
        let now = local_dt(2026, 3, 20, 10, 0);
        let all_day = make_event(
            "Holiday",
            local_dt(2026, 3, 20, 0, 0),
            local_dt(2026, 3, 21, 0, 0),
            true,
        );
        let done = make_event(
            "Done",
            local_dt(2026, 3, 20, 8, 0),
            local_dt(2026, 3, 20, 9, 0),
            false,
        );
        let upcoming = make_event(
            "Upcoming",
            local_dt(2026, 3, 20, 11, 0),
            local_dt(2026, 3, 20, 12, 0),
            false,
        );

        let events = [all_day, done, upcoming];
        let (all_day_events, timed) = split_agenda_events(&events);
        let (all_day_count, done_count, remaining_count) =
            summary_counts(&all_day_events, &timed, now);
        assert_eq!(all_day_count, 1);
        assert_eq!(done_count, 1);
        assert_eq!(remaining_count, 1);
    }
}
