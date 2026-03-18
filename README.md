# calx

Native macOS Calendar CLI built on EventKit.

A fast, single-binary command-line tool for managing Apple Calendar events. Built in Rust with direct EventKit framework access via `objc2`. No runtime dependencies.

## Install

```bash
brew install etak64n/tap/calx
```

Or build from source:

```bash
git clone https://github.com/etak64n/calx.git
cd calx
cargo install --path .
```

## Usage

```bash
# Show today's schedule
calx today

# Show upcoming 7 days
calx upcoming

# Show upcoming 3 days for a specific calendar
calx upcoming --days 3 --calendar "Work"

# List events in a date range
calx events --from 2026-03-18 --to 2026-03-25

# Show the next (or current) event
calx next

# List all calendars
calx calendars
```

## Natural Language Dates

The `add`, `update`, and `events` commands support natural language:

```bash
calx add --title "Meeting" --start "tomorrow 3pm" --end "tomorrow 4pm"
calx add --title "Lunch" --start "next friday 12pm" --end "next friday 1pm"
calx events --from tomorrow --to "in 7 days"
```

## Event Management

```bash
# Create an event
calx add --title "Meeting" --start "2026-03-20 14:00" --end "2026-03-20 15:00"

# Create with location and URL
calx add --title "Lunch" --start "tomorrow 12pm" --end "tomorrow 1pm" \
  --location "Cafe" --url "https://example.com"

# Create a recurring event (every 2 weeks, 10 times)
calx add --title "Standup" --start "tomorrow 9am" --end "tomorrow 9:30am" \
  --repeat weekly --repeat-interval 2 --repeat-count 10

# Create an all-day event
calx add --title "Holiday" --start 2026-03-25 --end 2026-03-25 --all-day

# Update an event
calx update <event-id> --title "New Title" --start "tomorrow 2pm"

# Show event details
calx show <event-id>

# Delete an event (preview first)
calx delete <event-id> --dry-run
calx delete <event-id>
```

## Search

Searches across title, notes, location, calendar name, and URL:

```bash
calx search "meeting"
calx search "lunch" --from 2026-03-01 --to 2026-06-01
```

## Filtering & Sorting

```bash
calx today --after 09:00 --before 17:00     # business hours only
calx upcoming --sort title --limit 10        # top 10 by title
calx events --from tomorrow --sort duration  # by duration
```

## Output Formats

Event commands support 7 output formats via `-o` (ics only applies to event data):

```bash
calx today                # human-readable (default)
calx today -o json        # JSON
calx today -o yaml        # YAML
calx today -o table       # box-drawing table
calx today -o csv         # CSV
calx today -o tsv         # TSV
calx today -o ics         # ICS (iCalendar)
```

## Display Options

```bash
calx today -v             # verbose: show all fields (id, notes, location, etc.)
calx today --fields title,start,calendar -o json  # select specific fields
calx today --no-color     # disable ANSI colors
calx today --no-header    # suppress column headers
```

## Export

```bash
calx events --from 2026-03-01 --to 2026-03-31 -o ics > events.ics
calx events --calendar "Work" -o csv > work.csv
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Calendar access denied or timeout |
| 3 | Calendar or event not found |
| 4 | Invalid date or argument |
| 5 | EventKit error |

## Shell Completions

```bash
# Zsh
calx completions zsh > ~/.zfunc/_calx

# Bash
calx completions bash > /etc/bash_completion.d/calx

# Fish
calx completions fish > ~/.config/fish/completions/calx.fish
```

## Permissions

On first run, macOS will prompt for Calendar access. Grant access in **System Settings > Privacy & Security > Calendars**.

## License

MIT
