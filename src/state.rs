use crate::error::AppError;
use crate::store::{EventDraft, RecurrenceScope};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEMPLATES_FILE: &str = "templates.json";
const UNDO_FILE: &str = "undo.json";
const PENDING_UNDO_FILE: &str = "undo.pending.json";
const STATE_LOCK_FILE: &str = ".state.lock";
const MAX_UNDO_STACK: usize = 20;
const LOCK_RETRY_ATTEMPTS: usize = 100;
const LOCK_RETRY_DELAY_MS: u64 = 20;
const LOCK_STALE_AFTER_SECS: u64 = 30;
const PENDING_UNDO_STALE_AFTER_SECS: u64 = 300;
const PENDING_UNDO_BLOCK_MESSAGE: &str =
    "Undo is already in progress. Finish it with `calx undo` before making another change.";

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredTemplate {
    pub name: String,
    pub draft: EventDraft,
    pub saved_at: DateTime<Local>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoRecord {
    pub action: UndoAction,
    pub recorded_at: DateTime<Local>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingUndoClaim {
    record: UndoRecord,
    pid: u32,
    claimed_at_unix_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UndoAction {
    DeleteCreated {
        event_id: String,
        selected_start: DateTime<Local>,
        scope: Option<RecurrenceScope>,
    },
    RestoreDeleted {
        draft: EventDraft,
    },
    ReplaceWithDraft {
        current_event_id: String,
        current_start: DateTime<Local>,
        current_scope: Option<RecurrenceScope>,
        draft: EventDraft,
    },
    Unavailable {
        reason: String,
    },
}

pub fn list_templates() -> Result<Vec<StoredTemplate>, AppError> {
    let mut templates = read_templates()?;
    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(templates)
}

pub fn get_template(name: &str) -> Result<StoredTemplate, AppError> {
    list_templates()?
        .into_iter()
        .find(|template| template.name == name)
        .ok_or_else(|| AppError::TemplateNotFound(name.to_string()))
}

pub fn save_template(name: &str, draft: EventDraft, overwrite: bool) -> Result<(), AppError> {
    with_state_lock(|| {
        let mut templates = read_templates()?;
        let stored = StoredTemplate {
            name: name.to_string(),
            draft,
            saved_at: Local::now(),
        };

        if let Some(existing) = templates.iter_mut().find(|template| template.name == name) {
            if !overwrite {
                return Err(AppError::InvalidArgument(format!(
                    "Template '{name}' already exists. Use --force to overwrite."
                )));
            }
            *existing = stored;
        } else {
            templates.push(stored);
        }

        write_templates(&templates)
    })
}

pub fn delete_template(name: &str) -> Result<(), AppError> {
    with_state_lock(|| {
        let mut templates = read_templates()?;
        let original_len = templates.len();
        templates.retain(|template| template.name != name);
        if templates.len() == original_len {
            return Err(AppError::TemplateNotFound(name.to_string()));
        }
        write_templates(&templates)
    })
}

#[cfg(test)]
pub fn load_undo() -> Result<Option<UndoRecord>, AppError> {
    Ok(read_undo_stack()?.into_iter().last())
}

pub fn save_undo(action: UndoAction) -> Result<(), AppError> {
    with_state_lock(|| {
        reject_pending_undo()?;
        let mut stack = read_undo_stack()?;
        stack.push(UndoRecord {
            action,
            recorded_at: Local::now(),
        });
        trim_undo_stack(&mut stack);
        write_undo_stack(&stack)
    })
}

pub fn take_undo() -> Result<Option<UndoRecord>, AppError> {
    with_state_lock(|| {
        if let Some(claim) = read_pending_undo()? {
            if pending_claim_is_recoverable(&claim) {
                let record = claim.record;
                write_pending_undo(&PendingUndoClaim::new(record.clone()))?;
                return Ok(Some(record));
            }
            return Err(AppError::InvalidArgument(
                "Undo is already in progress. Try again in a moment.".to_string(),
            ));
        }

        let stack = read_undo_stack()?;
        let mut remaining = stack.clone();
        let Some(record) = remaining.pop() else {
            return Ok(None);
        };

        write_remaining_undo_stack(&remaining)?;
        if let Err(err) = write_pending_undo(&PendingUndoClaim::new(record.clone())) {
            if let Err(restore_err) = write_undo_stack(&stack) {
                return Err(AppError::Io(format!(
                    "failed to claim undo record: {err}; rollback failed: {restore_err}"
                )));
            }
            return Err(err);
        }

        Ok(Some(record))
    })
}

pub fn ensure_no_pending_undo() -> Result<(), AppError> {
    reject_pending_undo()
}

pub fn restore_undo_record(record: UndoRecord) -> Result<(), AppError> {
    with_state_lock(|| {
        clear_pending_undo_for(&record)?;
        let mut stack = read_undo_stack()?;
        stack.push(record);
        trim_undo_stack(&mut stack);
        write_undo_stack(&stack)
    })
}

pub fn finalize_undo_record(record: &UndoRecord) -> Result<(), AppError> {
    with_state_lock(|| clear_pending_undo_for(record))
}

#[cfg(test)]
pub fn clear_undo() -> Result<(), AppError> {
    with_state_lock(|| {
        let mut stack = read_undo_stack()?;
        if stack.pop().is_none() {
            return Ok(());
        }
        write_remaining_undo_stack(&stack)
    })
}

#[cfg(test)]
pub fn clear_undo_record(expected: &UndoRecord) -> Result<bool, AppError> {
    with_state_lock(|| {
        let mut stack = read_undo_stack()?;
        let Some(index) = stack.iter().rposition(|record| record == expected) else {
            return Ok(false);
        };

        stack.remove(index);
        if stack.is_empty() {
            match fs::remove_file(undo_path()) {
                Ok(()) => Ok(true),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(true),
                Err(err) => Err(AppError::Io(err.to_string())),
            }
        } else {
            write_undo_stack(&stack)?;
            Ok(true)
        }
    })
}

fn read_templates() -> Result<Vec<StoredTemplate>, AppError> {
    Ok(read_optional_json(&templates_path())?.unwrap_or_default())
}

fn reject_pending_undo() -> Result<(), AppError> {
    if read_pending_undo()?.is_some() {
        return Err(AppError::InvalidArgument(
            PENDING_UNDO_BLOCK_MESSAGE.to_string(),
        ));
    }
    Ok(())
}

fn write_templates(templates: &[StoredTemplate]) -> Result<(), AppError> {
    write_json(&templates_path(), templates)
}

fn read_undo_stack() -> Result<Vec<UndoRecord>, AppError> {
    let Some(bytes) = read_optional_bytes(&undo_path())? else {
        return Ok(Vec::new());
    };

    match serde_json::from_slice::<Vec<UndoRecord>>(&bytes) {
        Ok(stack) => Ok(stack),
        Err(stack_err) => match serde_json::from_slice::<UndoRecord>(&bytes) {
            Ok(record) => Ok(vec![record]),
            Err(record_err) => Err(AppError::Io(format!(
                "failed to parse undo history: {stack_err}; {record_err}"
            ))),
        },
    }
}

fn write_undo_stack(stack: &[UndoRecord]) -> Result<(), AppError> {
    write_json(&undo_path(), stack)
}

fn read_pending_undo() -> Result<Option<PendingUndoClaim>, AppError> {
    read_optional_json(&pending_undo_path())
}

fn write_pending_undo(claim: &PendingUndoClaim) -> Result<(), AppError> {
    write_json(&pending_undo_path(), claim)
}

fn pending_undo_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(PENDING_UNDO_FILE)
}

fn write_remaining_undo_stack(stack: &[UndoRecord]) -> Result<(), AppError> {
    if stack.is_empty() {
        match fs::remove_file(undo_path()) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(AppError::Io(err.to_string())),
        }
    } else {
        write_undo_stack(stack)
    }
}

fn clear_pending_undo_for(expected: &UndoRecord) -> Result<(), AppError> {
    match read_pending_undo()? {
        Some(claim) if claim.record == *expected => remove_pending_undo_file(),
        Some(_) => Err(AppError::Io(
            "pending undo record did not match the expected action".to_string(),
        )),
        None => Ok(()),
    }
}

fn remove_pending_undo_file() -> Result<(), AppError> {
    match fs::remove_file(pending_undo_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::Io(err.to_string())),
    }
}

