use crate::error::AppError;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::*;
use objc2_foundation::*;
use serde::Serialize;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Serialize)]
pub struct CalendarInfo {
    pub title: String,
    pub source: String,
}

#[derive(Clone, Serialize, serde::Deserialize)]
pub struct EventInfo {
    pub id: String,
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub calendar: String,
    pub location: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub all_day: bool,
    pub status: String,
    pub availability: String,
    pub organizer: Option<String>,
    pub created: Option<DateTime<Local>>,
    pub modified: Option<DateTime<Local>>,
    pub recurring: bool,
    pub recurrence: Option<String>,
}

#[derive(Clone, Serialize, serde::Deserialize)]
pub struct FreeSlot {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub duration_mins: u32,
}

pub struct CalendarStore {
    store: Retained<EKEventStore>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecurrenceScope {
    This,
    Future,
}

impl CalendarStore {
    pub fn new() -> Result<Self, AppError> {
        let store = unsafe { EKEventStore::new() };
        Self::request_access(&store)?;
        Ok(Self { store })
    }

    fn request_access(store: &EKEventStore) -> Result<(), AppError> {
        let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };

        if status == EKAuthorizationStatus::FullAccess {
            return Ok(());
        }

        if status == EKAuthorizationStatus::Denied || status == EKAuthorizationStatus::Restricted {
            return Err(AppError::AccessDenied);
        }

        let (tx, rx) = mpsc::channel();

