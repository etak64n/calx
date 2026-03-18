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

The `add` and `update` commands support natural language:

```bash
calx add --title "Meeting" --start "tomorrow 3pm" --end "tomorrow 4pm"
calx add --title "Lunch" --start "next friday 12pm" --end "next friday 1pm"
calx add --title "Review" --start "in 3 days" --end "in 3 days"
```

## Event Management

```bash
# Create an event
calx add --title "Meeting" --start "2026-03-20 14:00" --end "2026-03-20 15:00"

# Create an event with location and URL
calx add --title "Lunch" --start "tomorrow 12pm" --end "tomorrow 1pm" \
  --location "Cafe" --url "https://example.com"

# Create an all-day event
calx add --title "Holiday" --start 2026-03-25 --end 2026-03-25 --all-day

# Update an event
calx update <event-id> --title "New Title" --start "tomorrow 2pm"
calx update <event-id> --location "New Place" --url "https://new.example.com"

# Show event details
calx show <event-id>

# Delete an event
calx delete <event-id>
```

## Search

Searches across title, notes, location, calendar name, and URL:

```bash
calx search "meeting"
calx search "lunch" --from 2026-03-01 --to 2026-06-01
```

## Output Formats

All commands support 7 output formats via `-o`:

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

## Export & Import

```bash
# Export to ICS
calx events --from 2026-03-01 --to 2026-03-31 -o ics > events.ics

# Export to CSV
calx events --calendar "Work" -o csv > work.csv

# Import from ICS or CSV
calx import events.ics
calx import data.csv

# Import from stdin
cat events.ics | calx import -
```

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
