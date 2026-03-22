use chrono::{Local, TimeZone};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn calx(args: &[&str]) -> (String, String, i32) {
    calx_env(args, &[])
}

fn calx_env(args: &[&str], envs: &[(&str, &str)]) -> (String, String, i32) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_calx"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().expect("failed to execute calx");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

fn temp_config_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("calx-integration-{suffix}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn seed_template(config_dir: &Path, name: &str) {
    let saved_at = Local.with_ymd_and_hms(2026, 3, 19, 12, 0, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 3, 20, 9, 0, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 3, 20, 10, 0, 0).unwrap();
    let payload = serde_json::json!([{
        "name": name,
        "draft": {
            "title": "Focus Block",
            "start": start,
            "end": end,
            "calendar": "Work",
            "calendar_id": "cal-1",
            "location": null,
            "url": "https://example.com/focus",
            "notes": "Deep work",
            "all_day": false,
            "alerts": [10, 30],
            "recurrence_rule": {
                "frequency": "weekly",
                "interval": 1,
                "count": null,
                "until": null
            }
        },
        "saved_at": saved_at
    }]);
    fs::write(
        config_dir.join("templates.json"),
        serde_json::to_vec_pretty(&payload).unwrap(),
    )
    .unwrap();
}

fn seed_all_day_template(config_dir: &Path, name: &str) {
    let saved_at = Local.with_ymd_and_hms(2026, 3, 19, 12, 0, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 3, 20, 0, 0, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 3, 23, 0, 0, 0).unwrap();
    let payload = serde_json::json!([{
        "name": name,
        "draft": {
            "title": "Trip",
            "start": start,
            "end": end,
            "calendar": "Personal",
            "calendar_id": "cal-2",
            "location": null,
            "url": null,
            "notes": null,
            "all_day": true,
            "alerts": [],
            "recurrence_rule": null
        },
        "saved_at": saved_at
    }]);
    fs::write(
        config_dir.join("templates.json"),
        serde_json::to_vec_pretty(&payload).unwrap(),
    )
    .unwrap();
}

fn seed_unavailable_undo(config_dir: &Path) {
    let recorded_at = Local.with_ymd_and_hms(2026, 3, 19, 12, 0, 0).unwrap();
    let payload = serde_json::json!({
        "action": { "Unavailable": { "reason": "test fixture" } },
        "recorded_at": recorded_at
    });
    fs::write(
        config_dir.join("undo.json"),
        serde_json::to_vec_pretty(&payload).unwrap(),
    )
    .unwrap();
}

