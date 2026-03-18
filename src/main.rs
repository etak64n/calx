mod cli;
mod commands;
mod error;
mod output;
mod store;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    if let Commands::Completions { shell } = cli.command {
        commands::completions::run(shell);
        return;
    }

    let store = match store::CalendarStore::new() {
        Ok(s) => s,
        Err(e) => {
            print_error(&cli, &e);
            std::process::exit(1);
        }
    };

    let result = match cli.command {
        Commands::Calendars => commands::calendars::run(&store, cli.output),
        Commands::Events {
            ref from,
            ref to,
            ref calendar,
        } => commands::events::run(
            &store,
            from.clone(),
            to.clone(),
            calendar.clone(),
            cli.output,
        ),
        Commands::Today { ref calendar } => {
            commands::today::run(&store, calendar.clone(), cli.output)
        }
        Commands::Upcoming { days, ref calendar } => {
            commands::upcoming::run(&store, days, calendar.clone(), cli.output)
        }
        Commands::Add {
            ref title,
            ref start,
            ref end,
            ref calendar,
            ref notes,
            all_day,
        } => commands::add::run(
            &store,
            title,
            start,
            end,
            calendar.as_deref(),
            notes.as_deref(),
            all_day,
            cli.output,
        ),
        Commands::Delete { ref event_id } => commands::delete::run(&store, event_id, cli.output),
        Commands::Completions { .. } => unreachable!(),
    };

    if let Err(e) = result {
        print_error(&cli, &e);
        std::process::exit(1);
    }
}

fn print_error(cli: &Cli, error: &error::AppError) {
    match cli.output {
        cli::OutputFormat::Human => eprintln!("Error: {error}"),
        cli::OutputFormat::Json => {
            let err = serde_json::json!({ "error": error.to_string() });
            eprintln!("{}", serde_json::to_string_pretty(&err).unwrap());
        }
    }
}
