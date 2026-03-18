use block2::RcBlock;
use chrono::{Duration, Local, NaiveDate};
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::*;
use objc2_foundation::*;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::ffi::OsStr;
use std::process::Command;
use std::sync::{Mutex, OnceLock, mpsc};
use std::thread::sleep;
use std::time::{SystemTime, UNIX_EPOCH};

static LIVE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn calx<I, S>(args: I) -> (String, String, i32)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(env!("CARGO_BIN_EXE_calx"))
        .args(args)
        .output()
        .expect("failed to execute calx");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

fn parse_success_json<T: DeserializeOwned>(args: &[String]) -> T {
    let (stdout, stderr, code) = calx(args);
    assert_eq!(
        code, 0,
        "command failed:\nargs={args:?}\nstdout={stdout}\nstderr={stderr}"
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("failed to parse JSON:\nargs={args:?}\nstdout={stdout}\nstderr={stderr}\nerror={e}")
    })
}

fn wait_for_success_json<T, F, P>(mut args_fn: F, predicate: P) -> T
where
    T: DeserializeOwned,
    F: FnMut() -> Vec<String>,
    P: Fn(&T) -> bool,
{
    let mut last_error = String::new();
    for attempt in 0..20 {
        let args = args_fn();
        let (stdout, stderr, code) = calx(&args);
        if code == 0 {
            match serde_json::from_str::<T>(&stdout) {
                Ok(value) if predicate(&value) => return value,
                Ok(_) => {
                    last_error = format!(
                        "predicate not satisfied on attempt {}:\nargs={args:?}\nstdout={stdout}\nstderr={stderr}",
                        attempt + 1
                    );
                }
                Err(e) => {
                    last_error = format!(
                        "failed to parse JSON on attempt {}:\nargs={args:?}\nstdout={stdout}\nstderr={stderr}\nerror={e}",
                        attempt + 1
                    );
                }
            }
        } else {
            last_error = format!(
                "command failed on attempt {}:\nargs={args:?}\nstdout={stdout}\nstderr={stderr}\ncode={code}",
                attempt + 1
            );
        }
        sleep(std::time::Duration::from_millis(500));
    }
    panic!("{last_error}");
}

fn live_test_calendar() -> String {
    std::env::var("CALX_LIVE_TEST_CALENDAR").unwrap_or_else(|_| {
        panic!(
            "Set CALX_LIVE_TEST_CALENDAR to a dedicated writable calendar name before running live EventKit tests"
        )
    })
}

fn live_test_lock() -> std::sync::MutexGuard<'static, ()> {
    LIVE_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn request_eventkit_access(store: &EKEventStore) -> Result<(), String> {
    let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };

    if status == EKAuthorizationStatus::FullAccess {
        return Ok(());
    }

    if status == EKAuthorizationStatus::Denied || status == EKAuthorizationStatus::Restricted {
        return Err("Calendar access was denied.".to_string());
    }

    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        let _ = tx.send(granted.as_bool());
    });
    let block_ptr = &*block as *const block2::Block<_> as *mut block2::Block<_>;
    unsafe { store.requestFullAccessToEventsWithCompletion(block_ptr) };

    match rx.recv_timeout(std::time::Duration::from_secs(30)) {
        Ok(true) => Ok(()),
        Ok(false) => Err("Calendar access was denied.".to_string()),
        Err(_) => Err("Timeout waiting for calendar access.".to_string()),
    }
}

fn open_event_store() -> Retained<EKEventStore> {
    let store = unsafe { EKEventStore::new() };
    request_eventkit_access(&store)
        .unwrap_or_else(|e| panic!("EventKit access is not ready for live tests: {e}"));
    store
}

fn find_calendar_by_title(store: &EKEventStore, title: &str) -> Option<Retained<EKCalendar>> {
    let calendars = unsafe { store.calendarsForEntityType(EKEntityType::Event) };
    let count = calendars.count();
    for i in 0..count {
        let calendar = calendars.objectAtIndex(i);
        if unsafe { calendar.title() }.to_string() == title {
            return Some(calendar.clone());
        }
    }
    None
}

