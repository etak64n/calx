use crate::cli::OutputFormat;
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
        OutputFormat::Csv => print_csv(data),
        OutputFormat::Tsv => print_tsv(data),
    }
}

fn print_csv<T: Serialize>(data: &T) {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    // Try as array first, fall back to single record
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

fn print_tsv<T: Serialize>(data: &T) {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
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

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}
