use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarStore, EventInfo};
use chrono::Local;

pub fn run(
    store: &CalendarStore,
    calendar: Option<String>,
    format: OutputFormat,
    no_color: bool,
) -> Result<(), AppError> {
    let now = Local::now();
    let today = now.date_naive();
    let events = store.events(today, today, calendar.as_deref())?;

    // For structured formats, use standard serialization
    if !matches!(format, OutputFormat::Human) {
        print_output(format, &events, |_| {});
        return Ok(());
    }

    if events.is_empty() {
        println!("No events today.");
        return Ok(());
    }

    let (bold, dim, reset, green, cyan) = if !no_color {
        ("\x1b[1m", "\x1b[2m", "\x1b[0m", "\x1b[32m", "\x1b[36m")
    } else {
        ("", "", "", "", "")
    };

    println!("{bold}{}{reset}", now.format("%A, %B %-d, %Y"));
    println!();

    let timed: Vec<&EventInfo> = events.iter().filter(|e| !e.all_day).collect();
    let all_day: Vec<&EventInfo> = events.iter().filter(|e| e.all_day).collect();

    // All-day events first
    for ev in &all_day {
        println!(
            "  {cyan}┄┄┄ all day ┄┄┄{reset}  {bold}{}{reset}  {dim}{}{reset}",
            ev.title, ev.calendar
        );
    }
    if !all_day.is_empty() && !timed.is_empty() {
        println!();
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
                println!("  {dim}  ── {gap_str} gap ──{reset}");
            }
        }

        // Status marker and time info
        let time_range = format!("{} - {}", ev.start.format("%H:%M"), ev.end.format("%H:%M"));

        if is_past {
            println!("  {dim}✓ {time_range}  {}{reset}", ev.title,);
        } else if is_now {
            let remaining = ev.end.signed_duration_since(now);
            let remaining_str = format_relative(remaining.num_minutes());
            println!(
                "  {green}▶ {bold}{time_range}{reset}  {bold}{}{reset}  {dim}{}{reset}",
                ev.title, ev.calendar,
            );
            println!("  {green}  ← now ({remaining_str} left){reset}",);
            if let Some(loc) = &ev.location {
                if !loc.is_empty() {
                    // Show first line of location
                    let first_line = loc.lines().next().unwrap_or("");
                    println!("  {dim}  📍 {first_line}{reset}");
                }
            }
        } else {
            let until = ev.start.signed_duration_since(now);
            let until_str = format_relative(until.num_minutes());
            println!(
                "  · {time_range}  {bold}{}{reset}  {dim}{}  in {until_str}{reset}",
                ev.title, ev.calendar,
            );
            if let Some(loc) = &ev.location {
                if !loc.is_empty() {
                    let first_line = loc.lines().next().unwrap_or("");
                    println!("  {dim}  📍 {first_line}{reset}");
                }
            }
        }

        prev_end = Some(ev.end);
    }

    // Summary
    let done = timed.iter().filter(|e| e.end <= now).count();
    let remaining = timed.len() - done;
    println!();
    println!("{dim}{done} event(s) done, {remaining} remaining{reset}");

    Ok(())
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
