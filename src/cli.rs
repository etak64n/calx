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

    /// Output format
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
    },

    /// Show today's schedule
    Today {
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Show upcoming events for the next N days
    Upcoming {
        /// Number of days to look ahead
        #[arg(long, default_value = "7")]
        days: u32,
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
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
    },

    /// Modify an existing event
    Update {
        /// Event identifier (from 'show' or 'events -o json')
        event_id: String,
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
    },

    /// Remove an event
    Delete {
        /// Event identifier
        event_id: String,
    },

    /// Display full details of an event
    Show {
        /// Event identifier
        event_id: String,
    },

    /// Find events by keyword (searches title, notes, location, and calendar)
    Search {
        /// Search keyword
        query: String,
        /// Start of search range (default: today)
        #[arg(long)]
        from: Option<String>,
        /// End of search range (default: 90 days ahead)
        #[arg(long)]
        to: Option<String>,
    },

    /// Show the next upcoming event (composable with `watch(1)`)
    Next {
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Import events from an ICS or CSV file (use "-" for stdin)
    Import {
        /// Path to .ics or .csv file, or "-" for stdin
        file: String,
    },

    /// Generate shell completion script
    Completions {
        /// Target shell: bash, zsh, fish
        shell: Shell,
    },
}
