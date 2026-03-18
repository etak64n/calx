use crate::error::AppError;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
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

#[derive(Serialize)]
pub struct EventInfo {
    pub id: String,
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub calendar: String,
    pub notes: Option<String>,
    pub all_day: bool,
}

pub struct CalendarStore {
    store: Retained<EKEventStore>,
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
            let end = nsdate_to_datetime(unsafe { event.endDate() });
            let notes = unsafe { event.notes() }.map(|n| n.to_string());
            let all_day = unsafe { event.isAllDay() };
            let id = unsafe { event.eventIdentifier() }
                .map(|i| i.to_string())
                .unwrap_or_default();

            result.push(EventInfo {
                id,
                title,
                start,
                end,
                calendar: cal_name,
                notes,
                all_day,
            });
        }

        Ok(result)
    }

    pub fn add_event(
        &self,
        title: &str,
        start: NaiveDateTime,
        end: NaiveDateTime,
        calendar_name: Option<&str>,
        notes: Option<&str>,
        all_day: bool,
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

    pub fn delete_event(&self, event_id: &str) -> Result<(), AppError> {
        let ns_id = NSString::from_str(event_id);
        let event = unsafe { self.store.eventWithIdentifier(&ns_id) }
            .ok_or_else(|| AppError::EventNotFound(event_id.to_string()))?;

        unsafe {
            self.store
                .removeEvent_span_error(&event, EKSpan::ThisEvent)
                .map_err(|e| AppError::EventKit(e.to_string()))?;
        }
        Ok(())
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
}

fn nsdate_to_datetime(date: Retained<NSDate>) -> DateTime<Local> {
    let ts = date.timeIntervalSince1970();
    DateTime::from_timestamp(ts as i64, 0)
        .map(|dt| dt.with_timezone(&Local))
        .unwrap_or_else(Local::now)
}
