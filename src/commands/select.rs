use crate::error::AppError;
use crate::store::{CalendarStore, EventInfo};
use std::io::{self, BufRead, IsTerminal, Write};

#[allow(clippy::too_many_arguments)]
pub fn resolve_event(
    store: &CalendarStore,
    event_id: Option<&str>,
    query: Option<&str>,
    exact: bool,
    in_calendar: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    interactive: bool,
) -> Result<EventInfo, AppError> {
    match (event_id, query) {
        (Some(event_id), None) => {
            if event_id.trim().is_empty() {
                return Err(AppError::InvalidArgument(
                    "EVENT_ID must not be empty".to_string(),
                ));
            }
            store.get_event(event_id)
        }
        (None, Some(query)) => {
            let (from_date, to_date) = super::search::resolve_search_range(from, to)?;
            let matches = store.search_events(query, exact, from_date, to_date, in_calendar)?;

            match matches.len() {
                0 => Err(AppError::EventNotFound(query.to_string())),
                1 => Ok(matches.into_iter().next().unwrap()),
                count => {
                    if interactive {
                        pick_event(matches)
                    } else {
                        let mut sorted = matches;
                        sorted.sort_by_key(|e| e.start);
                        let preview = sorted
                            .iter()
                            .take(5)
                            .map(|e| {
                                format!(
                                    "{} [{}] {} ({})",
                                    e.title,
                                    e.calendar,
                                    e.start.format("%Y-%m-%d %H:%M"),
                                    e.id
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("; ");
                        Err(AppError::InvalidArgument(format!(
                            "Query matched {count} events. Use -i to pick interactively, or narrow with --exact title/--in-calendar/--from/--to. Matches: {preview}"
                        )))
                    }
                }
            }
        }
        (Some(_), Some(_)) => Err(AppError::InvalidArgument(
            "Use either EVENT_ID or --query, not both.".to_string(),
        )),
        (None, None) => Err(AppError::InvalidArgument(
            "Provide EVENT_ID or --query.".to_string(),
        )),
    }
}

fn pick_event(mut events: Vec<EventInfo>) -> Result<EventInfo, AppError> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(AppError::InvalidArgument(
            "Interactive selection requires a TTY. Narrow the query or omit -i.".to_string(),
        ));
    }

    events.sort_by_key(|e| e.start);

    eprintln!("Multiple events found. Pick one:\n");
    for (i, ev) in events.iter().enumerate() {
        if ev.all_day {
            eprintln!(
                "  {:>3})  {}  [All Day]        {}  ({})",
                i + 1,
                ev.start.format("%Y-%m-%d"),
                ev.title,
                ev.calendar,
            );
        } else {
            eprintln!(
                "  {:>3})  {} {} - {}  {}  ({})",
                i + 1,
                ev.start.format("%Y-%m-%d"),
                ev.start.format("%H:%M"),
                ev.end.format("%H:%M"),
                ev.title,
                ev.calendar,
            );
        }
    }

    eprint!("\nEnter number (1-{}) or q to cancel: ", events.len());
    io::stderr().flush().ok();

    let mut input = String::new();
    io::stdin()
        .lock()
        .read_line(&mut input)
        .map_err(|e| AppError::EventKit(format!("Failed to read input: {e}")))?;

    let trimmed = input.trim();
    if matches!(trimmed, "q" | "quit" | "exit") {
        return Err(AppError::InvalidArgument(
            "Interactive selection cancelled.".to_string(),
        ));
    }

    let choice: usize = trimmed
        .parse()
        .map_err(|_| AppError::InvalidArgument("Invalid number".to_string()))?;

    if choice < 1 || choice > events.len() {
        return Err(AppError::InvalidArgument(format!(
            "Number out of range: {choice}"
        )));
    }

    Ok(events.into_iter().nth(choice - 1).unwrap())
}
