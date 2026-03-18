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

# List all calendars
calx calendars
```

## Natural Language Dates

The `add`, `update`, and `search` commands support natural language dates:

```bash
calx add --title "Meeting" --start "tomorrow 3pm" --end "tomorrow 4pm"
calx add --title "Lunch" --start "next friday 12pm" --end "next friday 1pm"
calx add --title "Review" --start "in 3 days" --end "in 3 days"
```

## Event Management

```bash
# Create an event
calx add --title "Meeting" --start "2026-03-20 14:00" --end "2026-03-20 15:00"

# Create an all-day event
calx add --title "Holiday" --start 2026-03-25 --end 2026-03-25 --all-day

# Create an event interactively with guided prompts
calx interactive

# Update an event
calx update <event-id> --title "New Title" --start "tomorrow 2pm"

# Show event details
calx show <event-id>

# Delete an event
calx delete <event-id>
```

## Search

```bash
calx search "meeting"
calx search "lunch" --from 2026-03-01 --to 2026-06-01
```

## Export & Import

```bash
# Export to ICS
calx export --format ics > events.ics

# Export to CSV
calx export --format csv --calendar "Work" > work.csv

# Import from ICS or CSV
calx import events.ics
calx import data.csv
```

## Watch Mode

Live-display the next upcoming event with a countdown timer:

```bash
calx watch
```

## JSON Output

All commands support `--output json` for scripting and automation:

```bash
calx today --output json
calx calendars -o json
calx events --from 2026-03-18 --to 2026-03-25 -o json
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