fn trim_undo_stack(stack: &mut Vec<UndoRecord>) {
    if stack.len() > MAX_UNDO_STACK {
        let drop_count = stack.len() - MAX_UNDO_STACK;
        stack.drain(0..drop_count);
    }
}

impl PendingUndoClaim {
    fn new(record: UndoRecord) -> Self {
        Self {
            record,
            pid: std::process::id(),
            claimed_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default(),
        }
    }
}

fn pending_claim_is_recoverable(claim: &PendingUndoClaim) -> bool {
    claim_is_stale(claim) || !process_exists(claim.pid)
}

fn claim_is_stale(claim: &PendingUndoClaim) -> bool {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    now_ms.saturating_sub(claim.claimed_at_unix_ms)
        > Duration::from_secs(PENDING_UNDO_STALE_AFTER_SECS).as_millis()
}

fn process_exists(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    if result == 0 {
        return true;
    }

    match std::io::Error::last_os_error().raw_os_error() {
        Some(code) if code == libc::EPERM => true,
        Some(code) if code == libc::ESRCH => false,
        _ => false,
    }
}

fn config_dir() -> Result<PathBuf, AppError> {
    if let Some(explicit) = std::env::var_os("CALX_CONFIG_DIR") {
        return Ok(PathBuf::from(explicit));
    }

    let home = std::env::var_os("HOME").ok_or_else(|| {
        AppError::Io("HOME is not set and CALX_CONFIG_DIR is not configured".to_string())
    })?;
    Ok(PathBuf::from(home).join(".config").join("calx"))
}

