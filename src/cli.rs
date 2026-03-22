use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::io::{self, IsTerminal};

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Auto,
    Human,
    Json,
    Yaml,
    Table,
    Csv,
    Tsv,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum RecurrenceScope {
    This,
    Future,
}

#[derive(Parser)]
#[command(
    name = "calx",
    version,
    about = "Native macOS Calendar CLI built on EventKit",
    long_about = "Native macOS Calendar CLI built on EventKit.\n\n\
        Manage Apple Calendar events directly from the terminal.\n\
        Supports natural language dates, structured output, and more.",
    after_help = "Quick Start:\n  \
        calx today                                         Show today's events\n  \
        calx add --title \"Meeting\" --start \"tomorrow 3pm\" --end \"tomorrow 4pm\"\n  \
        calx search \"lunch\" --from 2026-03-01\n  \
        calx template save weekly-1on1 --query \"Weekly 1:1\" --exact\n\n  \
        Workflows:\n  \
        calx search \"1:1\" --calendar Work -o json        Find candidates for automation\n  \
        calx duplicate <event-id> --start \"next friday 15:00\"\n  \
        calx conflicts --start \"2026-03-20 14:00\" --end \"2026-03-20 15:00\"\n  \
        calx undo                                          Revert the last supported change\n\n  \
        Output:\n  \
        -o auto (default) shows human output on a TTY and JSON when piped\n  \
        Use -o json for guaranteed machine-readable output"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format. 'auto' shows human output on a TTY and JSON when piped (ignored by completions)
    #[arg(long, short, global = true, default_value = "auto")]
    pub output: OutputFormat,

    /// Show all fields (id, notes, etc.)
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Comma-separated fields for structured output on event-list commands (events, today, upcoming, search, next, conflicts)
    #[arg(long, global = true)]
    pub fields: Option<String>,

    /// Suppress column headers
    #[arg(long, global = true)]
    pub no_header: bool,

    /// Disable color output
    #[arg(long, global = true)]
    pub no_color: bool,
}