fn seed_pending_undo(config_dir: &Path) {
    let recorded_at = Local.with_ymd_and_hms(2026, 3, 19, 12, 0, 0).unwrap();
    let payload = serde_json::json!({
        "record": {
            "action": { "Unavailable": { "reason": "test fixture" } },
            "recorded_at": recorded_at
        },
        "pid": std::process::id(),
        "claimed_at_unix_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    });
    fs::write(
        config_dir.join("undo.pending.json"),
        serde_json::to_vec_pretty(&payload).unwrap(),
    )
    .unwrap();
}

// -----------------------------------------------------------
// CLI argument parsing
// -----------------------------------------------------------

#[test]
fn test_help() {
    let (stdout, _, code) = calx(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Native macOS Calendar CLI"));
    assert!(stdout.contains("Commands:"));
}

#[test]
fn test_version() {
    let (stdout, _, code) = calx(&["--version"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("calx "));
}

#[test]
fn test_invalid_subcommand() {
    let (_, stderr, code) = calx(&["nonexistent"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("error"));
}

#[test]
fn test_output_formats_accepted() {
    for fmt in &["auto", "human", "json", "yaml", "csv", "tsv", "table"] {
        let (_, stderr, code) = calx(&["today", "-o", fmt]);
        // Should succeed (code 0) or fail for calendar access (code 1 with error msg)
        // but NOT fail for unknown format
        if code != 0 {
            assert!(
                stderr.contains("Calendar access") || stderr.contains("error"),
                "Unexpected error for format {fmt}: {stderr}"
            );
        }
    }
}

#[test]
fn test_invalid_output_format() {
    let (_, stderr, code) = calx(&["today", "-o", "xml"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("invalid value"));
}

#[test]
fn test_ics_output_format_rejected() {
    let (_, stderr, code) = calx(&["today", "-o", "ics"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("invalid value"));
}

#[test]
fn test_completions_bash() {
    let (stdout, _, code) = calx(&["completions", "bash"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("calx"));
}

#[test]
fn test_completions_zsh() {
    let (stdout, _, code) = calx(&["completions", "zsh"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("calx"));
}

#[test]
fn test_completions_fish() {
    let (stdout, _, code) = calx(&["completions", "fish"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("calx"));
}

// -----------------------------------------------------------
// Add command argument validation
// -----------------------------------------------------------

#[test]
fn test_add_missing_required_args() {
    let (_, stderr, code) = calx(&["add"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("--title") || stderr.contains("required"));
}

#[test]
fn test_add_missing_end() {
    let (_, stderr, code) = calx(&["add", "--title", "Test", "--start", "2026-03-20 10:00"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("--end") || stderr.contains("required"));
}

#[test]
fn test_add_blank_title_rejected() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--title must not be empty"));
}

// -----------------------------------------------------------
// Add: start > end validation
// -----------------------------------------------------------

#[test]
fn test_add_end_before_start() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 15:00",
        "--end",
        "2026-03-20 14:00",
    ]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("end time must be after start") || stderr.contains("error"),
        "Should reject end < start: {stderr}"
    );
}

#[test]
fn test_add_rejects_zero_duration() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 15:00",
        "--end",
        "2026-03-20 15:00",
    ]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("end time must be after start"),
        "Should reject zero-duration timed event: {stderr}"
    );
}

#[test]
fn test_add_invalid_repeat_value() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--repeat",
        "foo",
    ]);
    assert_eq!(code, 4);
    assert!(
        stderr.contains("Unknown repeat frequency"),
        "Should reject invalid repeat value before calendar access: {stderr}"
    );
    assert!(
        !stderr.contains("EventKit"),
        "Invalid repeat should be reported as argument error: {stderr}"
    );
}

#[test]
fn test_add_repeat_count_requires_repeat() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--repeat-count",
        "5",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--repeat-count requires --repeat"));
}

#[test]
fn test_add_repeat_count_zero_rejected() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--repeat",
        "weekly",
        "--repeat-count",
        "0",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--repeat-count must be greater than 0"));
}

#[test]
fn test_add_repeat_interval_requires_repeat() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--repeat-interval",
        "2",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--repeat-interval requires --repeat"));
}

#[test]
fn test_add_repeat_interval_zero_rejected() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--repeat",
        "weekly",
        "--repeat-interval",
        "0",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--repeat-interval must be greater than 0"));
}

#[test]
fn test_add_negative_alert_rejected() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--alert=-10",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--alert expects minutes before the event"));
}

#[test]
fn test_add_invalid_url_rejected() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--url",
        "http://[::1",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("Invalid URL"));
}

#[test]
fn test_add_all_day_rejects_time_input() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "All Day",
        "--all-day",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20",
    ]);
    assert_eq!(code, 4);
    assert!(
        stderr.contains("Invalid date"),
        "all-day add should reject time input: {stderr}"
    );
}

#[test]
fn test_add_rejected_when_undo_is_pending() {
    let dir = temp_config_dir();
    seed_pending_undo(&dir);
    let dir_str = dir.to_string_lossy().into_owned();

    let (_, stderr, code) = calx_env(
        &[
            "add",
            "--title",
            "Blocked",
            "--start",
            "2026-03-20 10:00",
            "--end",
            "2026-03-20 11:00",
        ],
        &[("CALX_CONFIG_DIR", dir_str.as_str())],
    );

    assert_eq!(code, 4);
    assert!(stderr.contains("Undo is already in progress"));

    let _ = fs::remove_dir_all(dir);
}

// -----------------------------------------------------------
// Verbose and fields flags
// -----------------------------------------------------------

