use std::process::Command;

fn calx(args: &[&str]) -> (String, String, i32) {
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
    for fmt in &["human", "json", "yaml", "csv", "tsv", "table", "ics"] {
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
    let (_, stderr, _) = calx(&["today", "--fields", "title,start"]);
    assert!(!stderr.contains("unexpected argument"));
}

// -----------------------------------------------------------
// Import file validation
// -----------------------------------------------------------

#[test]
fn test_import_nonexistent_file() {
    let (_, stderr, code) = calx(&["import", "/nonexistent/file.ics"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("Failed to read file") || stderr.contains("error"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_import_unknown_format() {
    // Create a temp file with wrong extension
    let tmp = std::env::temp_dir().join("calx_test.xyz");
    std::fs::write(&tmp, "hello").unwrap();
    let (_, stderr, code) = calx(&["import", tmp.to_str().unwrap()]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("Unknown file format") || stderr.contains("error"),
        "stderr: {stderr}"
    );
    std::fs::remove_file(tmp).ok();
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
        "import",
        "completions",
    ] {
        assert!(stdout.contains(cmd), "Help should list command: {cmd}");
    }
}

#[test]
fn test_help_lists_all_output_formats() {
    let (stdout, _, _) = calx(&["--help"]);
    for fmt in &["human", "json", "yaml", "table", "csv", "tsv", "ics"] {
        assert!(
            stdout.contains(fmt),
            "Help should list output format: {fmt}"
        );
    }
}

#[test]
fn test_help_shows_examples() {
    let (stdout, _, _) = calx(&["--help"]);
    assert!(stdout.contains("Examples:"));
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
    assert!(stdout.contains("--title"));
    assert!(stdout.contains("--start"));
    assert!(stdout.contains("--end"));
}

#[test]
fn test_search_help() {
    let (stdout, _, code) = calx(&["search", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--from"));
    assert!(stdout.contains("--to"));
}

#[test]
fn test_import_help() {
    let (stdout, _, code) = calx(&["import", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("stdin"));
}

#[test]
fn test_next_help() {
    let (stdout, _, code) = calx(&["next", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("watch(1)") || stdout.contains("upcoming"));
}
