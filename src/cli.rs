use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Parser)]
#[command(
    name = "calx",
    version,
    about = "Native macOS Calendar CLI built on EventKit"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format
    #[arg(long, short, global = true, default_value = "human")]
    pub output: OutputFormat,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List all calendars
    Calendars,

    /// List events in a date range
    Events {
        /// Start date (YYYY-MM-DD). Defaults to today
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD). Defaults to 7 days from start
        #[arg(long)]
        to: Option<String>,
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Show today's events
    Today {
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Show upcoming events
    Upcoming {
        /// Number of days to look ahead
        #[arg(long, default_value = "7")]
        days: u32,
        /// Filter by calendar name
        #[arg(long)]
        calendar: Option<String>,
    },

    /// Add a new event
    Add {
        /// Event title
        #[arg(long)]
        title: String,
        /// Start (YYYY-MM-DD HH:MM or YYYY-MM-DD for all-day)
        #[arg(long)]
        start: String,
        /// End (YYYY-MM-DD HH:MM or YYYY-MM-DD for all-day)
        #[arg(long)]
        end: String,
        /// Calendar name (uses default if omitted)
        #[arg(long)]
        calendar: Option<String>,
        /// Notes
        #[arg(long)]
        notes: Option<String>,
        /// All-day event
        #[arg(long, default_value_t = false)]
        all_day: bool,
    },

    /// Delete an event by ID
    Delete {
        /// Event identifier
        event_id: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell type
        shell: Shell,
    },
}
