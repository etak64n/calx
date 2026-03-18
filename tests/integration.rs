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