fn ensure_config_dir() -> Result<PathBuf, AppError> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir).map_err(|err| AppError::Io(err.to_string()))?;
    Ok(dir)
}

fn templates_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(TEMPLATES_FILE)
}

fn undo_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(UNDO_FILE)
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), AppError> {
    let dir = ensure_config_dir()?;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        dir.join(path)
    };
    let bytes = serde_json::to_vec_pretty(value).map_err(|err| AppError::Io(err.to_string()))?;
    write_atomic(&path, &bytes)
}

fn read_optional_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, AppError> {
    let Some(bytes) = read_optional_bytes(path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| AppError::Io(err.to_string()))
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, AppError> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_dir()?.join(path)
    };
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AppError::Io(err.to_string())),
    }
}

fn with_state_lock<T>(f: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    let _guard = StateLockGuard::acquire()?;
    f()
}

struct StateLockGuard {
    path: PathBuf,
    _file: File,
}

#[derive(Serialize, Deserialize)]
struct StateLockInfo {
    pid: u32,
    created_at_unix_ms: u128,
}

impl StateLockGuard {
    fn acquire() -> Result<Self, AppError> {
        let dir = ensure_config_dir()?;
        let path = dir.join(STATE_LOCK_FILE);

        for attempt in 0..LOCK_RETRY_ATTEMPTS {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    write_lock_info(&mut file)?;
                    return Ok(Self { path, _file: file });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    if lock_is_stale(&path) {
                        match fs::remove_file(&path) {
                            Ok(()) => continue,
                            Err(remove_err)
                                if remove_err.kind() == std::io::ErrorKind::NotFound =>
                            {
                                continue;
                            }
                            Err(remove_err) => {
                                return Err(AppError::Io(remove_err.to_string()));
                            }
                        }
                    }
                    if attempt + 1 == LOCK_RETRY_ATTEMPTS {
                        return Err(AppError::Io(format!(
                            "timed out waiting for state lock: {}",
                            path.display()
                        )));
                    }
                    sleep(Duration::from_millis(LOCK_RETRY_DELAY_MS));
                }
                Err(err) => return Err(AppError::Io(err.to_string())),
            }
        }

        Err(AppError::Io("failed to acquire state lock".to_string()))
    }
}

impl Drop for StateLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or_default();
    let tmp_path = path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        unique
    ));

    let write_result = (|| -> Result<(), AppError> {
        let mut file = File::create(&tmp_path).map_err(|err| AppError::Io(err.to_string()))?;
        file.write_all(bytes)
            .map_err(|err| AppError::Io(err.to_string()))?;
        file.sync_all()
            .map_err(|err| AppError::Io(err.to_string()))?;
        fs::rename(&tmp_path, path).map_err(|err| AppError::Io(err.to_string()))?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }

    write_result
}

fn write_lock_info(file: &mut File) -> Result<(), AppError> {
    let info = StateLockInfo {
        pid: std::process::id(),
        created_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default(),
    };
    let bytes = serde_json::to_vec(&info).map_err(|err| AppError::Io(err.to_string()))?;
    file.write_all(&bytes)
        .map_err(|err| AppError::Io(err.to_string()))?;
    file.sync_all().map_err(|err| AppError::Io(err.to_string()))
}

