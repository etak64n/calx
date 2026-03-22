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

#[derive(Clone, Serialize)]
pub struct CalendarInfo {
    pub id: String,
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
    pub calendar_id: String,
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
    pub recurrence_rule: Option<RecurrenceRuleInfo>,
    pub alerts: Vec<i64>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub enum RecurrenceScope {
    This,
    Future,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldUpdate<'a> {
    Keep,
    Set(&'a str),
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertUpdate<'a> {
    Keep,
    Set(&'a [i64]),
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct RecurrenceRuleInfo {
    pub frequency: String,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<NaiveDate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct EventDraft {
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub calendar: String,
    pub calendar_id: String,
    pub location: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub all_day: bool,
    pub alerts: Vec<i64>,
    pub recurrence_rule: Option<RecurrenceRuleInfo>,
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
            let id = unsafe { cal.calendarIdentifier() }.to_string();
            let title = unsafe { cal.title() }.to_string();
            let source = unsafe { cal.source() }
                .map(|s| unsafe { s.title() }.to_string())
                .unwrap_or_default();
            result.push(CalendarInfo { id, title, source });
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
            let (cal_name, cal_id) = event_calendar_ref(&event);

            if let Some(spec) = calendar_name {
                if cal_name != spec && cal_id != spec {
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
            let recurrence_rule = recurrence_rule_info(&event);
            let alerts = alarm_minutes_before(&event, start);

            result.push(EventInfo {
                id,
                title,
                start,
                end,
                calendar: cal_name,
                calendar_id: cal_id,
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
                recurrence_rule,
                alerts,
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
        validate_repeat_configuration(repeat, repeat_count, repeat_interval)?;
        validate_alert_minutes(alerts)?;
        let recurrence_rule = recurrence_spec_from_inputs(repeat, repeat_count, repeat_interval)?;
        let start_local = localize_datetime(start)?;
        let end_local = localize_datetime(end)?;
        let event = self.build_event(
            title,
            start_local,
            end_local,
            calendar_name,
            location,
            url,
            notes,
            all_day,
            recurrence_rule.as_ref(),
            alerts,
        )?;

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

    pub fn event_draft(
        &self,
        event_id: &str,
        selected_start: DateTime<Local>,
    ) -> Result<EventDraft, AppError> {
        let event = self.find_event_instance(event_id, selected_start)?;
        Ok(event_draft_from_event(&event))
    }

    pub fn create_event_from_draft(
        &self,
        draft: &EventDraft,
        title_override: Option<&str>,
        start: DateTime<Local>,
        end: DateTime<Local>,
        calendar_override: Option<&str>,
        keep_recurrence: bool,
    ) -> Result<EventInfo, AppError> {
        let event = self.build_event(
            title_override.unwrap_or(&draft.title),
            start,
            end,
            calendar_override.or(Some(draft.calendar_id.as_str())),
            draft.location.as_deref(),
            draft.url.as_deref(),
            draft.notes.as_deref(),
            draft.all_day,
            keep_recurrence
                .then_some(draft.recurrence_rule.as_ref())
                .flatten(),
            &draft.alerts,
        )?;

        unsafe {
            self.store
                .saveEvent_span_error(&event, EKSpan::ThisEvent)
                .map_err(|e| AppError::EventKit(e.to_string()))?;
        }

        let event_id = unsafe { event.eventIdentifier() }
            .map(|id| id.to_string())
            .unwrap_or_default();
        self.get_event(&event_id)
    }

    pub fn get_event(&self, event_id: &str) -> Result<EventInfo, AppError> {
        let ns_id = NSString::from_str(event_id);
        let event = unsafe { self.store.eventWithIdentifier(&ns_id) }
            .ok_or_else(|| AppError::EventNotFound(event_id.to_string()))?;

        let title = unsafe { event.title() }.to_string();
        let (calendar, calendar_id) = event_calendar_ref(&event);
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
        let recurrence_rule = recurrence_rule_info(&event);
        let alerts = alarm_minutes_before(&event, start);

        Ok(EventInfo {
            id,
            title,
            start,
            end,
            calendar,
            calendar_id,
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
            recurrence_rule,
            alerts,
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
        Ok(filter_search_matches(events, &query_lower, exact))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_event(
        &self,
        event_id: &str,
        selected_start: DateTime<Local>,
        title: Option<&str>,
        start: Option<NaiveDateTime>,
        end: Option<NaiveDateTime>,
        location: FieldUpdate<'_>,
        url: FieldUpdate<'_>,
        notes: FieldUpdate<'_>,
        alerts: AlertUpdate<'_>,
        calendar_name: Option<&str>,
        all_day: Option<bool>,
        scope: Option<RecurrenceScope>,
    ) -> Result<EventInfo, AppError> {
        let event = self.find_event_instance(event_id, selected_start)?;
        let current_all_day = unsafe { event.isAllDay() };
        let current_start = nsdate_to_datetime(unsafe { event.startDate() });
        let current_end = nsdate_to_datetime(unsafe { event.endDate() });
        let updated_all_day = all_day.unwrap_or(current_all_day);
        validate_all_day_transition(current_all_day, updated_all_day, start, end)?;
        let (updated_start, updated_end) =
            validate_updated_time_range(current_start, current_end, start, end, updated_all_day)?;

        if let Some(t) = title {
            let ns = NSString::from_str(t);
            unsafe { event.setTitle(Some(&ns)) };
        }

        match location {
            FieldUpdate::Keep => {}
            FieldUpdate::Set(loc) => {
                let ns = NSString::from_str(loc);
                unsafe { event.setLocation(Some(&ns)) };
            }
            FieldUpdate::Clear => unsafe { event.setLocation(None) },
        }

        match url {
            FieldUpdate::Keep => {}
            FieldUpdate::Set(u) => {
                let ns_url = parse_nsurl(u)?;
                unsafe { event.setURL(Some(&ns_url)) };
            }
            FieldUpdate::Clear => unsafe { event.setURL(None) },
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

        match notes {
            FieldUpdate::Keep => {}
            FieldUpdate::Set(n) => {
                let ns = NSString::from_str(n);
                unsafe { event.setNotes(Some(&ns)) };
            }
            FieldUpdate::Clear => unsafe { event.setNotes(None) },
        }

        match alerts {
            AlertUpdate::Keep => {}
            AlertUpdate::Set(minutes) => apply_alerts(&event, minutes)?,
            AlertUpdate::Clear => apply_alerts(&event, &[])?,
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
        let updated_id = unsafe { event.eventIdentifier() }
            .map(|id| id.to_string())
            .unwrap_or_else(|| event_id.to_string());
        self.get_event(&updated_id)
    }

    pub fn restore_event_from_draft(
        &self,
        event_id: &str,
        selected_start: DateTime<Local>,
        scope: Option<RecurrenceScope>,
        draft: &EventDraft,
    ) -> Result<EventInfo, AppError> {
        let event = self.find_event_instance(event_id, selected_start)?;
        let save_span = recurrence_save_span(event_is_recurring(&event), scope)?;
        let calendar = self.find_calendar(draft.calendar_id.as_str())?;
        let parsed_url = draft.url.as_deref().map(parse_nsurl).transpose()?;

        if !draft.all_day && draft.end <= draft.start {
            return Err(AppError::InvalidDate(
                "end time must be after start time".to_string(),
            ));
        }

        let (event_start, event_end) = eventkit_update_range(draft.start, draft.end, draft.all_day);
        let start_date = datetime_to_nsdate(event_start);
        let end_date = datetime_to_nsdate(event_end);

        let title = NSString::from_str(&draft.title);
        unsafe { event.setTitle(Some(&title)) };
        unsafe {
            event.setStartDate(Some(&start_date));
            event.setEndDate(Some(&end_date));
            event.setAllDay(draft.all_day);
            event.setCalendar(Some(&calendar));
        }

        match &draft.location {
            Some(location) => {
                let ns = NSString::from_str(location);
                unsafe { event.setLocation(Some(&ns)) };
            }
            None => unsafe { event.setLocation(None) },
        }

        match parsed_url {
            Some(url) => unsafe { event.setURL(Some(&url)) },
            None => unsafe { event.setURL(None) },
        }

        match &draft.notes {
            Some(notes) => {
                let ns = NSString::from_str(notes);
                unsafe { event.setNotes(Some(&ns)) };
            }
            None => unsafe { event.setNotes(None) },
        }

        apply_alerts(&event, &draft.alerts)?;
        apply_recurrence_rule(&event, draft.recurrence_rule.as_ref(), draft.all_day)?;

        unsafe {
            self.store
                .saveEvent_span_error(&event, save_span)
                .map_err(|e| AppError::EventKit(e.to_string()))?;
        }

        let restored_id = unsafe { event.eventIdentifier() }
            .map(|id| id.to_string())
            .unwrap_or_else(|| event_id.to_string());
        self.get_event(&restored_id)
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
            .filter(|e| event_conflicts_range(e, start_local, end_local))
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
        Ok(calculate_free_slots(
            &events,
            from,
            to,
            after_time,
            before_time,
            min_duration_mins,
        ))
    }

    pub fn default_calendar(&self) -> Option<CalendarInfo> {
        unsafe { self.store.defaultCalendarForNewEvents() }.map(|cal| CalendarInfo {
            id: unsafe { cal.calendarIdentifier() }.to_string(),
            title: unsafe { cal.title() }.to_string(),
            source: unsafe { cal.source() }
                .map(|s| unsafe { s.title() }.to_string())
                .unwrap_or_default(),
        })
    }

    pub fn is_calendar_writable(&self, name: &str) -> bool {
        let cals = unsafe { self.store.calendarsForEntityType(EKEntityType::Event) };
        let count = cals.count();
        for i in 0..count {
            let cal = cals.objectAtIndex(i);
            if calendar_matches(&cal, name) {
                return unsafe { cal.allowsContentModifications() };
            }
        }
        false
    }

    fn find_calendar(&self, name: &str) -> Result<Retained<EKCalendar>, AppError> {
        let resolved_id = resolve_calendar_identifier(name, &self.calendars())?;
        let cals = unsafe { self.store.calendarsForEntityType(EKEntityType::Event) };
        let count = cals.count();

        for i in 0..count {
            let cal = cals.objectAtIndex(i);
            if unsafe { cal.calendarIdentifier() }.to_string() == resolved_id {
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

    #[allow(clippy::too_many_arguments)]
    fn build_event(
        &self,
        title: &str,
        start: DateTime<Local>,
        end: DateTime<Local>,
        calendar_name: Option<&str>,
        location: Option<&str>,
        url: Option<&str>,
        notes: Option<&str>,
        all_day: bool,
        recurrence_rule: Option<&RecurrenceRuleInfo>,
        alerts: &[i64],
    ) -> Result<Retained<EKEvent>, AppError> {
        if !all_day && end <= start {
            return Err(AppError::InvalidDate(
                "end time must be after start time".to_string(),
            ));
        }

        let event = unsafe { EKEvent::eventWithEventStore(&self.store) };
        let ns_title = NSString::from_str(title);
        unsafe { event.setTitle(Some(&ns_title)) };

        let start_date = datetime_to_nsdate(start);
        let end_date = datetime_to_nsdate(end);
        unsafe {
            event.setStartDate(Some(&start_date));
            event.setEndDate(Some(&end_date));
            event.setAllDay(all_day);
        }

        if let Some(loc) = location {
            let ns = NSString::from_str(loc);
            unsafe { event.setLocation(Some(&ns)) };
        }

        if let Some(u) = url {
            let ns_url = parse_nsurl(u)?;
            unsafe { event.setURL(Some(&ns_url)) };
        }

        if let Some(text) = notes {
            let ns_notes = NSString::from_str(text);
            unsafe { event.setNotes(Some(&ns_notes)) };
        }

        let calendar = match calendar_name {
            Some(name) => self.find_calendar(name)?,
            None => unsafe { self.store.defaultCalendarForNewEvents() }.ok_or_else(|| {
                AppError::InvalidArgument(
                    "No default calendar is configured. Use --calendar.".to_string(),
                )
            })?,
        };
        unsafe { event.setCalendar(Some(&calendar)) };

        apply_recurrence_rule(&event, recurrence_rule, all_day)?;
        apply_alerts(&event, alerts)?;

        Ok(event)
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

pub(crate) fn validate_url_string(url: &str) -> Result<(), AppError> {
    parse_nsurl(url).map(|_| ())
}

fn validate_repeat_configuration(
    repeat: Option<&str>,
    repeat_count: Option<u32>,
    repeat_interval: Option<u32>,
) -> Result<(), AppError> {
    if repeat.is_none() {
        if repeat_count.is_some() {
            return Err(AppError::InvalidArgument(
                "--repeat-count requires --repeat".to_string(),
            ));
        }
        if repeat_interval.is_some() {
            return Err(AppError::InvalidArgument(
                "--repeat-interval requires --repeat".to_string(),
            ));
        }
        return Ok(());
    }

    if repeat_count == Some(0) {
        return Err(AppError::InvalidArgument(
            "--repeat-count must be greater than 0".to_string(),
        ));
    }
    if repeat_interval == Some(0) {
        return Err(AppError::InvalidArgument(
            "--repeat-interval must be greater than 0".to_string(),
        ));
    }

    Ok(())
}

fn validate_alert_minutes(alerts: &[i64]) -> Result<(), AppError> {
    if let Some(minutes) = alerts.iter().find(|&&minutes| minutes < 0) {
        return Err(AppError::InvalidArgument(format!(
            "--alert expects minutes before the event; got {minutes}"
        )));
    }
    Ok(())
}

fn recurrence_spec_from_inputs(
    repeat: Option<&str>,
    repeat_count: Option<u32>,
    repeat_interval: Option<u32>,
) -> Result<Option<RecurrenceRuleInfo>, AppError> {
    validate_repeat_configuration(repeat, repeat_count, repeat_interval)?;
    let Some(repeat) = repeat else {
        return Ok(None);
    };

    Ok(Some(RecurrenceRuleInfo {
        frequency: repeat.to_string(),
        interval: repeat_interval.unwrap_or(1),
        count: repeat_count,
        until: None,
    }))
}

fn parse_nsurl(url: &str) -> Result<Retained<NSURL>, AppError> {
    NSURL::URLWithString(&NSString::from_str(url))
        .ok_or_else(|| AppError::InvalidArgument(format!("Invalid URL: {url}")))
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

fn calculate_free_slots(
    events: &[EventInfo],
    from: NaiveDate,
    to: NaiveDate,
    after_time: Option<chrono::NaiveTime>,
    before_time: Option<chrono::NaiveTime>,
    min_duration_mins: u32,
) -> Vec<FreeSlot> {
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

        let mut blocked: Vec<(DateTime<Local>, DateTime<Local>)> = events
            .iter()
            .filter_map(|event| blocked_interval_for_window(event, window_start, window_end))
            .collect();
        blocked.sort_by_key(|(start, _)| *start);

        let mut cursor = window_start;
        for (block_start, block_end) in blocked {
            if block_start > cursor && (block_start - cursor) >= min_dur {
                slots.push(FreeSlot {
                    start: cursor,
                    end: block_start,
                    duration_mins: (block_start - cursor).num_minutes() as u32,
                });
            }
            cursor = cursor.max(block_end);
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

    slots
}

fn validate_all_day_transition(
    current_all_day: bool,
    updated_all_day: bool,
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
) -> Result<(), AppError> {
    if current_all_day && !updated_all_day && (start.is_none() || end.is_none()) {
        return Err(AppError::InvalidArgument(
            "Converting an all-day event to a timed event requires both --start and --end."
                .to_string(),
        ));
    }
    Ok(())
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

    if updated_end <= updated_start {
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
    recurrence_save_span(event_is_recurring(event), scope)
}

fn recurrence_save_span(
    is_recurring: bool,
    scope: Option<RecurrenceScope>,
) -> Result<EKSpan, AppError> {
    if !is_recurring {
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

fn recurrence_rule_info(event: &EKEvent) -> Option<RecurrenceRuleInfo> {
    let rules = unsafe { event.recurrenceRules() }?;
    let rule = rules.firstObject()?;
    let frequency = match unsafe { rule.frequency() } {
        EKRecurrenceFrequency::Daily => "daily",
        EKRecurrenceFrequency::Weekly => "weekly",
        EKRecurrenceFrequency::Monthly => "monthly",
        EKRecurrenceFrequency::Yearly => "yearly",
        _ => "custom",
    }
    .to_string();
    let interval = unsafe { rule.interval() }.max(1) as u32;
    let (count, until) = if let Some(end) = unsafe { rule.recurrenceEnd() } {
        let count = unsafe { end.occurrenceCount() };
        if count > 0 {
            (Some(count as u32), None)
        } else {
            (
                None,
                unsafe { end.endDate() }.map(|date| nsdate_to_datetime(date).date_naive()),
            )
        }
    } else {
        (None, None)
    };

    Some(RecurrenceRuleInfo {
        frequency,
        interval,
        count,
        until,
    })
}

fn alarm_minutes_before(event: &EKEvent, start: DateTime<Local>) -> Vec<i64> {
    let Some(alarms) = (unsafe { event.alarms() }) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    for i in 0..alarms.count() {
        let alarm = alarms.objectAtIndex(i);
        let relative = unsafe { alarm.relativeOffset() };
        let minutes = if relative != 0.0 {
            Some(((-relative) / 60.0).round() as i64)
        } else {
            unsafe { alarm.absoluteDate() }.map(|absolute| {
                start
                    .signed_duration_since(nsdate_to_datetime(absolute))
                    .num_minutes()
            })
        };

        if let Some(minutes) = minutes {
            if minutes >= 0 {
                result.push(minutes);
            }
        }
    }

    result.sort_unstable();
    result.dedup();
    result
}

fn event_draft_from_event(event: &EKEvent) -> EventDraft {
    let (calendar, calendar_id) = event_calendar_ref(event);
    let start = nsdate_to_datetime(unsafe { event.startDate() });
    let all_day = unsafe { event.isAllDay() };
    let end = normalize_all_day_end(nsdate_to_datetime(unsafe { event.endDate() }), all_day);
    EventDraft {
        title: unsafe { event.title() }.to_string(),
        start,
        end,
        calendar,
        calendar_id,
        location: unsafe { event.location() }.map(|l| l.to_string()),
        url: unsafe { event.URL() }.and_then(|u| u.absoluteString().map(|s| s.to_string())),
        notes: unsafe { event.notes() }.map(|n| n.to_string()),
        all_day,
        alerts: alarm_minutes_before(event, start),
        recurrence_rule: recurrence_rule_info(event),
    }
}

fn recurrence_frequency_from_str(value: &str) -> Result<EKRecurrenceFrequency, AppError> {
    match value {
        "daily" => Ok(EKRecurrenceFrequency::Daily),
        "weekly" => Ok(EKRecurrenceFrequency::Weekly),
        "monthly" => Ok(EKRecurrenceFrequency::Monthly),
        "yearly" => Ok(EKRecurrenceFrequency::Yearly),
        other => Err(AppError::InvalidArgument(format!(
            "Unknown repeat frequency: {other}. Use daily, weekly, monthly, or yearly."
        ))),
    }
}

fn recurrence_end_for_rule(
    rule: &RecurrenceRuleInfo,
    all_day: bool,
) -> Result<Option<Retained<EKRecurrenceEnd>>, AppError> {
    if let Some(count) = rule.count {
        return Ok(Some(unsafe {
            EKRecurrenceEnd::recurrenceEndWithOccurrenceCount(count as usize)
        }));
    }

    let Some(until) = rule.until else {
        return Ok(None);
    };
    let end_of_day = until.and_hms_opt(23, 59, 59).unwrap();
    let end_local = localize_datetime(end_of_day)?;
    let _ = all_day;
    let until_date = datetime_to_nsdate(end_local);
    Ok(Some(unsafe {
        EKRecurrenceEnd::recurrenceEndWithEndDate(&until_date)
    }))
}

fn apply_recurrence_rule(
    event: &EKEvent,
    recurrence_rule: Option<&RecurrenceRuleInfo>,
    all_day: bool,
) -> Result<(), AppError> {
    match recurrence_rule {
        None => unsafe { event.setRecurrenceRules(None) },
        Some(rule_info) => {
            let frequency = recurrence_frequency_from_str(&rule_info.frequency)?;
            let recurrence_end = recurrence_end_for_rule(rule_info, all_day)?;
            let rule = unsafe {
                EKRecurrenceRule::initRecurrenceWithFrequency_interval_end(
                    EKRecurrenceRule::alloc(),
                    frequency,
                    rule_info.interval.max(1) as isize,
                    recurrence_end.as_deref(),
                )
            };
            let rules = NSArray::from_retained_slice(&[rule]);
            unsafe { event.setRecurrenceRules(Some(&rules)) };
        }
    }
    Ok(())
}

fn apply_alerts(event: &EKEvent, alerts: &[i64]) -> Result<(), AppError> {
    validate_alert_minutes(alerts)?;
    if alerts.is_empty() {
        unsafe { event.setAlarms(None) };
        return Ok(());
    }

    let alarms = alerts
        .iter()
        .map(|minutes| unsafe { EKAlarm::alarmWithRelativeOffset(-((*minutes as f64) * 60.0)) })
        .collect::<Vec<_>>();
    let alarm_array = NSArray::from_retained_slice(&alarms);
    unsafe { event.setAlarms(Some(&alarm_array)) };
    Ok(())
}

fn filter_search_matches(events: Vec<EventInfo>, query_lower: &str, exact: bool) -> Vec<EventInfo> {
    events
        .into_iter()
        .filter(|event| event.status != "canceled")
        .filter(|event| event_matches_query(event, query_lower, exact))
        .collect()
}

fn event_matches_query(event: &EventInfo, query_lower: &str, exact: bool) -> bool {
    if exact {
        return event.title.to_lowercase() == query_lower;
    }

    let matches = |value: &str| value.to_lowercase().contains(query_lower);

    matches(&event.title)
        || matches(&event.calendar)
        || matches(&event.calendar_id)
        || event.location.as_deref().is_some_and(matches)
        || event.url.as_deref().is_some_and(matches)
        || event.notes.as_deref().is_some_and(matches)
}

fn event_blocks_schedule(event: &EventInfo) -> bool {
    event.status != "canceled" && event.availability != "free"
}

fn event_conflicts_range(
    event: &EventInfo,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
) -> bool {
    event_blocks_schedule(event) && event.start < range_end && event.end > range_start
}

fn blocked_interval_for_window(
    event: &EventInfo,
    window_start: DateTime<Local>,
    window_end: DateTime<Local>,
) -> Option<(DateTime<Local>, DateTime<Local>)> {
    if !event_blocks_schedule(event) {
        return None;
    }

    if event.end <= window_start || event.start >= window_end {
        return None;
    }

    if event.all_day {
        return Some((window_start, window_end));
    }

    Some((event.start.max(window_start), event.end.min(window_end)))
}

fn resolve_calendar_identifier(spec: &str, calendars: &[CalendarInfo]) -> Result<String, AppError> {
    if let Some(cal) = calendars.iter().find(|cal| cal.id == spec) {
        return Ok(cal.id.clone());
    }

    let matches: Vec<&CalendarInfo> = calendars.iter().filter(|cal| cal.title == spec).collect();
    match matches.len() {
        0 => Err(AppError::CalendarNotFound(spec.to_string())),
        1 => Ok(matches[0].id.clone()),
        _ => {
            let preview = matches
                .iter()
                .map(|cal| format!("{} [{}] ({})", cal.title, cal.source, cal.id))
                .collect::<Vec<_>>()
                .join("; ");
            Err(AppError::InvalidArgument(format!(
                "Calendar name '{spec}' is ambiguous. Use a calendar ID instead. Matches: {preview}"
            )))
        }
    }
}

fn calendar_matches(calendar: &EKCalendar, spec: &str) -> bool {
    unsafe { calendar.title() }.to_string() == spec
        || unsafe { calendar.calendarIdentifier() }.to_string() == spec
}

fn event_calendar_ref(event: &EKEvent) -> (String, String) {
    unsafe { event.calendar() }
        .map(|calendar| {
            (
                unsafe { calendar.title() }.to_string(),
                unsafe { calendar.calendarIdentifier() }.to_string(),
            )
        })
        .unwrap_or_else(|| (String::new(), String::new()))
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
        CalendarInfo, EventInfo, blocked_interval_for_window, calculate_free_slots,
        event_blocks_schedule, event_conflicts_range, event_matches_query, filter_search_matches,
        normalize_all_day_end, resolve_calendar_identifier, validate_all_day_transition,
        validate_updated_time_range,
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
    fn test_resolve_calendar_identifier_prefers_exact_id() {
        let calendars = vec![
            CalendarInfo {
                id: "cal-1".to_string(),
                title: "Work".to_string(),
                source: "iCloud".to_string(),
            },
            CalendarInfo {
                id: "cal-2".to_string(),
                title: "Work".to_string(),
                source: "Exchange".to_string(),
            },
        ];

        assert_eq!(
            resolve_calendar_identifier("cal-2", &calendars).unwrap(),
            "cal-2"
        );
    }

    #[test]
    fn test_resolve_calendar_identifier_rejects_ambiguous_title() {
        let calendars = vec![
            CalendarInfo {
                id: "cal-1".to_string(),
                title: "Work".to_string(),
                source: "iCloud".to_string(),
            },
            CalendarInfo {
                id: "cal-2".to_string(),
                title: "Work".to_string(),
                source: "Exchange".to_string(),
            },
        ];

        let err = resolve_calendar_identifier("Work", &calendars).unwrap_err();
        assert!(matches!(err, AppError::InvalidArgument(_)));
    }

    #[test]
    fn test_filter_search_matches_skips_canceled_events() {
        let mut canceled = make_event("Review", "Work", None);
        canceled.status = "canceled".to_string();
        let visible = make_event("Review", "Work", None);

        let matches = filter_search_matches(vec![canceled, visible.clone()], "review", false);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].title, visible.title);
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
            calendar_id: "cal-1".to_string(),
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
            recurrence_rule: None,
            alerts: Vec::new(),
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
        assert!(!event_matches_query(&event, "room 1", true));
    }

    #[test]
    fn test_calculate_free_slots_blocks_all_day_events() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let event = EventInfo {
            all_day: true,
            start: local_dt(2026, 3, 20, 0, 0),
            end: local_dt(2026, 3, 21, 0, 0),
            ..make_event("PTO", "Work", None)
        };

        let slots = calculate_free_slots(
            &[event],
            day,
            day,
            Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            Some(chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            30,
        );

        assert!(slots.is_empty());
    }

    #[test]
    fn test_calculate_free_slots_blocks_multi_day_all_day_events_each_day() {
        let from = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 3, 21).unwrap();
        let event = EventInfo {
            all_day: true,
            start: local_dt(2026, 3, 20, 0, 0),
            end: local_dt(2026, 3, 22, 0, 0),
            ..make_event("Trip", "Work", None)
        };

        let slots = calculate_free_slots(
            &[event],
            from,
            to,
            Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            Some(chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            30,
        );

        assert!(slots.is_empty());
    }

    #[test]
    fn test_event_blocks_schedule_ignores_canceled_and_free_events() {
        let mut canceled = make_event("Canceled", "Work", None);
        canceled.status = "canceled".to_string();
        assert!(!event_blocks_schedule(&canceled));

        let mut free = make_event("FYI", "Work", None);
        free.availability = "free".to_string();
        assert!(!event_blocks_schedule(&free));

        let busy = make_event("Busy", "Work", None);
        assert!(event_blocks_schedule(&busy));
    }

    #[test]
    fn test_validate_all_day_transition_requires_both_bounds_when_disabling_all_day() {
        let start = NaiveDate::from_ymd_opt(2026, 3, 20)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        assert!(validate_all_day_transition(true, false, Some(start), None).is_err());
        assert!(validate_all_day_transition(true, false, None, Some(start)).is_err());
        assert!(validate_all_day_transition(true, false, Some(start), Some(start)).is_ok());
        assert!(validate_all_day_transition(false, false, None, None).is_ok());
    }

    #[test]
    fn test_event_conflicts_range_matches_all_day_overlap() {
        let event = EventInfo {
            all_day: true,
            start: local_dt(2026, 3, 20, 0, 0),
            end: local_dt(2026, 3, 21, 0, 0),
            ..make_event("PTO", "Work", None)
        };

        assert!(event_conflicts_range(
            &event,
            local_dt(2026, 3, 20, 10, 0),
            local_dt(2026, 3, 20, 11, 0),
        ));
    }

    #[test]
    fn test_event_conflicts_range_skips_canceled_events() {
        let mut event = make_event("Canceled", "Work", None);
        event.status = "canceled".to_string();

        assert!(!event_conflicts_range(
            &event,
            local_dt(2026, 3, 20, 10, 0),
            local_dt(2026, 3, 20, 11, 0),
        ));
    }

    #[test]
    fn test_blocked_interval_skips_non_blocking_events() {
        let window_start = local_dt(2026, 3, 20, 9, 0);
        let window_end = local_dt(2026, 3, 20, 17, 0);

        let mut canceled = make_event("Canceled", "Work", None);
        canceled.status = "canceled".to_string();
        assert!(blocked_interval_for_window(&canceled, window_start, window_end).is_none());

        let mut free = make_event("FYI", "Work", None);
        free.availability = "free".to_string();
        assert!(blocked_interval_for_window(&free, window_start, window_end).is_none());
    }

    #[test]
    fn test_calculate_free_slots_ignores_free_and_canceled_events() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();

        let mut free = make_event("FYI", "Work", None);
        free.availability = "free".to_string();

        let mut canceled = make_event("Canceled", "Work", None);
        canceled.status = "canceled".to_string();

        let slots = calculate_free_slots(
            &[free, canceled],
            day,
            day,
            Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
            Some(chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap()),
            30,
        );

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].start, local_dt(2026, 3, 20, 9, 0));
        assert_eq!(slots[0].end, local_dt(2026, 3, 20, 17, 0));
    }
}
