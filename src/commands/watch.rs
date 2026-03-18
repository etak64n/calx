use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::Local;
use std::io::{self, Write};

pub fn run(store: &CalendarStore, calendar: Option<String>) -> Result<(), AppError> {
    loop {
        let now = Local::now();
        let today = now.date_naive();
        let events = store.events(today, today, calendar.as_deref())?;

        // Find next upcoming event
        let next = events.iter().find(|e| !e.all_day && e.start > now);

        print!("\x1b[2J\x1b[H"); // clear screen
        println!("calx watch — {}", now.format("%H:%M:%S"));
        println!();

        if let Some(ev) = next {
            let until = ev.start.signed_duration_since(now);
            let hours = until.num_hours();
            let mins = until.num_minutes() % 60;

            if hours > 0 {
                println!("Next: {} in {}h {}m", ev.title, hours, mins);
            } else {
                println!("Next: {} in {}m", ev.title, mins);
            }
            println!(
                "      {} - {}",
                ev.start.format("%H:%M"),
                ev.end.format("%H:%M")
            );
            println!("      {}", ev.calendar);
        } else {
            println!("No more events today.");
        }

        println!();
        println!("--- Today's events ---");
        for ev in &events {
            if ev.all_day {
                println!("  [All Day]  {}", ev.title);
            } else {
                let marker = if ev.start <= now && ev.end > now {
                    "▶"
                } else if ev.start > now {
                    " "
                } else {
                    "✓"
                };
                println!(
                    "  {} {} - {}  {}",
                    marker,
                    ev.start.format("%H:%M"),
                    ev.end.format("%H:%M"),
                    ev.title,
                );
            }
        }

        io::stdout().flush().ok();
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