fn lock_is_stale(path: &Path) -> bool {
    let max_age = Duration::from_secs(LOCK_STALE_AFTER_SECS);

    if let Ok(bytes) = fs::read(path) {
        if let Ok(info) = serde_json::from_slice::<StateLockInfo>(&bytes) {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default();
            return now_ms.saturating_sub(info.created_at_unix_ms) > max_age.as_millis();
        }
    }

    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed > max_age)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn with_temp_config_dir<T>(f: impl FnOnce(&Path) -> T) -> T {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let dir = std::env::temp_dir().join(format!(
            "calx-state-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros()
        ));
        fs::create_dir_all(&dir).unwrap();

        let previous = std::env::var_os("CALX_CONFIG_DIR");
        unsafe { std::env::set_var("CALX_CONFIG_DIR", &dir) };

        let result = f(&dir);

        match previous {
            Some(value) => unsafe { std::env::set_var("CALX_CONFIG_DIR", value) },
            None => unsafe { std::env::remove_var("CALX_CONFIG_DIR") },
        }
        let _ = fs::remove_dir_all(&dir);

        result
    }

    fn unavailable_record(reason: &str) -> UndoRecord {
        UndoRecord {
            action: UndoAction::Unavailable {
                reason: reason.to_string(),
            },
            recorded_at: Local.with_ymd_and_hms(2026, 3, 22, 12, 0, 0).unwrap(),
        }
    }

    fn sample_draft(title: &str) -> EventDraft {
        EventDraft {
            title: title.to_string(),
            start: Local.with_ymd_and_hms(2026, 3, 22, 9, 0, 0).unwrap(),
            end: Local.with_ymd_and_hms(2026, 3, 22, 10, 0, 0).unwrap(),
            calendar: "Work".to_string(),
            calendar_id: "cal-1".to_string(),
            location: None,
            url: None,
            notes: None,
            all_day: false,
            alerts: vec![10],
            recurrence_rule: None,
        }
    }

    #[test]
    fn test_save_undo_preserves_stack_and_loads_latest() {
        with_temp_config_dir(|dir| {
            save_undo(unavailable_record("first").action).unwrap();
            save_undo(unavailable_record("second").action).unwrap();

            let latest = load_undo().unwrap().unwrap();
            match latest.action {
                UndoAction::Unavailable { reason } => assert_eq!(reason, "second"),
                _ => panic!("unexpected undo action"),
            }

            let bytes = fs::read(dir.join(UNDO_FILE)).unwrap();
            let stack: Vec<UndoRecord> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(stack.len(), 2);
        });
    }

    #[test]
    fn test_load_undo_reads_legacy_single_record_file() {
        with_temp_config_dir(|dir| {
            let record = unavailable_record("legacy");
            fs::write(
                dir.join(UNDO_FILE),
                serde_json::to_vec_pretty(&record).unwrap(),
            )
            .unwrap();

            let latest = load_undo().unwrap().unwrap();
            match latest.action {
                UndoAction::Unavailable { reason } => assert_eq!(reason, "legacy"),
                _ => panic!("unexpected undo action"),
            }
        });
    }

    #[test]
    fn test_clear_undo_pops_latest_record() {
        with_temp_config_dir(|_| {
            save_undo(unavailable_record("first").action).unwrap();
            save_undo(unavailable_record("second").action).unwrap();

            clear_undo().unwrap();
            let latest = load_undo().unwrap().unwrap();
            match latest.action {
                UndoAction::Unavailable { reason } => assert_eq!(reason, "first"),
                _ => panic!("unexpected undo action"),
            }

            clear_undo().unwrap();
            assert!(load_undo().unwrap().is_none());
        });
    }

    #[test]
    fn test_clear_undo_record_removes_matching_entry() {
        with_temp_config_dir(|_| {
            let first = unavailable_record("first");
            let second = unavailable_record("second");
            write_undo_stack(&[first.clone(), second.clone()]).unwrap();

            assert!(clear_undo_record(&first).unwrap());
            let latest = load_undo().unwrap().unwrap();
            assert_eq!(latest, second);
        });
    }

    #[test]
    fn test_take_undo_claims_latest_record() {
        with_temp_config_dir(|dir| {
            save_undo(unavailable_record("first").action).unwrap();
            save_undo(unavailable_record("second").action).unwrap();

            let claimed = take_undo().unwrap().unwrap();
            match &claimed.action {
                UndoAction::Unavailable { reason } => assert_eq!(reason, "second"),
                _ => panic!("unexpected undo action"),
            }

            let latest = load_undo().unwrap().unwrap();
            match latest.action {
                UndoAction::Unavailable { reason } => assert_eq!(reason, "first"),
                _ => panic!("unexpected undo action"),
            }

            let pending: PendingUndoClaim =
                serde_json::from_slice(&fs::read(dir.join(PENDING_UNDO_FILE)).unwrap()).unwrap();
            assert_eq!(pending.record, claimed);
        });
    }

    #[test]
    fn test_restore_undo_record_pushes_record_back() {
        with_temp_config_dir(|dir| {
            save_undo(unavailable_record("first").action).unwrap();
            let record = take_undo().unwrap().unwrap();
            restore_undo_record(record.clone()).unwrap();

            let latest = load_undo().unwrap().unwrap();
            assert_eq!(latest, record);
            assert!(!dir.join(PENDING_UNDO_FILE).exists());
        });
    }

    #[test]
    fn test_finalize_undo_record_clears_pending_claim() {
        with_temp_config_dir(|dir| {
            save_undo(unavailable_record("first").action).unwrap();
            let record = take_undo().unwrap().unwrap();

            finalize_undo_record(&record).unwrap();

            assert!(load_undo().unwrap().is_none());
            assert!(!dir.join(PENDING_UNDO_FILE).exists());
        });
    }

    #[test]
    fn test_take_undo_reclaims_stale_pending_claim() {
        with_temp_config_dir(|dir| {
            let record = unavailable_record("stale");
            let claim = PendingUndoClaim {
                record: record.clone(),
                pid: 999_999,
                claimed_at_unix_ms: 0,
            };
            fs::write(
                dir.join(PENDING_UNDO_FILE),
                serde_json::to_vec_pretty(&claim).unwrap(),
            )
            .unwrap();

            let claimed = take_undo().unwrap().unwrap();
            assert_eq!(claimed, record);
        });
    }

    #[test]
    fn test_take_undo_rejects_active_pending_claim() {
        with_temp_config_dir(|dir| {
            let claim = PendingUndoClaim {
                record: unavailable_record("active"),
                pid: std::process::id(),
                claimed_at_unix_ms: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
            };
            fs::write(
                dir.join(PENDING_UNDO_FILE),
                serde_json::to_vec_pretty(&claim).unwrap(),
            )
            .unwrap();

            let err = take_undo().unwrap_err();
            assert!(err.to_string().contains("already in progress"));
        });
    }

    #[test]
    fn test_save_undo_rejects_pending_claim() {
        with_temp_config_dir(|dir| {
            let claim = PendingUndoClaim {
                record: unavailable_record("active"),
                pid: std::process::id(),
                claimed_at_unix_ms: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
            };
            fs::write(
                dir.join(PENDING_UNDO_FILE),
                serde_json::to_vec_pretty(&claim).unwrap(),
            )
            .unwrap();

            let err = save_undo(unavailable_record("new").action).unwrap_err();
            assert!(err.to_string().contains("Undo is already in progress"));
        });
    }

    #[test]
    fn test_save_undo_recovers_from_stale_lock() {
        with_temp_config_dir(|dir| {
            let stale = StateLockInfo {
                pid: 999_999,
                created_at_unix_ms: 0,
            };
            fs::write(
                dir.join(STATE_LOCK_FILE),
                serde_json::to_vec(&stale).unwrap(),
            )
            .unwrap();

            save_undo(unavailable_record("recovered").action).unwrap();
            let latest = load_undo().unwrap().unwrap();
            match latest.action {
                UndoAction::Unavailable { reason } => assert_eq!(reason, "recovered"),
                _ => panic!("unexpected undo action"),
            }
        });
    }

    #[test]
    fn test_save_template_rejects_overwrite_without_force() {
        with_temp_config_dir(|_| {
            save_template("focus", sample_draft("Focus"), false).unwrap();
            let err = save_template("focus", sample_draft("Focus 2"), false).unwrap_err();
            assert!(err.to_string().contains("--force"));
        });
    }

    #[test]
    fn test_save_template_overwrites_with_force() {
        with_temp_config_dir(|_| {
            save_template("focus", sample_draft("Focus"), false).unwrap();
            save_template("focus", sample_draft("Focus 2"), true).unwrap();

            let stored = get_template("focus").unwrap();
            assert_eq!(stored.draft.title, "Focus 2");
        });
    }
}
