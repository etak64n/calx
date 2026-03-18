use crate::error::AppError;
use crate::store::{CalendarStore, EventInfo};

pub fn resolve_event(
    store: &CalendarStore,
    event_id: Option<&str>,
    query: Option<&str>,
    exact: bool,
    in_calendar: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<EventInfo, AppError> {
    match (event_id, query) {
        (Some(event_id), None) => store.get_event(event_id),
        (None, Some(query)) => {
            let (from_date, to_date) = super::search::resolve_search_range(from, to)?;
            store.find_unique_event(query, exact, from_date, to_date, in_calendar)
        }
        (Some(_), Some(_)) => Err(AppError::InvalidArgument(
            "Use either EVENT_ID or --query, not both.".to_string(),
        )),
        (None, None) => Err(AppError::InvalidArgument(
            "Provide EVENT_ID or --query.".to_string(),
        )),
    }
}