#[test]
fn test_verbose_flag_accepted() {
    let (_, stderr, _) = calx(&["today", "-v"]);
    // Should not fail due to flag parsing
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_fields_flag_accepted() {
    let (_, stderr, code) = calx(&["today", "-o", "json", "--fields", "title,start"]);
    assert!(
        code == 0 || code == 2,
        "structured --fields should parse and only fail on runtime access: {stderr}"
    );
    assert!(!stderr.contains("unexpected argument"));
    assert!(!stderr.contains("--fields requires"));
}

#[test]
fn test_search_calendar_flag_accepted() {
    let (_, stderr, _) = calx(&["search", "test", "--calendar", "Work"]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_search_exact_flag_accepted() {
    let (_, stderr, _) = calx(&["search", "test", "--exact"]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_fields_requires_structured_output() {
    let (_, stderr, code) = calx(&["today", "-o", "human", "--fields", "title,start"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--fields requires a structured output format"));
}

#[test]
fn test_fields_allowed_with_auto_output_when_piped() {
    let (_, stderr, code) = calx(&["today", "--fields", "title,start"]);
    assert!(
        code == 0 || code == 2,
        "auto output should resolve to structured output in non-TTY contexts: {stderr}"
    );
    assert!(!stderr.contains("--fields requires a structured output format"));
}

#[test]
fn test_fields_rejected_for_unsupported_command() {
    let (_, stderr, code) = calx(&["calendars", "-o", "json", "--fields", "title"]);
    assert_eq!(code, 4);
    assert!(stderr.contains(
        "--fields is only supported for events, today, upcoming, search, next, and conflicts"
    ));
}

#[test]
fn test_fields_rejects_empty_field_name() {
    let (_, stderr, code) = calx(&["today", "-o", "json", "--fields", "title,,start"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--fields must be a comma-separated list of field names"));
}

#[test]
fn test_fields_rejects_unknown_field_name() {
    let (_, stderr, code) = calx(&["today", "-o", "json", "--fields", "title,titel"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("Unknown field(s): titel"));
}

#[test]
fn test_doctor_rejects_fields_flag() {
    let (_, stderr, code) = calx(&["doctor", "-o", "json", "--fields", "title"]);
    assert_eq!(code, 4);
    assert!(stderr.contains(
        "--fields is only supported for events, today, upcoming, search, next, and conflicts"
    ));
}

#[test]
fn test_conflicts_help() {
    let (stdout, _, code) = calx(&["conflicts", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Show events that conflict"));
}

#[test]
fn test_conflicts_rejects_reversed_range() {
    let (_, stderr, code) = calx(&[
        "conflicts",
        "--start",
        "2026-03-20 15:00",
        "--end",
        "2026-03-20 14:00",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("end time must be after start time"));
}

#[test]
fn test_duplicate_blank_event_id_rejected() {
    let (_, stderr, code) = calx(&["duplicate", ""]);
    assert_eq!(code, 4);
    assert!(stderr.contains("EVENT_ID must not be empty"));
}

#[test]
fn test_duplicate_query_flags_accepted() {
    let (_, stderr, _) = calx(&[
        "duplicate",
        "--query",
        "test",
        "--exact",
        "--in-calendar",
        "Work",
        "--start",
        "2026-03-20 10:00",
    ]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_duplicate_invalid_start_is_local_error() {
    let (_, stderr, code) = calx(&["duplicate", "fake-id", "--start", "notadate"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("Invalid date"));
}

#[test]
fn test_update_negative_alert_rejected() {
    let (_, stderr, code) = calx(&["update", "fake-id", "--alert=-10"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--alert expects minutes before the event"));
}

#[test]
fn test_update_clear_alerts_counts_as_change() {
    let (_, stderr, code) = calx(&["update", "fake-id", "--clear-alerts"]);
    assert_ne!(
        code, 4,
        "clear-alerts should count as an update change: {stderr}"
    );
}

#[test]
fn test_template_list_local_empty_state() {
    let config_dir = temp_config_dir();
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (stdout, _, code) = calx_env(
        &["template", "list"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "[]");
}

#[test]
fn test_template_show_missing_is_local_error() {
    let config_dir = temp_config_dir();
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(
        &["template", "show", "missing"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 3);
    assert!(stderr.contains("\"error\": \"Template not found\""));
    assert!(stderr.contains("No template named 'missing' exists."));
}

#[test]
fn test_template_delete_missing_is_local_error() {
    let config_dir = temp_config_dir();
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(
        &["template", "delete", "missing"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 3);
    assert!(stderr.contains("\"error\": \"Template not found\""));
    assert!(stderr.contains("No template named 'missing' exists."));
}

#[test]
fn test_template_add_missing_is_local_error() {
    let config_dir = temp_config_dir();
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(
        &["template", "add", "missing", "--start", "2026-03-20 11:00"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 3);
    assert!(stderr.contains("\"error\": \"Template not found\""));
}

#[test]
fn test_template_add_invalid_start_is_local_error() {
    let config_dir = temp_config_dir();
    seed_template(&config_dir, "focus");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(
        &["template", "add", "focus", "--start", "notadate"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 4);
    assert!(stderr.contains("Invalid date"));
}

#[test]
fn test_template_save_rejects_overwrite_without_force() {
    let config_dir = temp_config_dir();
    seed_template(&config_dir, "focus");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(
        &["template", "save", "focus", "fake-id"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 4);
    assert!(stderr.contains("already exists"));
    assert!(stderr.contains("--force"));
}

#[test]
fn test_template_save_force_bypasses_local_overwrite_error() {
    let config_dir = temp_config_dir();
    seed_template(&config_dir, "focus");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(
        &["template", "save", "focus", "fake-id", "--force"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_ne!(code, 4);
    assert!(!stderr.contains("already exists"));
}

#[test]
fn test_template_list_reads_seeded_template() {
    let config_dir = temp_config_dir();
    seed_template(&config_dir, "focus");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (stdout, _, code) = calx_env(
        &["template", "list", "-o", "json"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("\"name\": \"focus\""));
}

#[test]
fn test_template_list_human_marks_all_day_templates() {
    let config_dir = temp_config_dir();
    seed_all_day_template(&config_dir, "trip");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (stdout, _, code) = calx_env(
        &["template", "list", "-o", "human"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("all-day"));
}

#[test]
fn test_template_show_human_includes_alerts_and_repeat() {
    let config_dir = temp_config_dir();
    seed_template(&config_dir, "focus");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (stdout, _, code) = calx_env(
        &["template", "show", "focus", "-o", "human"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("Alerts: 10m, 30m"));
    assert!(stdout.contains("Repeat: weekly"));
    assert!(stdout.contains("URL: https://example.com/focus"));
}

#[test]
fn test_template_show_all_day_human_uses_inclusive_dates() {
    let config_dir = temp_config_dir();
    seed_all_day_template(&config_dir, "trip");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (stdout, _, code) = calx_env(
        &["template", "show", "trip", "-o", "human"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("When: 2026-03-20 to 2026-03-22 (All Day)"));
}

#[test]
fn test_template_delete_removes_seeded_template() {
    let config_dir = temp_config_dir();
    seed_template(&config_dir, "focus");
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, _, code) = calx_env(
        &["template", "delete", "focus", "-o", "json"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 0);
    let (stdout, _, code) = calx_env(
        &["template", "list", "-o", "json"],
        &[("CALX_CONFIG_DIR", config_dir_str.as_str())],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "[]");
}

#[test]
fn test_undo_without_record_is_local_error() {
    let config_dir = temp_config_dir();
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(&["undo"], &[("CALX_CONFIG_DIR", config_dir_str.as_str())]);
    assert_eq!(code, 4);
    assert!(stderr.contains("No undoable action recorded."));
}

#[test]
fn test_undo_unavailable_record_is_local_error() {
    let config_dir = temp_config_dir();
    seed_unavailable_undo(&config_dir);
    let config_dir_str = config_dir.to_string_lossy().into_owned();
    let (_, stderr, code) = calx_env(&["undo"], &[("CALX_CONFIG_DIR", config_dir_str.as_str())]);
    assert_eq!(code, 4);
    assert!(stderr.contains("Undo unavailable for the last action: test fixture"));
}

// -----------------------------------------------------------
// search rejects invalid dates
// -----------------------------------------------------------

#[test]
fn test_search_invalid_from_date() {
    let (_, stderr, code) = calx(&["search", "test", "--from", "notadate"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("Invalid date") || stderr.contains("error"),
        "search should reject invalid --from: {stderr}"
    );
}

#[test]
fn test_search_invalid_to_date() {
    let (_, stderr, code) = calx(&["search", "test", "--to", "notadate"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("Invalid date") || stderr.contains("error"),
        "search should reject invalid --to: {stderr}"
    );
}

#[test]
fn test_search_blank_query_rejected() {
    let (_, stderr, code) = calx(&["search", ""]);
    assert_eq!(code, 4);
    assert!(stderr.contains("query must not be empty"));
}

#[test]
fn test_search_blank_calendar_rejected() {
    let (_, stderr, code) = calx(&["search", "test", "--calendar", ""]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--calendar must not be empty"));
}

#[test]
fn test_events_reversed_date_range() {
    let (_, stderr, code) = calx(&["events", "--from", "2026-03-25", "--to", "2026-03-18"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("to date must be on or after from date"),
        "events should reject reversed date range: {stderr}"
    );
}

#[test]
fn test_search_reversed_date_range() {
    let (_, stderr, code) = calx(&[
        "search",
        "test",
        "--from",
        "2026-03-25",
        "--to",
        "2026-03-18",
    ]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("to date must be on or after from date"),
        "search should reject reversed date range: {stderr}"
    );
}

#[test]
fn test_free_reversed_date_range() {
    let (_, stderr, code) = calx(&["free", "--from", "2026-03-25", "--to", "2026-03-18"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("to date must be on or after from date"),
        "free should reject reversed date range: {stderr}"
    );
}

#[test]
fn test_free_reversed_time_window() {
    let (_, stderr, code) = calx(&["free", "--after", "17:00", "--before", "09:00"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("--after must be earlier than --before"),
        "free should reject reversed time window: {stderr}"
    );
}

#[test]
fn test_update_invalid_start_date() {
    let (_, stderr, code) = calx(&["update", "fake-id", "--start", "notadate"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("Invalid date"),
        "update should reject invalid --start before calendar access: {stderr}"
    );
}

#[test]
fn test_update_without_changes_rejected() {
    let (_, stderr, code) = calx(&["update", "fake-id"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("No changes specified for update."));
}

#[test]
fn test_update_blank_query_rejected() {
    let (_, stderr, code) = calx(&["update", "--query", "", "--title", "Renamed"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--query must not be empty"));
}

#[test]
fn test_update_blank_title_rejected() {
    let (_, stderr, code) = calx(&["update", "fake-id", "--title", ""]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--title must not be empty"));
}

#[test]
fn test_update_end_before_start() {
    let (_, stderr, code) = calx(&[
        "update",
        "fake-id",
        "--start",
        "2026-03-20 15:00",
        "--end",
        "2026-03-20 14:00",
    ]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("end time must be after start"),
        "update should reject end < start before calendar access: {stderr}"
    );
}

#[test]
fn test_update_rejects_zero_duration() {
    let (_, stderr, code) = calx(&[
        "update",
        "fake-id",
        "--start",
        "2026-03-20 15:00",
        "--end",
        "2026-03-20 15:00",
    ]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("end time must be after start"),
        "update should reject zero-duration timed event before calendar access: {stderr}"
    );
}

#[test]
fn test_update_all_day_rejects_time_input() {
    let (_, stderr, code) = calx(&[
        "update",
        "fake-id",
        "--all-day",
        "true",
        "--start",
        "2026-03-20 10:00",
    ]);
    assert_eq!(code, 4);
    assert!(
        stderr.contains("Invalid date"),
        "update --all-day should reject time input before calendar access: {stderr}"
    );
}

#[test]
fn test_update_invalid_url_rejected() {
    let (_, stderr, code) = calx(&["update", "fake-id", "--url", "http://[::1"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("Invalid URL"));
}

#[test]
fn test_yaml_error_output_uses_yaml() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--url",
        "http://[::1",
        "-o",
        "yaml",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("error:"));
    assert!(!stderr.trim_start().starts_with('{'));
}

#[test]
fn test_csv_error_output_uses_csv() {
    let (_, stderr, code) = calx(&[
        "add",
        "--title",
        "Bad",
        "--start",
        "2026-03-20 10:00",
        "--end",
        "2026-03-20 11:00",
        "--url",
        "http://[::1",
        "-o",
        "csv",
    ]);
    assert_eq!(code, 4);
    let mut lines = stderr.lines();
    assert_eq!(lines.next(), Some("code,error,hint,why"));
    assert!(
        lines
            .next()
            .is_some_and(|line| line.contains(",Invalid argument,") && line.contains("Invalid URL"))
    );
}

// -----------------------------------------------------------
// show --no-color
// -----------------------------------------------------------

#[test]
fn test_show_no_color_flag_accepted() {
    let (_, stderr, _) = calx(&["show", "--no-color", "fake-id"]);
    // Should not fail due to flag parsing (will fail on event not found, which is fine)
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_show_query_exact_flag_accepted() {
    let (_, stderr, _) = calx(&["show", "--query", "test", "--exact"]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_update_scope_flag_accepted() {
    let (_, stderr, _) = calx(&["update", "fake-id", "--scope", "future"]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_update_clear_flags_accepted() {
    let (_, stderr, _) = calx(&[
        "update",
        "fake-id",
        "--clear-location",
        "--clear-url",
        "--clear-notes",
    ]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_delete_scope_flag_accepted() {
    let (_, stderr, _) = calx(&["delete", "fake-id", "--scope", "this"]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_show_query_flags_accepted() {
    let (_, stderr, _) = calx(&["show", "--query", "Test", "--in-calendar", "Work"]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_update_query_flags_accepted() {
    let (_, stderr, _) = calx(&[
        "update",
        "--query",
        "Test",
        "--in-calendar",
        "Work",
        "--title",
        "Renamed",
    ]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_delete_query_flags_accepted() {
    let (_, stderr, _) = calx(&[
        "delete",
        "--query",
        "Test",
        "--in-calendar",
        "Work",
        "--dry-run",
    ]);
    assert!(!stderr.contains("unexpected argument"));
}

#[test]
fn test_delete_blank_query_rejected() {
    let (_, stderr, code) = calx(&["delete", "--query", "", "--dry-run"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--query must not be empty"));
}

#[test]
fn test_delete_blank_event_id_rejected() {
    let (_, stderr, code) = calx(&["delete", "", "--dry-run"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("EVENT_ID must not be empty"));
}

#[test]
fn test_delete_blank_in_calendar_rejected() {
    let (_, stderr, code) = calx(&[
        "delete",
        "--query",
        "Test",
        "--in-calendar",
        "",
        "--dry-run",
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--in-calendar must not be empty"));
}

#[test]
fn test_show_query_invalid_from_date() {
    let (_, stderr, code) = calx(&["show", "--query", "Test", "--from", "notadate"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("Invalid date"));
}

#[test]
fn test_show_blank_query_rejected() {
    let (_, stderr, code) = calx(&["show", "--query", ""]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--query must not be empty"));
}

#[test]
fn test_show_blank_event_id_rejected() {
    let (_, stderr, code) = calx(&["show", ""]);
    assert_eq!(code, 4);
    assert!(stderr.contains("EVENT_ID must not be empty"));
}

#[test]
fn test_show_blank_in_calendar_rejected() {
    let (_, stderr, code) = calx(&["show", "--query", "Test", "--in-calendar", ""]);
    assert_eq!(code, 4);
    assert!(stderr.contains("--in-calendar must not be empty"));
}

#[test]
fn test_update_blank_event_id_rejected() {
    let (_, stderr, code) = calx(&["update", "", "--title", "Renamed"]);
    assert_eq!(code, 4);
    assert!(stderr.contains("EVENT_ID must not be empty"));
}

// -----------------------------------------------------------
// Invalid --after/--before/--sort
// -----------------------------------------------------------

#[test]
fn test_invalid_after_value() {
    let (_, stderr, code) = calx(&["today", "--after", "nottime"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("Invalid date") || stderr.contains("--after"));
}

#[test]
fn test_invalid_before_value() {
    let (_, stderr, code) = calx(&["today", "--before", "xyz"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("Invalid date") || stderr.contains("--before"));
}

#[test]
fn test_invalid_sort_value() {
    let (_, stderr, code) = calx(&["today", "--sort", "nonsense"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("Unknown sort key") || stderr.contains("error"));
}

// -----------------------------------------------------------
// --no-color and --no-header
// -----------------------------------------------------------

#[test]
fn test_no_color_flag() {
    let (stdout, _, _) = calx(&["today", "--no-color"]);
    assert!(
        !stdout.contains("\x1b["),
        "--no-color should produce no ANSI codes"
    );
}

#[test]
fn test_no_color_calendars() {
    let (stdout, _, _) = calx(&["calendars", "--no-color"]);
    assert!(
        !stdout.contains("\x1b["),
        "--no-color should produce no ANSI codes for calendars"
    );
}

#[test]
fn test_no_header_flag() {
    let (with_header, _, _) = calx(&["today", "--no-color"]);
    let (without_header, _, _) = calx(&["today", "--no-color", "--no-header"]);
    // --no-header should produce fewer lines (no column header rows)
    let with_count = with_header.lines().count();
    let without_count = without_header.lines().count();
    assert!(
        without_count <= with_count,
        "--no-header should have same or fewer lines: {without_count} vs {with_count}"
    );
}

#[test]
fn test_no_header_no_color_accepted() {
    let (_, stderr, _) = calx(&["today", "--no-header", "--no-color"]);
    assert!(!stderr.contains("unexpected argument"));
}

// -----------------------------------------------------------
// Help text completeness
// -----------------------------------------------------------

#[test]
fn test_help_lists_all_commands() {
    let (stdout, _, _) = calx(&["--help"]);
    for cmd in &[
        "calendars",
        "events",
        "today",
        "upcoming",
        "add",
        "update",
        "delete",
        "show",
        "search",
        "next",
        "completions",
    ] {
        assert!(stdout.contains(cmd), "Help should list command: {cmd}");
    }
}

#[test]
fn test_help_lists_all_output_formats() {
    let (stdout, _, _) = calx(&["--help"]);
    for fmt in &["auto", "human", "json", "yaml", "table", "csv", "tsv"] {
        assert!(
            stdout.contains(fmt),
            "Help should list output format: {fmt}"
        );
    }
}

#[test]
fn test_help_shows_examples() {
    let (stdout, _, _) = calx(&["--help"]);
    assert!(stdout.contains("Quick Start:"));
    assert!(stdout.contains("Workflows:"));
}

// -----------------------------------------------------------
// Subcommand help validation
// -----------------------------------------------------------

#[test]
fn test_events_help() {
    let (stdout, _, code) = calx(&["events", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--from"));
    assert!(stdout.contains("--to"));
    assert!(stdout.contains("--calendar"));
}

#[test]
fn test_add_help() {
    let (stdout, _, code) = calx(&["add", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--title"));
    assert!(stdout.contains("--start"));
    assert!(stdout.contains("--end"));
    assert!(stdout.contains("--calendar"));
    assert!(stdout.contains("--notes"));
    assert!(stdout.contains("--all-day"));
}

#[test]
fn test_update_help() {
    let (stdout, _, code) = calx(&["update", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--query"));
    assert!(stdout.contains("--in-calendar"));
    assert!(stdout.contains("--title"));
    assert!(stdout.contains("--start"));
    assert!(stdout.contains("--end"));
}

#[test]
fn test_search_help() {
    let (stdout, _, code) = calx(&["search", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--calendar"));
    assert!(stdout.contains("--from"));
    assert!(stdout.contains("--to"));
}

#[test]
fn test_delete_help() {
    let (stdout, _, code) = calx(&["delete", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--query"));
    assert!(stdout.contains("--in-calendar"));
}

#[test]
fn test_show_help() {
    let (stdout, _, code) = calx(&["show", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--query"));
    assert!(stdout.contains("--in-calendar"));
}

#[test]
fn test_next_help() {
    let (stdout, _, code) = calx(&["next", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("watch(1)"));
    assert!(stdout.contains("timed"));
    assert!(stdout.contains("30 days"));
}
