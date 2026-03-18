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
    #[error("Invalid date: {0}. Use YYYY-MM-DD, YYYY-MM-DD HH:MM, or natural language")]
    InvalidDate(String),
    #[error("EventKit: {0}")]
    EventKit(String),
}

impl AppError {
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::AccessDenied | AppError::AccessRejected | AppError::AccessTimeout => 2,
            AppError::CalendarNotFound(_) | AppError::EventNotFound(_) => 3,
            AppError::InvalidDate(_) => 4,
            AppError::EventKit(_) => 5,
        }
    }
}