fn writable_source(store: &EKEventStore) -> Result<Retained<EKSource>, String> {
    let calendars = unsafe { store.calendarsForEntityType(EKEntityType::Event) };
    let count = calendars.count();
    for i in 0..count {
        let calendar = calendars.objectAtIndex(i);
        if unsafe { calendar.allowsContentModifications() } && !unsafe { calendar.isImmutable() } {
            if let Some(source) = unsafe { calendar.source() } {
                return Ok(source);
            }
        }
    }
    Err("No writable calendar source was found for creating a live test calendar.".to_string())
}

struct LiveTestCalendar {
    title: String,
    created: bool,
    store: Option<Retained<EKEventStore>>,
    calendar: Option<Retained<EKCalendar>>,
}

impl LiveTestCalendar {
    fn existing(title: String) -> Self {
        Self {
            title,
            created: false,
            store: None,
            calendar: None,
        }
    }

    fn created(
        title: String,
        store: Retained<EKEventStore>,
        calendar: Retained<EKCalendar>,
    ) -> Self {
        Self {
            title,
            created: true,
            store: Some(store),
            calendar: Some(calendar),
        }
    }

    fn title(&self) -> &str {
        &self.title
    }
}

impl Drop for LiveTestCalendar {
    fn drop(&mut self) {
        if !self.created {
            return;
        }
        if let (Some(store), Some(calendar)) = (self.store.take(), self.calendar.take()) {
            if let Err(err) = unsafe { store.removeCalendar_commit_error(&calendar, true) } {
                eprintln!(
                    "failed to delete live test calendar {}: {}",
                    self.title, err
                );
            }
        }
    }
}

fn create_live_test_calendar(title: &str) -> LiveTestCalendar {
    let store = open_event_store();
    if let Some(calendar) = find_calendar_by_title(&store, title) {
        return LiveTestCalendar::existing(unsafe { calendar.title() }.to_string());
    }

    let source = writable_source(&store)
        .unwrap_or_else(|e| panic!("failed to choose writable source for live test calendar: {e}"));
    let calendar =
        unsafe { EKCalendar::calendarForEntityType_eventStore(EKEntityType::Event, &store) };
    let ns_title = NSString::from_str(title);
    unsafe {
        calendar.setTitle(&ns_title);
        calendar.setSource(Some(&source));
    }
    unsafe { store.saveCalendar_commit_error(&calendar, true) }
        .unwrap_or_else(|e| panic!("failed to create live test calendar '{title}': {e}"));
    LiveTestCalendar::created(title.to_string(), store, calendar)
}

fn ensure_live_test_ready(calendar: &str) -> LiveTestCalendar {
    let args = vec![
        "calendars".to_string(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let (stdout, stderr, code) = calx(&args);
    assert_eq!(
        code, 0,
        "Calendar access is not ready. Grant access to target/debug/calx in System Settings > Privacy & Security > Calendars.\nstdout={stdout}\nstderr={stderr}"
    );

    let calendars: Vec<CalendarJson> = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("failed to parse calendars JSON:\nstdout={stdout}\nstderr={stderr}\nerror={e}")
    });
    if calendars.iter().any(|c| c.title == calendar) {
        return LiveTestCalendar::existing(calendar.to_string());
    }

    let created = create_live_test_calendar(calendar);
    let (stdout, stderr, code) = calx(&args);
    assert_eq!(
        code, 0,
        "Calendar creation succeeded but listing calendars failed afterwards.\nstdout={stdout}\nstderr={stderr}"
    );
    let calendars: Vec<CalendarJson> = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("failed to parse calendars JSON after creation:\nstdout={stdout}\nstderr={stderr}\nerror={e}")
    });
    assert!(
        calendars.iter().any(|c| c.title == calendar),
        "Calendar '{calendar}' was created but is still not visible to calx."
    );
    created
}

fn unique_suffix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch");
    format!("{}-{}", std::process::id(), now.as_micros())
}

fn live_test_day() -> NaiveDate {
    Local::now().date_naive() + Duration::days(30)
}

