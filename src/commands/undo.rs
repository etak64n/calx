use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::state::{self, UndoAction, UndoRecord};
use crate::store::CalendarStore;
use serde::Serialize;
use serde_json::json;

#[derive(Serialize)]
struct UndoResult {
    undone: bool,
    action: String,
    event_id: Option<String>,
}

pub fn run(
    store: &CalendarStore,
    format: OutputFormat,
    record: UndoRecord,
) -> Result<(), AppError> {
    let outcome: Result<(String, Option<String>), AppError> = match record.action.clone() {
        UndoAction::DeleteCreated {
            event_id,
            selected_start,
            scope,
        } => {
            store.delete_event(&event_id, selected_start, scope)?;
            Ok(("delete_created".to_string(), Some(event_id)))
        }
        UndoAction::RestoreDeleted { draft } => {
            let restored = store.create_event_from_draft(
                &draft,
                None,
                draft.start,
                draft.end,
                Some(draft.calendar_id.as_str()),
                true,
            )?;
            Ok(("restore_deleted".to_string(), Some(restored.id)))
        }
        UndoAction::ReplaceWithDraft {
            current_event_id,
            current_start,
            current_scope,
            draft,
        } => {
            let restored = store.restore_event_from_draft(
                &current_event_id,
                current_start,
                current_scope,
                &draft,
            )?;
            Ok(("restore_updated".to_string(), Some(restored.id)))
        }
        UndoAction::Unavailable { reason } => Err(AppError::InvalidArgument(format!(
            "Undo unavailable for the last action: {reason}"
        ))),
    };

    let (action_name, event_id) = match outcome {
        Ok(result) => result,
        Err(err) => {
            if let Err(restore_err) = state::restore_undo_record(record) {
                let warning =
                    format!("Undo failed, and undo history could not be restored: {restore_err}");
                super::emit_warning(
                    format,
                    &warning,
                    &json!({
                        "warning": warning,
                        "undo_restored": false
                    }),
                );
            }
            return Err(err);
        }
    };

    if let Err(finalize_err) = state::finalize_undo_record(&record) {
        let warning =
            format!("Undo succeeded, but pending undo state could not be cleared: {finalize_err}");
        super::emit_warning(
            format,
            &warning,
            &json!({
                "warning": warning,
                "undo_finalized": false
            }),
        );
    }

    let result = UndoResult {
        undone: true,
        action: action_name,
        event_id: event_id.clone(),
    };
    print_output(format, &result, |_, out| {
        if let Some(event_id) = &event_id {
            writeln!(out, "Undo complete: {event_id}")
        } else {
            writeln!(out, "Undo complete.")
        }
    })
}
