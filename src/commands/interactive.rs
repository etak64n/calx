use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use dialoguer::{Confirm, Input, Select};
use serde::Serialize;

#[derive(Serialize)]
struct AddResult {
    event_id: String,
}

pub fn run(store: &CalendarStore, format: OutputFormat) -> Result<(), AppError> {
    let title: String = Input::new()
        .with_prompt("Title")
        .interact_text()
        .map_err(|e| AppError::EventKit(e.to_string()))?;

    let start_str: String = Input::new()
        .with_prompt("Start (e.g. tomorrow 3pm, 2026-03-20 14:00)")
        .interact_text()
        .map_err(|e| AppError::EventKit(e.to_string()))?;

    let end_str: String = Input::new()
        .with_prompt("End (e.g. tomorrow 4pm, 2026-03-20 15:00)")
        .interact_text()
        .map_err(|e| AppError::EventKit(e.to_string()))?;

    let start_dt = dateparse::parse_datetime(&start_str)
        .ok_or_else(|| AppError::InvalidDate(start_str.clone()))?;
    let end_dt = dateparse::parse_datetime(&end_str)
        .ok_or_else(|| AppError::InvalidDate(end_str.clone()))?;

    // Calendar selection
    let calendars = store.calendars();
    let cal_names: Vec<String> = calendars.iter().map(|c| c.title.clone()).collect();
    let cal_idx = Select::new()
        .with_prompt("Calendar")
        .items(&cal_names)
        .default(0)
        .interact()
        .map_err(|e| AppError::EventKit(e.to_string()))?;
    let calendar = Some(cal_names[cal_idx].as_str());

    let all_day = Confirm::new()
        .with_prompt("All day?")
        .default(false)
        .interact()
        .map_err(|e| AppError::EventKit(e.to_string()))?;

    let notes: String = Input::new()
        .with_prompt("Notes (optional)")
        .allow_empty(true)
        .interact_text()
        .map_err(|e| AppError::EventKit(e.to_string()))?;
    let notes_opt = if notes.is_empty() {
        None
    } else {
        Some(notes.as_str())
    };

    let event_id = store.add_event(&title, start_dt, end_dt, calendar, notes_opt, all_day)?;

    let result = AddResult { event_id };
    print_output(format, &result, |r| {
        println!("Event created: {}", r.event_id);
    });
    Ok(())
}