fn fmt_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn fmt_datetime(date: NaiveDate, hour: u32, minute: u32) -> String {
    date.and_hms_opt(hour, minute, 0)
        .unwrap()
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn assert_event_time(event: &EventJson, date: NaiveDate, start_hhmm: &str, end_hhmm: &str) {
    let date_str = fmt_date(date);
    assert!(
        event.start.starts_with(&format!("{date_str}T{start_hhmm}")),
        "unexpected start time: {}",
        event.start
    );
    assert!(
        event.end.starts_with(&format!("{date_str}T{end_hhmm}")),
        "unexpected end time: {}",
        event.end
    );
}

fn assert_all_day_range(event: &EventJson, start_date: NaiveDate, end_exclusive: NaiveDate) {
    let start_str = fmt_date(start_date);
    let end_str = fmt_date(end_exclusive);
    assert!(event.all_day, "event should be marked as all-day");
    assert!(
        event.start.starts_with(&format!("{start_str}T00:00")),
        "unexpected all-day start: {}",
        event.start
    );
    assert!(
        event.end.starts_with(&format!("{end_str}T00:00")),
        "unexpected all-day end: {}",
        event.end
    );
}

struct EventCleanup {
    event_id: Option<String>,
}

impl EventCleanup {
    fn new(event_id: String) -> Self {
        Self {
            event_id: Some(event_id),
        }
    }

    fn disarm(&mut self) {
        self.event_id = None;
    }
}

impl Drop for EventCleanup {
    fn drop(&mut self) {
        if let Some(event_id) = self.event_id.take() {
            let args = vec![
                "delete".to_string(),
                event_id.clone(),
                "-o".to_string(),
                "json".to_string(),
            ];
            let (_, stderr, code) = calx(&args);
            if code != 0 {
                eprintln!("failed to delete live test event {event_id}: {stderr}");
            }
        }
    }
}

#[derive(Deserialize)]
struct CalendarJson {
    title: String,
}

#[derive(Deserialize)]
struct EventJson {
    id: String,
    title: String,
    calendar: String,
    location: Option<String>,
    notes: Option<String>,
    start: String,
    end: String,
    all_day: bool,
    recurring: bool,
    recurrence: Option<String>,
}

#[derive(Deserialize)]
struct UpdateJson {
    updated: bool,
}

#[derive(Deserialize)]
struct DeleteJson {
    deleted: bool,
    event_id: String,
}

#[test]
#[ignore = "requires Calendar permission and CALX_LIVE_TEST_CALENDAR"]
fn test_live_eventkit_round_trip() {
    let _lock = live_test_lock();
    let test_calendar = ensure_live_test_ready(&live_test_calendar());
    let calendar = test_calendar.title().to_string();

    let suffix = unique_suffix();
    let test_day = live_test_day();
    let title = format!("calx-live-{suffix}");
    let updated_title = format!("calx-live-updated-{suffix}");
    let location = format!("desk-{suffix}");
    let updated_location = format!("room-{suffix}");
    let notes = format!("note-{suffix}");
    let updated_notes = format!("updated-note-{suffix}");
    let start = fmt_datetime(test_day, 10, 0);
    let end = fmt_datetime(test_day, 11, 0);
    let updated_start = fmt_datetime(test_day, 10, 30);
    let updated_end = fmt_datetime(test_day, 11, 30);
    let from = fmt_date(test_day);

    let add_args = vec![
        "add".to_string(),
        "--title".to_string(),
        title.clone(),
        "--start".to_string(),
        start,
        "--end".to_string(),
        end,
        "--calendar".to_string(),
        calendar.clone(),
        "--location".to_string(),
        location.clone(),
        "--notes".to_string(),
        notes.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let created: EventJson = parse_success_json(&add_args);
    assert_eq!(created.title, title);
    assert_eq!(created.calendar, calendar);
    assert_eq!(created.location.as_deref(), Some(location.as_str()));
    assert_eq!(created.notes.as_deref(), Some(notes.as_str()));
    assert_event_time(&created, test_day, "10:00", "11:00");

    let mut cleanup = EventCleanup::new(created.id.clone());

    let show_args = vec![
        "show".to_string(),
        created.id.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let shown: EventJson = parse_success_json(&show_args);
    assert_eq!(shown.id, created.id);
    assert_eq!(shown.title, title);

    let update_start_args = vec![
        "update".to_string(),
        "--query".to_string(),
        title.clone(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "--title".to_string(),
        updated_title.clone(),
        "--start".to_string(),
        updated_start,
        "-o".to_string(),
        "json".to_string(),
    ];
    let updated: UpdateJson = parse_success_json(&update_start_args);
    assert!(updated.updated);

    let query_show_args = vec![
        "show".to_string(),
        "--query".to_string(),
        updated_title.clone(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let after_start_update: EventJson = parse_success_json(&query_show_args);
    assert_eq!(after_start_update.title, updated_title);
    assert_event_time(&after_start_update, test_day, "10:30", "11:00");

    let update_end_args = vec![
        "update".to_string(),
        "--query".to_string(),
        updated_title.clone(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "--location".to_string(),
        updated_location.clone(),
        "--notes".to_string(),
        updated_notes.clone(),
        "--end".to_string(),
        updated_end,
        "-o".to_string(),
        "json".to_string(),
    ];
    let updated: UpdateJson = parse_success_json(&update_end_args);
    assert!(updated.updated);

    let after_end_update: EventJson = parse_success_json(&query_show_args);
    assert_eq!(
        after_end_update.location.as_deref(),
        Some(updated_location.as_str())
    );
    assert_eq!(
        after_end_update.notes.as_deref(),
        Some(updated_notes.as_str())
    );
    assert_event_time(&after_end_update, test_day, "10:30", "11:30");

    let search_args = vec![
        "search".to_string(),
        updated_title.clone(),
        "--calendar".to_string(),
        calendar.clone(),
        "--from".to_string(),
        from.clone(),
        "--to".to_string(),
        from.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let search_results: Vec<EventJson> = parse_success_json(&search_args);
    assert!(
        search_results.iter().any(|e| e.id == created.id),
        "updated event was not returned by search"
    );

    let events_args = vec![
        "events".to_string(),
        "--calendar".to_string(),
        calendar.clone(),
        "--from".to_string(),
        from.clone(),
        "--to".to_string(),
        from,
        "-o".to_string(),
        "json".to_string(),
    ];
    let events: Vec<EventJson> = parse_success_json(&events_args);
    assert!(
        events.iter().any(|e| e.id == created.id),
        "updated event was not returned by events"
    );

    let delete_args = vec![
        "delete".to_string(),
        "--query".to_string(),
        updated_title.clone(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let deleted: DeleteJson = parse_success_json(&delete_args);
    assert!(deleted.deleted);
    assert_eq!(deleted.event_id, created.id);
    cleanup.disarm();

    let (stdout, stderr, code) = calx(&show_args);
    assert_eq!(
        code, 3,
        "show after delete should fail with not found:\nstdout={stdout}\nstderr={stderr}"
    );
}

#[test]
#[ignore = "requires Calendar permission and CALX_LIVE_TEST_CALENDAR"]
fn test_live_eventkit_rejects_invalid_one_sided_updates() {
    let _lock = live_test_lock();
    let test_calendar = ensure_live_test_ready(&live_test_calendar());
    let calendar = test_calendar.title().to_string();

    let suffix = unique_suffix();
    let test_day = live_test_day();
    let title = format!("calx-live-invalid-{suffix}");
    let start = fmt_datetime(test_day, 13, 0);
    let end = fmt_datetime(test_day, 14, 0);
    let too_late_start = fmt_datetime(test_day, 15, 0);
    let too_early_end = fmt_datetime(test_day, 12, 0);

    let add_args = vec![
        "add".to_string(),
        "--title".to_string(),
        title.clone(),
        "--start".to_string(),
        start,
        "--end".to_string(),
        end,
        "--calendar".to_string(),
        calendar.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let created: EventJson = parse_success_json(&add_args);
    let _cleanup = EventCleanup::new(created.id.clone());

    let bad_start_args = vec![
        "update".to_string(),
        "--query".to_string(),
        title.clone(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "--start".to_string(),
        too_late_start,
        "-o".to_string(),
        "json".to_string(),
    ];
    let (_, stderr, code) = calx(&bad_start_args);
    assert_eq!(code, 4, "unexpected exit code for invalid start: {stderr}");
    assert!(
        stderr.contains("end time must be after start time"),
        "missing validation message for invalid start: {stderr}"
    );

    let bad_end_args = vec![
        "update".to_string(),
        "--query".to_string(),
        title,
        "--in-calendar".to_string(),
        calendar,
        "--end".to_string(),
        too_early_end,
        "-o".to_string(),
        "json".to_string(),
    ];
    let (_, stderr, code) = calx(&bad_end_args);
    assert_eq!(code, 4, "unexpected exit code for invalid end: {stderr}");
    assert!(
        stderr.contains("end time must be after start time"),
        "missing validation message for invalid end: {stderr}"
    );
}

#[test]
#[ignore = "requires Calendar permission and CALX_LIVE_TEST_CALENDAR"]
fn test_live_eventkit_all_day_round_trip() {
    let _lock = live_test_lock();
    let suffix = unique_suffix();
    let test_calendar = create_live_test_calendar(&format!("{}-{suffix}", live_test_calendar()));
    let calendar = test_calendar.title().to_string();

    let start_day = live_test_day();
    let end_day = start_day + Duration::days(2);
    let updated_start_day = start_day + Duration::days(1);
    let updated_end_day = start_day + Duration::days(3);
    let title = format!("calx-live-all-day-{suffix}");

    let add_args = vec![
        "add".to_string(),
        "--title".to_string(),
        title.clone(),
        "--all-day".to_string(),
        "--start".to_string(),
        fmt_date(start_day),
        "--end".to_string(),
        fmt_date(end_day),
        "--calendar".to_string(),
        calendar.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let created: EventJson = parse_success_json(&add_args);
    assert_all_day_range(&created, start_day, end_day + Duration::days(1));

    let show_args = vec![
        "show".to_string(),
        "--query".to_string(),
        title.clone(),
        "--exact".to_string(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let shown: EventJson = parse_success_json(&show_args);
    assert_eq!(shown.id, created.id);
    assert_all_day_range(&shown, start_day, end_day + Duration::days(1));

    let update_args = vec![
        "update".to_string(),
        "--query".to_string(),
        title.clone(),
        "--exact".to_string(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "--all-day".to_string(),
        "true".to_string(),
        "--start".to_string(),
        fmt_date(updated_start_day),
        "--end".to_string(),
        fmt_date(updated_end_day),
        "-o".to_string(),
        "json".to_string(),
    ];
    let updated: UpdateJson = parse_success_json(&update_args);
    assert!(updated.updated);

    let shown: EventJson = parse_success_json(&show_args);
    assert_all_day_range(
        &shown,
        updated_start_day,
        updated_end_day + Duration::days(1),
    );

    let search_args = vec![
        "search".to_string(),
        title,
        "--exact".to_string(),
        "--calendar".to_string(),
        calendar,
        "--from".to_string(),
        fmt_date(updated_start_day),
        "--to".to_string(),
        fmt_date(updated_end_day),
        "-o".to_string(),
        "json".to_string(),
    ];
    let matches: Vec<EventJson> = parse_success_json(&search_args);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, created.id);
}

#[test]
#[ignore = "requires Calendar permission and CALX_LIVE_TEST_CALENDAR"]
fn test_live_eventkit_recurring_scope_round_trip() {
    let _lock = live_test_lock();
    let suffix = unique_suffix();
    let test_calendar = create_live_test_calendar(&format!("{}-{suffix}", live_test_calendar()));
    let calendar = test_calendar.title().to_string();

    let start_day = live_test_day();
    let day_two = start_day + Duration::days(1);
    let day_three = start_day + Duration::days(2);
    let day_four = start_day + Duration::days(3);
    let title = format!("calx-live-recurring-{suffix}");
    let updated_title = format!("calx-live-recurring-this-{suffix}");

    let add_args = vec![
        "add".to_string(),
        "--title".to_string(),
        title.clone(),
        "--start".to_string(),
        fmt_datetime(start_day, 9, 0),
        "--end".to_string(),
        fmt_datetime(start_day, 9, 30),
        "--calendar".to_string(),
        calendar.clone(),
        "--repeat".to_string(),
        "daily".to_string(),
        "--repeat-count".to_string(),
        "4".to_string(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let created: EventJson = parse_success_json(&add_args);
    assert!(created.recurring);
    assert!(
        created
            .recurrence
            .as_deref()
            .is_some_and(|summary| summary.contains("Every day"))
    );

    let search_original_args = |from: NaiveDate, to: NaiveDate| {
        vec![
            "search".to_string(),
            title.clone(),
            "--exact".to_string(),
            "--calendar".to_string(),
            calendar.clone(),
            "--from".to_string(),
            fmt_date(from),
            "--to".to_string(),
            fmt_date(to),
            "-o".to_string(),
            "json".to_string(),
        ]
    };

    let matches: Vec<EventJson> = wait_for_success_json(
        || search_original_args(start_day, day_four),
        |matches: &Vec<EventJson>| matches.len() == 4,
    );
    assert_eq!(matches.len(), 4);

    let show_day_two_args = vec![
        "show".to_string(),
        "--query".to_string(),
        title.clone(),
        "--exact".to_string(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "--from".to_string(),
        fmt_date(day_two),
        "--to".to_string(),
        fmt_date(day_two),
        "-o".to_string(),
        "json".to_string(),
    ];
    let second_occurrence: EventJson = parse_success_json(&show_day_two_args);
    assert!(second_occurrence.recurring);

    let update_args = vec![
        "update".to_string(),
        "--query".to_string(),
        title.clone(),
        "--exact".to_string(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "--from".to_string(),
        fmt_date(day_two),
        "--to".to_string(),
        fmt_date(day_two),
        "--scope".to_string(),
        "this".to_string(),
        "--title".to_string(),
        updated_title.clone(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let updated: UpdateJson = parse_success_json(&update_args);
    assert!(updated.updated);

    let remaining_original: Vec<EventJson> = wait_for_success_json(
        || search_original_args(start_day, day_four),
        |matches: &Vec<EventJson>| matches.len() == 3,
    );
    assert_eq!(remaining_original.len(), 3);
    assert!(remaining_original.iter().all(|event| event.title == title));

    let updated_matches_args = vec![
        "search".to_string(),
        updated_title.clone(),
        "--exact".to_string(),
        "--calendar".to_string(),
        calendar.clone(),
        "--from".to_string(),
        fmt_date(day_two),
        "--to".to_string(),
        fmt_date(day_two),
        "-o".to_string(),
        "json".to_string(),
    ];
    let updated_matches: Vec<EventJson> = wait_for_success_json(
        || updated_matches_args.clone(),
        |matches: &Vec<EventJson>| matches.len() == 1,
    );
    assert_eq!(updated_matches.len(), 1);

    let delete_args = vec![
        "delete".to_string(),
        "--query".to_string(),
        title.clone(),
        "--exact".to_string(),
        "--in-calendar".to_string(),
        calendar.clone(),
        "--from".to_string(),
        fmt_date(day_three),
        "--to".to_string(),
        fmt_date(day_three),
        "--scope".to_string(),
        "future".to_string(),
        "-o".to_string(),
        "json".to_string(),
    ];
    let deleted: DeleteJson = parse_success_json(&delete_args);
    assert!(deleted.deleted);

    let remaining_original: Vec<EventJson> = wait_for_success_json(
        || search_original_args(start_day, day_four),
        |matches: &Vec<EventJson>| matches.len() == 1,
    );
    assert_eq!(remaining_original.len(), 1);
    assert!(remaining_original.iter().all(|event| event.title == title));

    let updated_matches: Vec<EventJson> = wait_for_success_json(
        || updated_matches_args.clone(),
        |matches: &Vec<EventJson>| matches.len() == 1,
    );
    assert_eq!(updated_matches.len(), 1);
}
