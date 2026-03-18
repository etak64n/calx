# Testing

This project has two layers of test coverage:

- fast unit and integration tests that do not require real Calendar data
- live EventKit tests that exercise the real macOS Calendar database

## Standard Test Suite

Run the regular test suite:

```bash
cargo test
```

Run lint checks:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

These cover CLI parsing, validation, date parsing, formatting, and internal logic.

## Live EventKit Tests

Live EventKit tests are implemented in `tests/live_eventkit.rs`.
They are ignored by default because they require:

- macOS Calendar permission for `target/debug/calx`
- a writable EventKit source
- serialized execution

Run them with:

```bash
CALX_LIVE_TEST_CALENDAR="calx-live" \
  cargo test --test live_eventkit -- --ignored --test-threads=1
```

### Calendar Selection

Set `CALX_LIVE_TEST_CALENDAR` to the name of a dedicated test calendar.

- If the calendar already exists, the tests reuse it.
- If the calendar does not exist, the tests try to create it on a writable EventKit source.
- The tests create, update, search, list, and delete real events in that calendar.

Using a dedicated calendar such as `calx-live` is recommended.

### Permission Notes

If access has not been granted yet, macOS must allow Calendar access for `target/debug/calx` in:

`System Settings > Privacy & Security > Calendars`

You can verify access with:

```bash
cargo run -- calendars -o json
```

If permission is available, this command should return calendar data instead of an access error.

### What The Live Tests Cover

The live suite currently verifies:

- `add -> show -> update -> search -> events -> delete`
- one-sided invalid `update` operations that would make `start > end`
