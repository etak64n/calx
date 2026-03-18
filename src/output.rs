use crate::cli::OutputFormat;
use serde::Serialize;

pub fn print_output<T: Serialize>(format: OutputFormat, data: &T, human_fn: impl FnOnce(&T)) {
    match format {
        OutputFormat::Human => human_fn(data),
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        }
    }
}
