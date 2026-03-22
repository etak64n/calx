use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Calendar access denied. Grant in System Settings > Privacy > Calendars.")]
    AccessDenied,
    #[error("Calendar access was denied.")]
    AccessRejected,
    #[error("Timeout waiting for calendar access.")]
    AccessTimeout,
    #[error("Calendar not found: {0}")]
    CalendarNotFound(String),
    #[error("Event not found: {0}")]
    EventNotFound(String),
    #[error("Template not found: {0}")]
    TemplateNotFound(String),
    #[error("Invalid date: {0}. Use YYYY-MM-DD, YYYY-MM-DD HH:MM, or natural language")]
    InvalidDate(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("EventKit: {0}")]
    EventKit(String),
}

impl AppError {
    pub fn title(&self) -> &'static str {
        match self {
            AppError::AccessDenied | AppError::AccessRejected | AppError::AccessTimeout => {
                "Calendar access"
            }
            AppError::CalendarNotFound(_) => "Calendar not found",
            AppError::EventNotFound(_) => "Event not found",
            AppError::TemplateNotFound(_) => "Template not found",
            AppError::InvalidDate(_) => "Invalid date",
            AppError::InvalidArgument(_) => "Invalid argument",
            AppError::Io(_) => "I/O error",
            AppError::EventKit(_) => "EventKit error",
        }
    }

    pub fn why(&self) -> String {
        match self {
            AppError::AccessDenied => {
                "The calx binary does not currently have Calendar access.".to_string()
            }
            AppError::AccessRejected => "Calendar access was denied.".to_string(),
            AppError::AccessTimeout => "Timed out while waiting for Calendar access.".to_string(),
            AppError::CalendarNotFound(name) => format!("No calendar matched '{name}'."),
            AppError::EventNotFound(value) => format!("No event matched '{value}'."),
            AppError::TemplateNotFound(name) => format!("No template named '{name}' exists."),
            AppError::InvalidDate(value) => value.clone(),
            AppError::InvalidArgument(value) => value.clone(),
            AppError::Io(value) => value.clone(),
            AppError::EventKit(value) => value.clone(),
        }
    }

    pub fn hint(&self, help_command: &str) -> Option<String> {
        match self {
            AppError::AccessDenied | AppError::AccessRejected | AppError::AccessTimeout => Some(
                "Grant access in System Settings > Privacy & Security > Calendars, then retry."
                    .to_string(),
            ),
            AppError::CalendarNotFound(_) => Some(
                "Run `calx calendars -o json` to list calendar names and IDs. Use the calendar ID when names are ambiguous.".to_string(),
            ),
            AppError::EventNotFound(_) => Some(
                "Try `calx search <query> -o json` or widen `--from/--to` to locate the event."
                    .to_string(),
            ),
            AppError::TemplateNotFound(_) => {
                Some("Run `calx template list` to inspect available templates.".to_string())
            }
            AppError::InvalidDate(_) => Some(format!(
                "Use YYYY-MM-DD, YYYY-MM-DD HH:MM, or natural language like `tomorrow 3pm`. {help_command}"
            )),
            AppError::InvalidArgument(_) => Some(help_command.to_string()),
            AppError::Io(_) => None,
            AppError::EventKit(_) => Some(
                "Retry once, then run `calx doctor` to verify permission, default calendar, and calendar writability.".to_string(),
            ),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::AccessDenied | AppError::AccessRejected | AppError::AccessTimeout => 2,
            AppError::CalendarNotFound(_)
            | AppError::EventNotFound(_)
            | AppError::TemplateNotFound(_) => 3,
            AppError::InvalidDate(_) | AppError::InvalidArgument(_) => 4,
            AppError::Io(_) => 6,
            AppError::EventKit(_) => 5,
        }
    }
}
