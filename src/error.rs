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
    #[error("Invalid date: {0}. Use YYYY-MM-DD or YYYY-MM-DD HH:MM")]
    InvalidDate(String),
    #[error("{0}")]
    EventKit(String),
}
