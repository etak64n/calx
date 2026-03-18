# calx

Native macOS Calendar CLI built on EventKit.

A fast, single-binary command-line tool for managing Apple Calendar events. Built in Rust with direct EventKit framework access via `objc2`.

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
# List all calendars
calx calendars

# Show today's events
calx today

# Show upcoming 7 days
calx upcoming

# Show upcoming 3 days for a specific calendar
calx upcoming --days 3 --calendar "Work"

# List events in a date range
calx events --from 2026-03-18 --to 2026-03-25

# Add an event
calx add --title "Meeting" --start "2026-03-20 14:00" --end "2026-03-20 15:00"

# Add an all-day event
calx add --title "Holiday" --start 2026-03-25 --end 2026-03-25 --all-day

# Delete an event
calx delete <event-id>
```

## JSON Output

All commands support `--output json` for scripting and automation:

```bash
calx today --output json
calx calendars -o json
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
