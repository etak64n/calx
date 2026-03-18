use crate::cli::OutputFormat;
use crate::store::EventInfo;
use serde::Serialize;

pub fn print_output<T: Serialize>(format: OutputFormat, data: &T, human_fn: impl FnOnce(&T)) {
    match format {
        OutputFormat::Human => human_fn(data),
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        }
        OutputFormat::Yaml => {
            print!("{}", serde_yaml::to_string(data).unwrap());
        }
        OutputFormat::Csv => print_delimited(data, b','),
        OutputFormat::Tsv => print_delimited(data, b'\t'),
        OutputFormat::Ics => {
            // ICS requires Vec<EventInfo>; for other types, fall back to JSON
            if let Ok(events) = serde_json::from_value::<Vec<EventInfo>>(
                serde_json::to_value(data).unwrap_or_default(),
            ) {
                print_ics(&events);
            } else {
                println!("{}", serde_json::to_string_pretty(data).unwrap());
            }
        }
    }
}

fn print_delimited<T: Serialize>(data: &T, delimiter: u8) {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(std::io::stdout());
    if let Ok(arr) = serde_json::to_value(data) {
        if let Some(items) = arr.as_array() {
            if let Some(first) = items.first() {
                if let Some(obj) = first.as_object() {
                    wtr.write_record(obj.keys()).ok();
                }
            }
            for item in items {
                if let Some(obj) = item.as_object() {
                    wtr.write_record(obj.values().map(value_to_string)).ok();
                }
            }
        } else if let Some(obj) = arr.as_object() {
            wtr.write_record(obj.keys()).ok();
            wtr.write_record(obj.values().map(value_to_string)).ok();
        }
    }
    wtr.flush().ok();
}

fn print_ics(events: &[EventInfo]) {
    println!("BEGIN:VCALENDAR");
    println!("VERSION:2.0");
    println!("PRODID:-//calx//EN");
    for ev in events {
        println!("BEGIN:VEVENT");
        println!("UID:{}", ev.id);
        if ev.all_day {
            println!("DTSTART;VALUE=DATE:{}", ev.start.format("%Y%m%d"));
            println!("DTEND;VALUE=DATE:{}", ev.end.format("%Y%m%d"));
        } else {
            println!("DTSTART:{}", ev.start.format("%Y%m%dT%H%M%S"));
            println!("DTEND:{}", ev.end.format("%Y%m%dT%H%M%S"));
        }
        println!("SUMMARY:{}", ev.title);
        if let Some(notes) = &ev.notes {
            let escaped = notes.replace('\n', "\\n");
            println!("DESCRIPTION:{escaped}");
        }
        println!("END:VEVENT");
    }
    println!("END:VCALENDAR");
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}
