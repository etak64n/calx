use crate::cli::OutputFormat;
use crate::store::EventInfo;
use serde::Serialize;
use unicode_width::UnicodeWidthStr;

pub fn print_output<T: Serialize>(format: OutputFormat, data: &T, human_fn: impl FnOnce(&T)) {
    match format {
        OutputFormat::Human => human_fn(data),
        OutputFormat::Table => print_table(data),
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
        println!("SUMMARY:{}", ics_escape(&ev.title));
        if let Some(notes) = &ev.notes {
            println!("DESCRIPTION:{}", ics_escape(notes));
        }
        println!("END:VEVENT");
    }
    println!("END:VCALENDAR");
}

fn print_table<T: Serialize>(data: &T) {
    let Ok(val) = serde_json::to_value(data) else {
        return;
    };

    let items: Vec<&serde_json::Map<String, serde_json::Value>> = if let Some(arr) = val.as_array()
    {
        arr.iter().filter_map(|v| v.as_object()).collect()
    } else if let Some(obj) = val.as_object() {
        vec![obj]
    } else {
        return;
    };

    if items.is_empty() {
        return;
    }

    let keys: Vec<&String> = items[0].keys().collect();

    // Calculate column widths using display width (CJK-aware)
    let widths: Vec<usize> = keys
        .iter()
        .map(|k| {
            let header_w = UnicodeWidthStr::width(k.as_str());
            let max_val_w = items
                .iter()
                .map(|obj| {
                    let s =
                        value_to_string(obj.get(k.as_str()).unwrap_or(&serde_json::Value::Null));
                    UnicodeWidthStr::width(s.as_str())
                })
                .max()
                .unwrap_or(0);
            header_w.max(max_val_w)
        })
        .collect();

    // Top border
    print!("┌");
    for (i, w) in widths.iter().enumerate() {
        print!("{}", "─".repeat(w + 2));
        print!("{}", if i < widths.len() - 1 { "┬" } else { "┐" });
    }
    println!();

    // Header
    print!("│");
    for (i, key) in keys.iter().enumerate() {
        let label = key.to_uppercase();
        let pad = widths[i] - UnicodeWidthStr::width(label.as_str());
        print!(" {}{} │", label, " ".repeat(pad));
    }
    println!();

    // Separator
    print!("├");
    for (i, w) in widths.iter().enumerate() {
        print!("{}", "─".repeat(w + 2));
        print!("{}", if i < widths.len() - 1 { "┼" } else { "┤" });
    }
    println!();

    // Rows
    for obj in &items {
        print!("│");
        for (i, key) in keys.iter().enumerate() {
            let val = value_to_string(obj.get(key.as_str()).unwrap_or(&serde_json::Value::Null));
            let pad = widths[i] - UnicodeWidthStr::width(val.as_str());
            print!(" {}{} │", val, " ".repeat(pad));
        }
        println!();
    }

    // Bottom border
    print!("└");
    for (i, w) in widths.iter().enumerate() {
        print!("{}", "─".repeat(w + 2));
        print!("{}", if i < widths.len() - 1 { "┴" } else { "┘" });
    }
    println!();
}

/// Escape text per RFC 5545 section 3.3.11 (TEXT).
fn ics_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_string_variants() {
        assert_eq!(
            value_to_string(&serde_json::Value::String("hello".into())),
            "hello"
        );
        assert_eq!(value_to_string(&serde_json::Value::Null), "");
        assert_eq!(value_to_string(&serde_json::Value::Bool(true)), "true");
        assert_eq!(value_to_string(&serde_json::json!(42)), "42");
    }
}
