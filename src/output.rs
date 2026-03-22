use crate::cli::OutputFormat;
use crate::error::AppError;
use serde::Serialize;
use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;

pub fn print_output<T: Serialize>(
    format: OutputFormat,
    data: &T,
    human_fn: impl FnOnce(&T, &mut dyn Write) -> io::Result<()>,
) -> Result<(), AppError> {
    let format = format.resolve_for_stdout();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    match format {
        OutputFormat::Human => human_fn(data, &mut out).map_err(|e| AppError::Io(e.to_string()))?,
        _ => write_structured_output_to(format, data, &mut out)?,
    }

    out.flush().map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

pub fn write_structured_output_to<T: Serialize, W: Write>(
    format: OutputFormat,
    data: &T,
    out: &mut W,
) -> Result<(), AppError> {
    let format = format.resolve_for_stdout();
    match format {
        OutputFormat::Auto | OutputFormat::Human => write_json(out, data)?,
        OutputFormat::Table => print_table(out, data).map_err(|e| AppError::Io(e.to_string()))?,
        OutputFormat::Json => write_json(out, data)?,
        OutputFormat::Yaml => write_yaml(out, data)?,
        OutputFormat::Csv => print_delimited(out, data, b',')?,
        OutputFormat::Tsv => print_delimited(out, data, b'\t')?,
    }
    out.flush().map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

fn write_json<T: Serialize, W: Write>(out: &mut W, data: &T) -> Result<(), AppError> {
    serde_json::to_writer_pretty(&mut *out, data).map_err(|e| AppError::Io(e.to_string()))?;
    out.write_all(b"\n")
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

fn write_yaml<T: Serialize, W: Write>(out: &mut W, data: &T) -> Result<(), AppError> {
    let yaml = serde_yml::to_string(data).map_err(|e| AppError::Io(e.to_string()))?;
    out.write_all(yaml.as_bytes())
        .map_err(|e| AppError::Io(e.to_string()))?;
    if !yaml.ends_with('\n') {
        out.write_all(b"\n")
            .map_err(|e| AppError::Io(e.to_string()))?;
    }
    Ok(())
}

fn print_delimited<T: Serialize, W: Write>(
    out: &mut W,
    data: &T,
    delimiter: u8,
) -> Result<(), AppError> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(out);
    if let Ok(arr) = serde_json::to_value(data) {
        if let Some(items) = arr.as_array() {
            if let Some(first) = items.first() {
                if let Some(obj) = first.as_object() {
                    wtr.write_record(obj.keys())
                        .map_err(|e| AppError::Io(e.to_string()))?;
                }
            }
            for item in items {
                if let Some(obj) = item.as_object() {
                    wtr.write_record(obj.values().map(value_to_string))
                        .map_err(|e| AppError::Io(e.to_string()))?;
                }
            }
        } else if let Some(obj) = arr.as_object() {
            wtr.write_record(obj.keys())
                .map_err(|e| AppError::Io(e.to_string()))?;
            wtr.write_record(obj.values().map(value_to_string))
                .map_err(|e| AppError::Io(e.to_string()))?;
        }
    }
    wtr.flush().map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

fn print_table<T: Serialize, W: Write>(out: &mut W, data: &T) -> io::Result<()> {
    let Ok(val) = serde_json::to_value(data) else {
        return Ok(());
    };

    let items: Vec<&serde_json::Map<String, serde_json::Value>> = if let Some(arr) = val.as_array()
    {
        arr.iter().filter_map(|v| v.as_object()).collect()
    } else if let Some(obj) = val.as_object() {
        vec![obj]
    } else {
        return Ok(());
    };

    if items.is_empty() {
        return Ok(());
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
    write!(out, "┌")?;
    for (i, w) in widths.iter().enumerate() {
        write!(out, "{}", "─".repeat(w + 2))?;
        write!(out, "{}", if i < widths.len() - 1 { "┬" } else { "┐" })?;
    }
    writeln!(out)?;

    // Header
    write!(out, "│")?;
    for (i, key) in keys.iter().enumerate() {
        let label = key.to_uppercase();
        let pad = widths[i] - UnicodeWidthStr::width(label.as_str());
        write!(out, " {}{} │", label, " ".repeat(pad))?;
    }
    writeln!(out)?;

    // Separator
    write!(out, "├")?;
    for (i, w) in widths.iter().enumerate() {
        write!(out, "{}", "─".repeat(w + 2))?;
        write!(out, "{}", if i < widths.len() - 1 { "┼" } else { "┤" })?;
    }
    writeln!(out)?;

    // Rows
    for obj in &items {
        write!(out, "│")?;
        for (i, key) in keys.iter().enumerate() {
            let val = value_to_string(obj.get(key.as_str()).unwrap_or(&serde_json::Value::Null));
            let pad = widths[i] - UnicodeWidthStr::width(val.as_str());
            write!(out, " {}{} │", val, " ".repeat(pad))?;
        }
        writeln!(out)?;
    }

    // Bottom border
    write!(out, "└")?;
    for (i, w) in widths.iter().enumerate() {
        write!(out, "{}", "─".repeat(w + 2))?;
        write!(out, "{}", if i < widths.len() - 1 { "┴" } else { "┘" })?;
    }
    writeln!(out)?;
    Ok(())
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
    use std::io;

    struct FailingWriter;

    impl io::Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"))
        }
    }

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

    #[test]
    fn test_write_json_propagates_io_errors() {
        let mut writer = FailingWriter;
        let err = write_json(&mut writer, &serde_json::json!({"ok": true})).unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
    }

    #[test]
    fn test_write_yaml_propagates_io_errors() {
        let mut writer = FailingWriter;
        let err = write_yaml(&mut writer, &serde_json::json!({"ok": true})).unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
    }

    #[test]
    fn test_print_table_propagates_io_errors() {
        let mut writer = FailingWriter;
        let err = print_table(&mut writer, &vec![serde_json::json!({"ok": true})]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
