use crate::cli::OutputFormat;
use crate::dateparse;
use crate::error::AppError;
use crate::store::CalendarStore;
use chrono::{Duration, Local, NaiveDate};

use super::events::DisplayOpts;

const SEARCH_PAST_DAYS: i64 = 30;
const SEARCH_FUTURE_DAYS: i64 = 90;

pub fn resolve_search_range(
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(NaiveDate, NaiveDate), AppError> {
    let today = Local::now().date_naive();
    let from_date = match (from, to) {
        (Some(s), _) => {
            dateparse::parse_date(s).ok_or_else(|| AppError::InvalidDate(s.to_string()))?
        }
        (None, Some(s)) => {
            let to_date =
                dateparse::parse_date(s).ok_or_else(|| AppError::InvalidDate(s.to_string()))?;
            to_date - Duration::days(SEARCH_PAST_DAYS)
        }
        (None, None) => today - Duration::days(SEARCH_PAST_DAYS),
    };
    let to_date = match (from, to) {
        (_, Some(s)) => {
            dateparse::parse_date(s).ok_or_else(|| AppError::InvalidDate(s.to_string()))?
        }
        (Some(_), None) => from_date + Duration::days(SEARCH_FUTURE_DAYS),
        (None, None) => today + Duration::days(SEARCH_FUTURE_DAYS),
    };
    super::events::validate_date_range(from_date, to_date)?;
    Ok((from_date, to_date))
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &CalendarStore,
    query: &str,
    exact: bool,
    calendar: Option<String>,
    from: Option<String>,
    to: Option<String>,
    format: OutputFormat,
    opts: &DisplayOpts,
) -> Result<(), AppError> {
    super::events::validate_opts(opts)?;
    let (from_date, to_date) = resolve_search_range(from.as_deref(), to.as_deref())?;

    let events = store.search_events(query, exact, from_date, to_date, calendar.as_deref())?;
    super::events::print_events(events, format, opts)
}

#[cfg(test)]
mod tests {
    use super::resolve_search_range;
    use chrono::{Duration, Local};

    #[test]
    fn test_resolve_search_range_defaults_around_today() {
        let today = Local::now().date_naive();
        let (from, to) = resolve_search_range(None, None).unwrap();
        assert_eq!(from, today - Duration::days(30));
        assert_eq!(to, today + Duration::days(90));
    }

    #[test]
    fn test_resolve_search_range_with_only_from() {
        let (from, to) = resolve_search_range(Some("2026-03-20"), None).unwrap();
        assert_eq!(from.format("%Y-%m-%d").to_string(), "2026-03-20");
        assert_eq!(to, from + Duration::days(90));
    }

    #[test]
    fn test_resolve_search_range_with_only_to() {
        let (from, to) = resolve_search_range(None, Some("2026-03-20")).unwrap();
        assert_eq!(to.format("%Y-%m-%d").to_string(), "2026-03-20");
        assert_eq!(from, to - Duration::days(30));
    }
}
