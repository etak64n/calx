mod cli;
mod commands;
mod dateparse;
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

    // Import validates input before requesting calendar access
    if let Commands::Import { ref file } = cli.command {
        let result = commands::import_cmd::run(file, cli.output);
        if let Err(e) = result {
            print_error(&cli, &e);
            std::process::exit(1);
        }
        return;
    }

    let store = match store::CalendarStore::new() {
        Ok(s) => s,
        Err(e) => {
            print_error(&cli, &e);
            std::process::exit(1);
        }
    };

    let verbose = cli.verbose;
    let fields = cli.fields.as_deref();
    let no_color = cli.no_color;
    let no_header = cli.no_header;

    let result = match cli.command {
        Commands::Calendars => commands::calendars::run(&store, cli.output, no_color, no_header),
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
            verbose,
            fields,
            no_color,
            no_header,
        ),
        Commands::Today { ref calendar } => commands::today::run(
            &store,
            calendar.clone(),
            cli.output,
            verbose,
            fields,
            no_color,
            no_header,
        ),
        Commands::Upcoming { days, ref calendar } => commands::upcoming::run(
            &store,
            days,
            calendar.clone(),
            cli.output,
            verbose,
            fields,
            no_color,
            no_header,
        ),
        Commands::Add {
            ref title,
            ref start,
            ref end,
            ref calendar,
            ref location,
            ref url,
            ref notes,
            all_day,
            ref repeat,
            repeat_count,
        } => commands::add::run(
            &store,
            title,
            start,
            end,
            calendar.as_deref(),
            location.as_deref(),
            url.as_deref(),
            notes.as_deref(),
            all_day,
            repeat.as_deref(),
            repeat_count,
            cli.output,
        ),
        Commands::Update {
            ref event_id,
            ref title,
            ref start,
            ref end,
            ref location,
            ref url,
            ref notes,
            ref calendar,
            all_day,
        } => commands::update::run(
            &store,
            event_id,
            title.as_deref(),
            start.as_deref(),
            end.as_deref(),
            location.as_deref(),
            url.as_deref(),
            notes.as_deref(),
            calendar.as_deref(),
            all_day,
            cli.output,
        ),
        Commands::Delete { ref event_id } => commands::delete::run(&store, event_id, cli.output),
        Commands::Show { ref event_id } => commands::show::run(
            &store, event_id, cli.output, verbose, fields, no_color, no_header,
        ),
        Commands::Search {
            ref query,
            ref from,
            ref to,
        } => commands::search::run(
            &store,
            query,
            from.clone(),
            to.clone(),
            cli.output,
            verbose,
            fields,
            no_color,
            no_header,
        ),
        Commands::Next { ref calendar } => commands::next::run(
            &store,
            calendar.clone(),
            cli.output,
            verbose,
            fields,
            no_color,
            no_header,
        ),
        Commands::Import { .. } => unreachable!(),
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
        _ => {
            let err = serde_json::json!({ "error": error.to_string() });
            eprintln!("{}", serde_json::to_string_pretty(&err).unwrap());
        }
    }
}
