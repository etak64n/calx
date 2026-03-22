pub mod add;
pub mod agenda;
pub mod calendars;
pub mod completions;
pub mod conflicts;
pub mod delete;
pub mod doctor;
pub mod duplicate;
pub mod events;
pub mod free;
pub mod next;
pub mod search;
pub mod select;
pub mod show;
pub mod template;
pub mod today;
pub mod undo;
pub mod upcoming;
pub mod update;

use crate::cli::OutputFormat;
use crate::output::write_structured_output_to;
use crate::state::{self, UndoAction};
use serde::Serialize;
use serde_json::json;
use std::io::{self, Write};

pub(crate) fn save_undo_best_effort(action: UndoAction, format: OutputFormat) {
    if let UndoAction::Unavailable { reason } = action {
        let warning = format!("Change succeeded, but undo is unavailable: {reason}");
        emit_warning(
            format,
            &warning,
            &json!({
                "warning": warning,
                "undo_saved": false,
                "undo_available": false
            }),
        );
        return;
    }

    if let Err(err) = state::save_undo(action) {
        let warning = format!("Change succeeded, but undo history could not be saved: {err}");
        emit_warning(
            format,
            &warning,
            &json!({
                "warning": warning,
                "undo_saved": false
            }),
        );
    }
}

pub(crate) fn emit_warning<T: Serialize>(format: OutputFormat, human_warning: &str, payload: &T) {
    let resolved = format.resolve_for_stdout();
    let stderr = io::stderr();
    let mut out = stderr.lock();

    if matches!(resolved, OutputFormat::Human) {
        let _ = writeln!(out, "Warning: {human_warning}");
        return;
    }

    let _ = write_structured_output_to(resolved, payload, &mut out);
}
