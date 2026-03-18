use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Yaml,
    Table,
    Csv,
    Tsv,
    Ics,
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
        Supports natural language dates, JSON output, ICS/CSV export, and more.",
    after_help = "Examples:\n  \
        calx today                                         Show today's events\n  \
        calx add --title \"Meeting\" --start \"tomorrow 3pm\" --end \"tomorrow 4pm\"\n  \
        calx add --title \"Lunch\" --start \"next friday 12pm\" --end \"next friday 1pm\"\n  \
        calx search \"lunch\" --from 2026-03-01\n  \
        calx events --from 2026-03-18 --to 2026-03-25 -o json\n  \
        calx today -o ics > events.ics"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format (ics only works with event data; ignored by completions)
    #[arg(long, short, global = true, default_value = "human")]
    pub output: OutputFormat,

    /// Show all fields (id, notes, etc.)
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Comma-separated list of fields to display (e.g. title,start,end,calendar)
    #[arg(long, global = true)]
    pub fields: Option<String>,

    /// Suppress column headers
    #[arg(long, global = true)]
    pub no_header: bool,

    /// Disable color output
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List all calendars with their sources
    Calendars,

    /// Query events within a date range
    Events {
        /// Start date (YYYY-MM-DD or natural language). Defaults to today
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD or natural language). Defaults to 7 days from start
        #[arg(long)]
        to: Option<String>,
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
        /// Only show events starting at or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events starting before this time (HH:MM)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show today's schedule
    Today {
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
        /// Only show events starting at or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events starting before this time (HH:MM)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show upcoming events for the next N days
    Upcoming {
        /// Number of days to look ahead
        #[arg(long, default_value = "7")]
        days: u32,
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
        /// Sort by: date, start, title, calendar, duration
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of events to display
        #[arg(long)]
        limit: Option<usize>,
        /// Only show events starting at or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events starting before this time (HH:MM)
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
        /// Target calendar (uses default if omitted)
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
        /// Filter by calendar name
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
        /// Require an exact match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Limit --query to a calendar name
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
        #[arg(long)]
        location: Option<String>,
        /// New URL
        #[arg(long)]
        url: Option<String>,
        /// New notes
        #[arg(long)]
        notes: Option<String>,
        /// Move to a different calendar
        #[arg(long)]
        calendar: Option<String>,
        /// Set as all-day event
        #[arg(long)]
        all_day: Option<bool>,
        /// Scope for recurring events: this occurrence or this and future occurrences
        #[arg(long)]
        scope: Option<RecurrenceScope>,
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
        /// Require an exact match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Limit --query to a calendar name
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
        /// Require an exact match when resolving --query
        #[arg(long, requires = "query")]
        exact: bool,
        /// Limit --query to a calendar name
        #[arg(long, requires = "query")]
        in_calendar: Option<String>,
        /// Start of --query range
        #[arg(long, requires = "query")]
        from: Option<String>,
        /// End of --query range
        #[arg(long, requires = "query")]
        to: Option<String>,
    },

    /// Find events by keyword (searches title, notes, location, and calendar)
    Search {
        /// Search keyword
        query: String,
        /// Require an exact match
        #[arg(long)]
        exact: bool,
        /// Limit search to a calendar name
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
        /// Only show events starting at or after this time (HH:MM)
        #[arg(long)]
        after: Option<String>,
        /// Only show events starting before this time (HH:MM)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show the next upcoming event (composable with `watch(1)`)
    Next {
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Generate shell completion script
    Completions {
        /// Target shell: bash, zsh, fish
        shell: Shell,
    },
}