        let block = block2::RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            let _ = tx.send(granted.as_bool());
        });

        let block_ptr = &*block as *const block2::Block<_> as *mut block2::Block<_>;
        unsafe { store.requestFullAccessToEventsWithCompletion(block_ptr) };

        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(true) => Ok(()),
            Ok(false) => Err(AppError::AccessRejected),
            Err(_) => Err(AppError::AccessTimeout),
        }
    }

    pub fn calendars(&self) -> Vec<CalendarInfo> {
        let cals = unsafe { self.store.calendarsForEntityType(EKEntityType::Event) };
        let count = cals.count();
        let mut result = Vec::with_capacity(count);

        for i in 0..count {
            let cal = cals.objectAtIndex(i);
            let title = unsafe { cal.title() }.to_string();
            let source = unsafe { cal.source() }
                .map(|s| unsafe { s.title() }.to_string())
                .unwrap_or_default();
            result.push(CalendarInfo { title, source });
        }
        result
    }

    pub fn events(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        calendar_name: Option<&str>,
    ) -> Result<Vec<EventInfo>, AppError> {
        let from_ts = Local
            .from_local_datetime(&from.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .ok_or_else(|| AppError::InvalidDate(from.to_string()))?
            .timestamp() as f64;
        let to_ts = Local
            .from_local_datetime(&to.and_hms_opt(23, 59, 59).unwrap())
            .earliest()
            .ok_or_else(|| AppError::InvalidDate(to.to_string()))?
            .timestamp() as f64;

        let start_date = NSDate::dateWithTimeIntervalSince1970(from_ts);
        let end_date = NSDate::dateWithTimeIntervalSince1970(to_ts);

        let predicate = unsafe {
            self.store
                .predicateForEventsWithStartDate_endDate_calendars(&start_date, &end_date, None)
        };

        let events = unsafe { self.store.eventsMatchingPredicate(&predicate) };
        let count = events.count();
        let mut result = Vec::new();

        for i in 0..count {
            let event = events.objectAtIndex(i);

            let title = unsafe { event.title() }.to_string();
            let cal_name = unsafe { event.calendar() }
                .map(|c| unsafe { c.title() }.to_string())
                .unwrap_or_default();

            if let Some(name) = calendar_name {
                if cal_name != name {
                    continue;
                }
            }

            let start = nsdate_to_datetime(unsafe { event.startDate() });
            let all_day = unsafe { event.isAllDay() };
            let end =
                normalize_all_day_end(nsdate_to_datetime(unsafe { event.endDate() }), all_day);
            let location = unsafe { event.location() }.map(|l| l.to_string());
            let url =
                unsafe { event.URL() }.and_then(|u| u.absoluteString().map(|s| s.to_string()));
            let notes = unsafe { event.notes() }.map(|n| n.to_string());
            let id = unsafe { event.eventIdentifier() }
                .map(|i| i.to_string())
                .unwrap_or_default();
            let status = match unsafe { event.status() } {
                EKEventStatus::Confirmed => "confirmed",
                EKEventStatus::Tentative => "tentative",
                EKEventStatus::Canceled => "canceled",
                _ => "none",
            }
            .to_string();
            let availability = match unsafe { event.availability() } {
                EKEventAvailability::Busy => "busy",
                EKEventAvailability::Free => "free",
                EKEventAvailability::Tentative => "tentative",
                EKEventAvailability::Unavailable => "unavailable",
                _ => "not supported",
            }
            .to_string();
            let organizer = unsafe { event.organizer() }.map(|p| {
                unsafe { p.name() }
                    .map(|n| n.to_string())
                    .unwrap_or_default()
            });
            let created = unsafe { event.creationDate() }.map(nsdate_to_datetime);
            let modified = unsafe { event.lastModifiedDate() }.map(nsdate_to_datetime);
            let recurring = event_is_recurring(&event);
            let recurrence = recurrence_summary(&event);

            result.push(EventInfo {
                id,
                title,
                start,
                end,
                calendar: cal_name,
                location,
                url,
                notes,
                all_day,
                status,
                availability,
                organizer,
                created,
                modified,
                recurring,
                recurrence,
            });
        }

        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_event(
        &self,
        title: &str,
        start: NaiveDateTime,
        end: NaiveDateTime,
        calendar_name: Option<&str>,
        location: Option<&str>,
        url: Option<&str>,
        notes: Option<&str>,
        all_day: bool,
        repeat: Option<&str>,
        repeat_count: Option<u32>,
        repeat_interval: Option<u32>,
        alerts: &[i64],
    ) -> Result<String, AppError> {
        let event = unsafe { EKEvent::eventWithEventStore(&self.store) };

        let ns_title = NSString::from_str(title);
        unsafe { event.setTitle(Some(&ns_title)) };

        let start_ts = Local
            .from_local_datetime(&start)
            .earliest()
            .ok_or_else(|| AppError::InvalidDate(start.to_string()))?
            .timestamp() as f64;
        let end_ts = Local
            .from_local_datetime(&end)
            .earliest()
            .ok_or_else(|| AppError::InvalidDate(end.to_string()))?
            .timestamp() as f64;

        let start_date = NSDate::dateWithTimeIntervalSince1970(start_ts);
        let end_date = NSDate::dateWithTimeIntervalSince1970(end_ts);

        unsafe {
            event.setStartDate(Some(&start_date));
            event.setEndDate(Some(&end_date));
            event.setAllDay(all_day);
        };

        if let Some(loc) = location {
            let ns = NSString::from_str(loc);
            unsafe { event.setLocation(Some(&ns)) };
        }

        if let Some(u) = url {
            let ns_url = NSURL::URLWithString(&NSString::from_str(u));
            if let Some(ns_url) = ns_url {
                unsafe { event.setURL(Some(&ns_url)) };
            }
        }

        if let Some(text) = notes {
            let ns_notes = NSString::from_str(text);
            unsafe { event.setNotes(Some(&ns_notes)) };
        }

        if let Some(name) = calendar_name {
            let cal = self.find_calendar(name)?;
            unsafe { event.setCalendar(Some(&cal)) };
        } else {
            let default_cal = unsafe { self.store.defaultCalendarForNewEvents() };
            if let Some(cal) = default_cal {
                unsafe { event.setCalendar(Some(&cal)) };
            }
        }

        if let Some(freq_str) = repeat {
            let freq = match freq_str {
                "daily" => EKRecurrenceFrequency::Daily,
                "weekly" => EKRecurrenceFrequency::Weekly,
                "monthly" => EKRecurrenceFrequency::Monthly,
                "yearly" => EKRecurrenceFrequency::Yearly,
                _ => {
                    return Err(AppError::InvalidArgument(format!(
                        "Unknown repeat frequency: {freq_str}. Use daily, weekly, monthly, or yearly."
                    )));
                }
            };
            let end = repeat_count
                .map(|n| unsafe { EKRecurrenceEnd::recurrenceEndWithOccurrenceCount(n as usize) });
            let interval = repeat_interval.unwrap_or(1) as isize;
            let rule = unsafe {
                EKRecurrenceRule::initRecurrenceWithFrequency_interval_end(
                    EKRecurrenceRule::alloc(),
                    freq,
                    interval,
                    end.as_deref(),
                )
            };
            unsafe {
                event.addRecurrenceRule(&rule);
            }
        }

        for &minutes in alerts {
            let offset = -(minutes * 60) as f64;
            let alarm = unsafe { EKAlarm::alarmWithRelativeOffset(offset) };
            unsafe { event.addAlarm(&alarm) };
        }

        unsafe {
            self.store
                .saveEvent_span_error(&event, EKSpan::ThisEvent)
                .map_err(|e| AppError::EventKit(e.to_string()))?;
        }

        let event_id = unsafe { event.eventIdentifier() }
            .map(|i| i.to_string())
            .unwrap_or_default();

        Ok(event_id)
    }

    pub fn get_event(&self, event_id: &str) -> Result<EventInfo, AppError> {
        let ns_id = NSString::from_str(event_id);
        let event = unsafe { self.store.eventWithIdentifier(&ns_id) }
            .ok_or_else(|| AppError::EventNotFound(event_id.to_string()))?;

        let title = unsafe { event.title() }.to_string();
        let calendar = unsafe { event.calendar() }
            .map(|c| unsafe { c.title() }.to_string())
            .unwrap_or_default();
        let start = nsdate_to_datetime(unsafe { event.startDate() });
        let all_day = unsafe { event.isAllDay() };
        let end = normalize_all_day_end(nsdate_to_datetime(unsafe { event.endDate() }), all_day);
        let location = unsafe { event.location() }.map(|l| l.to_string());
        let url = unsafe { event.URL() }.and_then(|u| u.absoluteString().map(|s| s.to_string()));
        let notes = unsafe { event.notes() }.map(|n| n.to_string());
        let id = unsafe { event.eventIdentifier() }
            .map(|i| i.to_string())
            .unwrap_or_default();
        let status = match unsafe { event.status() } {
            EKEventStatus::Confirmed => "confirmed",
            EKEventStatus::Tentative => "tentative",
            EKEventStatus::Canceled => "canceled",
            _ => "none",
        }
        .to_string();
        let availability = match unsafe { event.availability() } {
            EKEventAvailability::Busy => "busy",
            EKEventAvailability::Free => "free",
            EKEventAvailability::Tentative => "tentative",
            EKEventAvailability::Unavailable => "unavailable",
            _ => "not supported",
        }
        .to_string();
        let organizer = unsafe { event.organizer() }.map(|p| {
            unsafe { p.name() }
                .map(|n| n.to_string())
                .unwrap_or_default()
        });
        let created = unsafe { event.creationDate() }.map(nsdate_to_datetime);
        let modified = unsafe { event.lastModifiedDate() }.map(nsdate_to_datetime);
        let recurring = event_is_recurring(&event);
        let recurrence = recurrence_summary(&event);

        Ok(EventInfo {
            id,
            title,
            start,
            end,
            calendar,
            location,
            url,
            notes,
            all_day,
            status,
            availability,
            organizer,
            created,
            modified,
            recurring,
            recurrence,
        })
    }

    pub fn search_events(
        &self,
        query: &str,
        exact: bool,
        from: NaiveDate,
        to: NaiveDate,
        calendar_name: Option<&str>,
    ) -> Result<Vec<EventInfo>, AppError> {
        let events = self.events(from, to, calendar_name)?;
        let query_lower = query.to_lowercase();
        Ok(events
            .into_iter()
            .filter(|e| event_matches_query(e, &query_lower, exact))
            .collect())
    }

    pub fn find_unique_event(
        &self,
        query: &str,
        exact: bool,
        from: NaiveDate,
        to: NaiveDate,
        calendar_name: Option<&str>,
    ) -> Result<EventInfo, AppError> {
        let mut matches = self.search_events(query, exact, from, to, calendar_name)?;
        match matches.len() {
            0 => Err(AppError::EventNotFound(query.to_string())),
            1 => Ok(matches.remove(0)),
            count => {
                matches.sort_by_key(|e| e.start);
                let preview = matches
                    .iter()
                    .take(5)
                    .map(|e| {
                        format!(
                            "{} [{}] {} ({})",
                            e.title,
                            e.calendar,
                            e.start.format("%Y-%m-%d %H:%M"),
                            e.id
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                Err(AppError::InvalidArgument(format!(
                    "Query matched {count} events. Narrow it with --exact/--in-calendar/--from/--to. Matches: {preview}"
                )))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_event(
        &self,
        event_id: &str,
        selected_start: DateTime<Local>,
        title: Option<&str>,
        start: Option<NaiveDateTime>,
        end: Option<NaiveDateTime>,
        location: Option<&str>,
        url: Option<&str>,
        notes: Option<&str>,
        calendar_name: Option<&str>,
        all_day: Option<bool>,
        scope: Option<RecurrenceScope>,
    ) -> Result<(), AppError> {
        let event = self.find_event_instance(event_id, selected_start)?;
        let current_start = nsdate_to_datetime(unsafe { event.startDate() });
        let current_end = nsdate_to_datetime(unsafe { event.endDate() });
        let updated_all_day = all_day.unwrap_or_else(|| unsafe { event.isAllDay() });
        let (updated_start, updated_end) =
            validate_updated_time_range(current_start, current_end, start, end, updated_all_day)?;

        if let Some(t) = title {
            let ns = NSString::from_str(t);
            unsafe { event.setTitle(Some(&ns)) };
        }

        if let Some(loc) = location {
            let ns = NSString::from_str(loc);
            unsafe { event.setLocation(Some(&ns)) };
        }

        if let Some(u) = url {
            let ns_url = NSURL::URLWithString(&NSString::from_str(u));
            if let Some(ns_url) = ns_url {
                unsafe { event.setURL(Some(&ns_url)) };
            }
        }

        if start.is_some() || end.is_some() || all_day.is_some() {
            let (event_start, event_end) =
                eventkit_update_range(updated_start, updated_end, updated_all_day);
            let start_date = datetime_to_nsdate(event_start);
            let end_date = datetime_to_nsdate(event_end);
            unsafe {
                event.setStartDate(Some(&start_date));
                event.setEndDate(Some(&end_date));
            };
        }

        if let Some(n) = notes {
            let ns = NSString::from_str(n);
            unsafe { event.setNotes(Some(&ns)) };
        }

        if let Some(name) = calendar_name {
            let cal = self.find_calendar(name)?;
            unsafe { event.setCalendar(Some(&cal)) };
        }

        if let Some(ad) = all_day {
            unsafe { event.setAllDay(ad) };
        }

        unsafe {
            self.store
                .saveEvent_span_error(&event, recurrence_span(&event, scope)?)
                .map_err(|e| AppError::EventKit(e.to_string()))?;
        }
        Ok(())
    }

    pub fn delete_event(
        &self,
        event_id: &str,
        selected_start: DateTime<Local>,
        scope: Option<RecurrenceScope>,
    ) -> Result<(), AppError> {
        let event = self.find_event_instance(event_id, selected_start)?;

        unsafe {
            self.store
                .removeEvent_span_error(&event, recurrence_span(&event, scope)?)
                .map_err(|e| AppError::EventKit(e.to_string()))?;
        }
        Ok(())
    }

    /// Find events that conflict with a given time range.
    pub fn conflicts(
        &self,
        start: NaiveDateTime,
        end: NaiveDateTime,
        calendar_name: Option<&str>,
    ) -> Result<Vec<EventInfo>, AppError> {
        let start_date = start.date();
        let end_date = end.date();
        let events = self.events(start_date, end_date, calendar_name)?;

        let start_local = localize_datetime(start)?;
        let end_local = localize_datetime(end)?;

        Ok(events
            .into_iter()
            .filter(|e| !e.all_day && e.start < end_local && e.end > start_local)
            .collect())
    }

    /// Find free time slots in a date range.
    pub fn free_slots(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        after_time: Option<chrono::NaiveTime>,
        before_time: Option<chrono::NaiveTime>,
        min_duration_mins: u32,
        calendar_name: Option<&str>,
    ) -> Result<Vec<FreeSlot>, AppError> {
        let events = self.events(from, to, calendar_name)?;

        let day_start = after_time.unwrap_or(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let day_end = before_time.unwrap_or(chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap());
        let min_dur = chrono::Duration::minutes(min_duration_mins as i64);

        let mut slots = Vec::new();
        let mut current_date = from;

        while current_date <= to {
            let window_start = Local
                .from_local_datetime(&current_date.and_time(day_start))
                .earliest()
                .unwrap_or_else(Local::now);
            let window_end = Local
                .from_local_datetime(&current_date.and_time(day_end))
                .earliest()
                .unwrap_or_else(Local::now);

            // Get events for this day, sorted by start
            let mut day_events: Vec<&EventInfo> = events
                .iter()
                .filter(|e| {
                    !e.all_day
                        && e.start.date_naive() == current_date
                        && e.end > window_start
                        && e.start < window_end
                })
                .collect();
            day_events.sort_by_key(|e| e.start);

            let mut cursor = window_start;
            for ev in &day_events {
                let ev_start = ev.start.max(window_start);
                if ev_start > cursor && (ev_start - cursor) >= min_dur {
                    slots.push(FreeSlot {
                        start: cursor,
                        end: ev_start,
                        duration_mins: (ev_start - cursor).num_minutes() as u32,
                    });
                }
                cursor = cursor.max(ev.end);
            }
            if window_end > cursor && (window_end - cursor) >= min_dur {
                slots.push(FreeSlot {
                    start: cursor,
                    end: window_end,
                    duration_mins: (window_end - cursor).num_minutes() as u32,
                });
            }

            current_date += chrono::Duration::days(1);
        }

        Ok(slots)
    }

    fn find_calendar(&self, name: &str) -> Result<Retained<EKCalendar>, AppError> {
        let cals = unsafe { self.store.calendarsForEntityType(EKEntityType::Event) };
        let count = cals.count();

        for i in 0..count {
            let cal = cals.objectAtIndex(i);
            let title = unsafe { cal.title() }.to_string();
            if title == name {
                return Ok(cal.clone());
            }
        }
        Err(AppError::CalendarNotFound(name.to_string()))
    }

    fn find_event_instance(
        &self,
        event_id: &str,
        selected_start: DateTime<Local>,
    ) -> Result<Retained<EKEvent>, AppError> {
        let day = selected_start.date_naive();
        let from_ts = Local
            .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .ok_or_else(|| AppError::InvalidDate(day.to_string()))?
            .timestamp() as f64;
        let to_ts = Local
            .from_local_datetime(&day.and_hms_opt(23, 59, 59).unwrap())
            .earliest()
            .ok_or_else(|| AppError::InvalidDate(day.to_string()))?
            .timestamp() as f64;
        let start_date = NSDate::dateWithTimeIntervalSince1970(from_ts);
        let end_date = NSDate::dateWithTimeIntervalSince1970(to_ts);
        let predicate = unsafe {
            self.store
                .predicateForEventsWithStartDate_endDate_calendars(&start_date, &end_date, None)
        };
        let events = unsafe { self.store.eventsMatchingPredicate(&predicate) };
        let count = events.count();

        for i in 0..count {
            let event = events.objectAtIndex(i);
            let matches_id = unsafe { event.eventIdentifier() }
                .map(|id| id.to_string())
                .is_some_and(|id| id == event_id);
            let matches_start = nsdate_to_datetime(unsafe { event.startDate() }) == selected_start;
            if matches_id && matches_start {
                return Ok(event.clone());
            }
        }

        let ns_id = NSString::from_str(event_id);
        unsafe { self.store.eventWithIdentifier(&ns_id) }
            .ok_or_else(|| AppError::EventNotFound(event_id.to_string()))
    }
}

fn localize_datetime(dt: NaiveDateTime) -> Result<DateTime<Local>, AppError> {
    Local
        .from_local_datetime(&dt)
        .earliest()
        .ok_or_else(|| AppError::InvalidDate(dt.to_string()))
}

fn localize_date_start(date: NaiveDate) -> Result<DateTime<Local>, AppError> {
    localize_datetime(date.and_hms_opt(0, 0, 0).unwrap())
}

fn datetime_to_nsdate(dt: DateTime<Local>) -> Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(dt.timestamp() as f64)
}

fn eventkit_update_range(
    start: DateTime<Local>,
    end: DateTime<Local>,
    all_day: bool,
) -> (DateTime<Local>, DateTime<Local>) {
    if all_day {
        (start, end - chrono::Duration::days(1))
    } else {
        (start, end)
    }
}

fn validate_updated_time_range(
    current_start: DateTime<Local>,
    current_end: DateTime<Local>,
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
    all_day: bool,
) -> Result<(DateTime<Local>, DateTime<Local>), AppError> {
    if all_day {
        let midnight = chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        if start.is_some_and(|dt| dt.time() != midnight)
            || end.is_some_and(|dt| dt.time() != midnight)
        {
            return Err(AppError::InvalidArgument(
                "all-day events require date-only values for --start/--end".to_string(),
            ));
        }

        let current_end_date = current_all_day_end_date(current_start, current_end);
        let updated_start_date = start
            .map(|dt| dt.date())
            .unwrap_or(current_start.date_naive());
        let updated_end_date = end.map(|dt| dt.date()).unwrap_or(current_end_date);

        if updated_end_date < updated_start_date {
            return Err(AppError::InvalidDate(
                "end date must be on or after start date".to_string(),
            ));
        }

        let updated_start = localize_date_start(updated_start_date)?;
        let updated_end = localize_date_start(updated_end_date + chrono::Duration::days(1))?;
        return Ok((updated_start, updated_end));
    }

    let updated_start = match start {
        Some(start) => localize_datetime(start)?,
        None => current_start,
    };
    let updated_end = match end {
        Some(end) => localize_datetime(end)?,
        None => current_end,
    };

    if updated_end < updated_start {
        return Err(AppError::InvalidDate(
            "end time must be after start time".to_string(),
        ));
    }

    Ok((updated_start, updated_end))
}

fn current_all_day_end_date(
    current_start: DateTime<Local>,
    current_end: DateTime<Local>,
) -> NaiveDate {
    let current_end_date = current_end.date_naive();
    if current_end.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        && current_end_date > current_start.date_naive()
    {
        current_end_date - chrono::Duration::days(1)
    } else {
        current_end_date
    }
}

fn event_is_recurring(event: &EKEvent) -> bool {
    unsafe { event.recurrenceRules() }
        .map(|rules| rules.count() > 0)
        .unwrap_or(false)
}

fn normalize_all_day_end(end: DateTime<Local>, all_day: bool) -> DateTime<Local> {
    if all_day && end.time() == chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap() {
        end + chrono::Duration::seconds(1)
    } else {
        end
    }
}

fn recurrence_span(event: &EKEvent, scope: Option<RecurrenceScope>) -> Result<EKSpan, AppError> {
    if !event_is_recurring(event) {
        return Ok(EKSpan::ThisEvent);
    }

    match scope {
        Some(RecurrenceScope::This) => Ok(EKSpan::ThisEvent),
        Some(RecurrenceScope::Future) => Ok(EKSpan::FutureEvents),
        None => Err(AppError::InvalidArgument(
            "Recurring events require --scope this or --scope future.".to_string(),
        )),
    }
}

fn recurrence_summary(event: &EKEvent) -> Option<String> {
    let rules = unsafe { event.recurrenceRules() }?;
    let rule = rules.firstObject()?;
    let every = match unsafe { rule.frequency() } {
        EKRecurrenceFrequency::Daily => "day",
        EKRecurrenceFrequency::Weekly => "week",
        EKRecurrenceFrequency::Monthly => "month",
        EKRecurrenceFrequency::Yearly => "year",
        _ => "interval",
    };
    let interval = unsafe { rule.interval() };
    let mut summary = if interval <= 1 {
        format!("Every {every}")
    } else {
        format!("Every {interval} {every}s")
    };

    if let Some(end) = unsafe { rule.recurrenceEnd() } {
        let count = unsafe { end.occurrenceCount() };
        if count > 0 {
            summary.push_str(&format!(" for {count} occurrences"));
        } else if let Some(until) = unsafe { end.endDate() } {
            summary.push_str(&format!(
                " until {}",
                nsdate_to_datetime(until).format("%Y-%m-%d")
            ));
        }
    }

    Some(summary)
}

fn event_matches_query(event: &EventInfo, query_lower: &str, exact: bool) -> bool {
    let matches = |value: &str| {
        if exact {
            value.to_lowercase() == query_lower
        } else {
            value.to_lowercase().contains(query_lower)
        }
    };

    matches(&event.title)
        || matches(&event.calendar)
        || event.location.as_deref().is_some_and(matches)
        || event.url.as_deref().is_some_and(matches)
        || event.notes.as_deref().is_some_and(matches)
}

fn nsdate_to_datetime(date: Retained<NSDate>) -> DateTime<Local> {
    let ts = date.timeIntervalSince1970();
    DateTime::from_timestamp(ts as i64, 0)
        .map(|dt| dt.with_timezone(&Local))
        .unwrap_or_else(Local::now)
}

#[cfg(test)]
mod tests {
    use super::{
        EventInfo, event_matches_query, normalize_all_day_end, validate_updated_time_range,
    };
    use crate::error::AppError;
    use chrono::{Local, NaiveDate, TimeZone};

    fn local_dt(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> chrono::DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap();
        Local.from_local_datetime(&naive).earliest().unwrap()
    }

    #[test]
    fn test_validate_updated_time_range_rejects_start_after_current_end() {
        let current_start = local_dt(2026, 3, 20, 10, 0);
        let current_end = local_dt(2026, 3, 20, 11, 0);
        let new_start = NaiveDate::from_ymd_opt(2026, 3, 20)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();

        let err =
            validate_updated_time_range(current_start, current_end, Some(new_start), None, false)
                .unwrap_err();

        assert!(matches!(err, AppError::InvalidDate(_)));
    }

    #[test]
    fn test_validate_updated_time_range_rejects_end_before_current_start() {
        let current_start = local_dt(2026, 3, 20, 10, 0);
        let current_end = local_dt(2026, 3, 20, 11, 0);
        let new_end = NaiveDate::from_ymd_opt(2026, 3, 20)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();

        let err =
            validate_updated_time_range(current_start, current_end, None, Some(new_end), false)
                .unwrap_err();

        assert!(matches!(err, AppError::InvalidDate(_)));
    }

    #[test]
    fn test_validate_updated_time_range_normalizes_all_day_end_exclusive() {
        let current_start = local_dt(2026, 3, 20, 0, 0);
        let current_end = local_dt(2026, 3, 21, 0, 0);
        let new_end = NaiveDate::from_ymd_opt(2026, 3, 22)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let (updated_start, updated_end) =
            validate_updated_time_range(current_start, current_end, None, Some(new_end), true)
                .unwrap();

        assert_eq!(
            updated_start.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 20).unwrap()
        );
        assert_eq!(
            updated_end.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 23).unwrap()
        );
    }

    #[test]
    fn test_validate_updated_time_range_rejects_timed_values_for_all_day() {
        let current_start = local_dt(2026, 3, 20, 0, 0);
        let current_end = local_dt(2026, 3, 21, 0, 0);
        let timed = NaiveDate::from_ymd_opt(2026, 3, 22)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();

        let err = validate_updated_time_range(current_start, current_end, Some(timed), None, true)
            .unwrap_err();

        assert!(matches!(err, AppError::InvalidArgument(_)));
    }

    #[test]
    fn test_normalize_all_day_end_converts_inclusive_last_second() {
        let end = local_dt(2026, 3, 20, 23, 59);
        let normalized = normalize_all_day_end(end + chrono::Duration::seconds(59), true);
        assert_eq!(
            normalized.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 21).unwrap()
        );
        assert_eq!(
            normalized.time(),
            chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        );
    }

    fn make_event(title: &str, calendar: &str, notes: Option<&str>) -> EventInfo {
        let start = local_dt(2026, 3, 20, 10, 0);
        let end = local_dt(2026, 3, 20, 11, 0);
        EventInfo {
            id: "event-1".to_string(),
            title: title.to_string(),
            start,
            end,
            calendar: calendar.to_string(),
            location: Some("Room 1".to_string()),
            url: Some("https://example.com".to_string()),
            notes: notes.map(str::to_string),
            all_day: false,
            status: "confirmed".to_string(),
            availability: "busy".to_string(),
            organizer: None,
            created: None,
            modified: None,
            recurring: false,
            recurrence: None,
        }
    }

    #[test]
    fn test_event_matches_query_partial() {
        let event = make_event("Weekly Standup", "Work", Some("notes"));
        assert!(event_matches_query(&event, "stand", false));
        assert!(event_matches_query(&event, "room", false));
    }

    #[test]
    fn test_event_matches_query_exact() {
        let event = make_event("Weekly Standup", "Work", Some("notes"));
        assert!(event_matches_query(&event, "weekly standup", true));
        assert!(!event_matches_query(&event, "stand", true));
    }
}