impl OutputFormat {
    pub fn resolve_for_stdout(self) -> Self {
        match self {
            Self::Auto => {
                if io::stdout().is_terminal() {
                    Self::Human
                } else {
                    Self::Json
                }
            }
            other => other,
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// List all calendars with their sources and IDs
    Calendars,

    /// Human-friendly daily overview with time-relative info
    ///
    /// Only --calendar, -o, and --no-color are used. --fields is not supported here.
    Agenda {
        /// Filter by calendar name
        /// or calendar ID
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Query events within a date range
    Events {
        /// Start date (YYYY-MM-DD or natural language). Defaults to today
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD or natural language). Defaults to 7 days from start
        #[arg(long)]
        to: Option<String>,
        /// Filter by calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
        /// Only show events overlapping or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events overlapping or before this time (HH:MM)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show today's schedule
    Today {
        /// Filter by calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
        /// Only show events overlapping or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events overlapping or before this time (HH:MM)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show upcoming events for the next N days
    Upcoming {
        /// Number of days to look ahead
        #[arg(long, default_value = "7")]
        days: u32,
        /// Filter by calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
        /// Only show events overlapping or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events overlapping or before this time (HH:MM)
        #[arg(long)]
        before: Option<String>,
    },

    /// Create a new event (supports natural language dates)
    Add {
        /// Event title
        #[arg(long)]
        title: String,
        /// Start date/time: YYYY-MM-DD HH:MM, "tomorrow 3pm", "next monday 10am"
        #[arg(long)]
        start: String,
        /// End date/time: YYYY-MM-DD HH:MM, "tomorrow 4pm", "next monday 11am"
        #[arg(long)]
        end: String,
        /// Target calendar name or calendar ID (uses default if omitted)
        #[arg(long)]
        calendar: Option<String>,
        /// Event location
        #[arg(long)]
        location: Option<String>,
        /// Event URL
        #[arg(long)]
        url: Option<String>,
        /// Event notes
        #[arg(long)]
        notes: Option<String>,
        /// Mark as all-day event
        #[arg(long, default_value_t = false)]
        all_day: bool,
        /// Repeat: daily, weekly, monthly, yearly
        #[arg(long)]
        repeat: Option<String>,
        /// Number of occurrences (default: forever)
        #[arg(long)]
        repeat_count: Option<u32>,
        /// Repeat every N intervals (e.g. --repeat weekly --repeat-interval 2 = every 2 weeks)
        #[arg(long)]
        repeat_interval: Option<u32>,
        /// Alert minutes before event (can be specified multiple times: --alert 10 --alert 60)
        #[arg(long)]
        alert: Vec<i64>,
        /// Check for conflicts before creating
        #[arg(long)]
        check_conflicts: bool,
    },

    /// Find free time slots in a date range
    Free {
        /// Start date (YYYY-MM-DD or natural language). Defaults to today
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD or natural language). Defaults to 5 days from start
        #[arg(long)]
        to: Option<String>,
        /// Filter by calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Minimum slot duration in minutes
        #[arg(long, default_value = "30")]
        duration: u32,
        /// Day starts at (HH:MM). Default: 09:00
        #[arg(long)]
        after: Option<String>,
        /// Day ends at (HH:MM). Default: 17:00
        #[arg(long)]
        before: Option<String>,
        /// Maximum number of slots to display
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Show events that conflict with a proposed time range
    Conflicts {
        /// Start date/time
        #[arg(long)]
        start: String,
        /// End date/time
        #[arg(long)]
        end: String,
        /// Treat --start/--end as all-day date-only values
        #[arg(long, default_value_t = false)]
        all_day: bool,
        /// Filter by calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Modify an existing event
    Update {
        /// Event identifier (mutually exclusive with --query)
        #[arg(required_unless_present = "query", conflicts_with = "query")]
        event_id: Option<String>,
        /// Resolve a single event by search query
        #[arg(
            long,
            required_unless_present = "event_id",
            conflicts_with = "event_id"
        )]
        query: Option<String>,
        /// Require an exact title match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Pick interactively when multiple events match --query
        #[arg(short, long, requires = "query")]
        interactive: bool,
        /// Limit --query to a calendar name or calendar ID
        #[arg(long, requires = "query")]
        in_calendar: Option<String>,
        /// Start of --query range
        #[arg(long, requires = "query")]
        from: Option<String>,
        /// End of --query range
        #[arg(long, requires = "query")]
        to: Option<String>,
        /// New title
        #[arg(long)]
        title: Option<String>,
        /// New start date/time (supports natural language)
        #[arg(long)]
        start: Option<String>,
        /// New end date/time (supports natural language)
        #[arg(long)]
        end: Option<String>,
        /// New location
        #[arg(long, conflicts_with = "clear_location")]
        location: Option<String>,
        /// Clear the location
        #[arg(long, conflicts_with = "location")]
        clear_location: bool,
        /// New URL
        #[arg(long, conflicts_with = "clear_url")]
        url: Option<String>,
        /// Clear the URL
        #[arg(long, conflicts_with = "url")]
        clear_url: bool,
        /// New notes
        #[arg(long, conflicts_with = "clear_notes")]
        notes: Option<String>,
        /// Clear the notes
        #[arg(long, conflicts_with = "notes")]
        clear_notes: bool,
        /// Replace alerts with minutes-before values (repeat flag to set multiple alerts)
        #[arg(long, conflicts_with = "clear_alerts")]
        alert: Vec<i64>,
        /// Remove all alerts
        #[arg(long, conflicts_with = "alert")]
        clear_alerts: bool,
        /// Move to a different calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Set as all-day event
        #[arg(long)]
        all_day: Option<bool>,
        /// Scope for recurring events: this occurrence or this and future occurrences
        #[arg(long)]
        scope: Option<RecurrenceScope>,
    },

    /// Duplicate an existing event into a new event
    Duplicate {
        /// Event identifier (mutually exclusive with --query)
        #[arg(required_unless_present = "query", conflicts_with = "query")]
        event_id: Option<String>,
        /// Resolve a single event by search query
        #[arg(
            long,
            required_unless_present = "event_id",
            conflicts_with = "event_id"
        )]
        query: Option<String>,
        /// Require an exact title match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Pick interactively when multiple events match --query
        #[arg(short, long, requires = "query")]
        interactive: bool,
        /// Limit --query to a calendar name or calendar ID
        #[arg(long, requires = "query")]
        in_calendar: Option<String>,
        /// Start of --query range
        #[arg(long, requires = "query")]
        from: Option<String>,
        /// End of --query range
        #[arg(long, requires = "query")]
        to: Option<String>,
        /// Override the copied title
        #[arg(long)]
        title: Option<String>,
        /// New start date/time (defaults to original start)
        #[arg(long)]
        start: Option<String>,
        /// New end date/time (defaults to original end or preserved duration when only --start is given)
        #[arg(long)]
        end: Option<String>,
        /// Target calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Copy recurrence rules as well
        #[arg(long, default_value_t = false)]
        keep_recurrence: bool,
    },

    /// Remove an event
    Delete {
        /// Event identifier (mutually exclusive with --query)
        #[arg(required_unless_present = "query", conflicts_with = "query")]
        event_id: Option<String>,
        /// Resolve a single event by search query
        #[arg(
            long,
            required_unless_present = "event_id",
            conflicts_with = "event_id"
        )]
        query: Option<String>,
        /// Require an exact title match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Pick interactively when multiple events match --query
        #[arg(short, long, requires = "query")]
        interactive: bool,
        /// Limit --query to a calendar name or calendar ID
        #[arg(long, requires = "query")]
        in_calendar: Option<String>,
        /// Start of --query range
        #[arg(long, requires = "query")]
        from: Option<String>,
        /// End of --query range
        #[arg(long, requires = "query")]
        to: Option<String>,
        /// Show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,
        /// Scope for recurring events: this occurrence or this and future occurrences
        #[arg(long)]
        scope: Option<RecurrenceScope>,
    },

    /// Display full details of an event
    Show {
        /// Event identifier (mutually exclusive with --query)
        #[arg(required_unless_present = "query", conflicts_with = "query")]
        event_id: Option<String>,
        /// Resolve a single event by search query
        #[arg(
            long,
            required_unless_present = "event_id",
            conflicts_with = "event_id"
        )]
        query: Option<String>,
        /// Require an exact title match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Pick interactively when multiple events match --query
        #[arg(short, long, requires = "query")]
        interactive: bool,
        /// Limit --query to a calendar name or calendar ID
        #[arg(long, requires = "query")]
        in_calendar: Option<String>,
        /// Start of --query range
        #[arg(long, requires = "query")]
        from: Option<String>,
        /// End of --query range
        #[arg(long, requires = "query")]
        to: Option<String>,
    },

    /// Find events by keyword (searches title, notes, location, calendar, and calendar ID)
    Search {
        /// Search keyword
        query: String,
        /// Require an exact title match
        #[arg(long)]
        exact: bool,
        /// Limit search to a calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Start of search range (default: 30 days ago)
        #[arg(long)]
        from: Option<String>,
        /// End of search range (default: 90 days ahead)
        #[arg(long)]
        to: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
        /// Only show events overlapping or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events overlapping or before this time (HH:MM)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show the next future timed event within the next 30 days (composable with `watch(1)`)
    Next {
        /// Filter by calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Diagnose calendar access, permissions, and configuration
    Doctor,

    /// Manage reusable event templates
    Template {
        #[command(subcommand)]
        command: TemplateCommand,
    },

    /// Undo the last supported mutating action
    Undo,

    /// Generate shell completion script
    Completions {
        /// Target shell: bash, zsh, fish
        shell: Shell,
    },
}

#[derive(Subcommand)]
pub enum TemplateCommand {
    /// List saved templates
    List,
    /// Show one saved template
    Show {
        /// Template name
        name: String,
    },
    /// Save a template from an existing event
    Save {
        /// Template name
        name: String,
        /// Overwrite an existing template with the same name
        #[arg(long, default_value_t = false)]
        force: bool,
        /// Event identifier (mutually exclusive with --query)
        #[arg(required_unless_present = "query", conflicts_with = "query")]
        event_id: Option<String>,
        /// Resolve a single event by search query
        #[arg(
            long,
            required_unless_present = "event_id",
            conflicts_with = "event_id"
        )]
        query: Option<String>,
        /// Require an exact title match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Pick interactively when multiple events match --query
        #[arg(short, long, requires = "query")]
        interactive: bool,
        /// Limit --query to a calendar name or calendar ID
        #[arg(long, requires = "query")]
        in_calendar: Option<String>,
        /// Start of --query range
        #[arg(long, requires = "query")]
        from: Option<String>,
        /// End of --query range
        #[arg(long, requires = "query")]
        to: Option<String>,
    },
    /// Delete a saved template
    Delete {
        /// Template name
        name: String,
    },
    /// Create a new event from a saved template
    Add {
        /// Template name
        name: String,
        /// Override the template title
        #[arg(long)]
        title: Option<String>,
        /// New start date/time (required)
        #[arg(long)]
        start: String,
        /// New end date/time (defaults to preserved duration/span)
        #[arg(long)]
        end: Option<String>,
        /// Target calendar name or calendar ID
        #[arg(long)]
        calendar: Option<String>,
        /// Do not apply the template recurrence rule
        #[arg(long, default_value_t = false)]
        drop_recurrence: bool,
    },
}
