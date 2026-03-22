# calx

`calx` is a native macOS command-line tool for Apple Calendar.

It lets you browse, search, create, update, and delete calendar events from the terminal, with natural language dates, structured output formats, and direct integration with EventKit.

## Highlights

- Native Apple Calendar integration on macOS
- Natural language date input like `tomorrow 3pm` or `next friday`
- Fast event listing, filtering, and search
- Create, update, show, and delete events from the CLI
- Duplicate events, save reusable templates, and undo the last supported change
- Conflict checks and free-slot discovery for scheduling workflows
- Export event data as JSON, YAML, CSV, TSV, or table
- Single binary with no runtime dependencies

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

## Quick Start

```bash
# Show today's schedule
calx today

# Show upcoming events
calx upcoming

# List calendars
calx calendars

# Show the next future timed event within the next 30 days
calx next
```

## Common Tasks

```bash
# List events in a date range
calx events --from 2026-03-18 --to 2026-03-25

# Search across title, notes, location, calendar name, and URL
calx search "meeting"

# Create an event
calx add --title "Meeting" --start "2026-03-20 14:00" --end "2026-03-20 15:00"

# Check conflicts before scheduling
calx conflicts --start "2026-03-20 14:00" --end "2026-03-20 15:00"

# Update an event
calx update <event-id> --title "New Title" --start "tomorrow 2pm"

# Duplicate an event
calx duplicate <event-id> --start "2026-03-27 14:00"

# Show full event details
calx show <event-id>

# Delete an event
calx delete <event-id>

# Save and reuse a template
calx template save weekly-1on1 --query "Weekly 1:1" --exact
# Overwrite an existing template name
calx template save weekly-1on1 --query "Weekly 1:1" --exact --force
calx template add weekly-1on1 --start "next friday 15:00"

# Undo the last supported change
calx undo
```

## Natural Language Dates

`calx` accepts both explicit timestamps and natural language input.

```bash
calx add --title "Meeting" --start "tomorrow 3pm" --end "tomorrow 4pm"
calx add --title "Lunch" --start "next friday 12pm" --end "next friday 1pm"
calx events --from tomorrow --to "in 7 days"
```

## Filtering and Sorting

```bash
calx today --after 09:00 --before 17:00
calx upcoming --sort title --limit 10
calx events --from tomorrow --sort duration
```

## Output Formats

Event commands support multiple output formats via `-o`:

```bash
calx today -o json
calx today -o yaml
calx today -o table
calx today -o csv
calx today -o tsv
```

## Permissions

On first run, macOS will prompt for Calendar access.

Grant access in **System Settings > Privacy & Security > Calendars**.

## License

MIT
