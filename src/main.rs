mod cli;
mod commands;
mod dateparse;
mod error;
mod output;
mod store;

use clap::Parser;
use cli::{Cli, Commands};
use commands::events::DisplayOpts;

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

    let base_opts = DisplayOpts {
        verbose: cli.verbose,
        fields: cli.fields.as_deref(),
        no_color: cli.no_color,
        no_header: cli.no_header,
        ..Default::default()
    };

    let result = match cli.command {
        Commands::Calendars => {
            commands::calendars::run(&store, cli.output, base_opts.no_color, base_opts.no_header)
        }
        Commands::Events {
            ref from,
            ref to,
            ref calendar,
            ref sort,
            limit,
            ref after,
            ref before,
        } => {
            let opts = DisplayOpts {
                sort: sort.as_deref(),
                limit,
                after: after.as_deref(),
                before: before.as_deref(),
                ..base_opts
            };
            commands::events::run(
                &store,
                from.clone(),
                to.clone(),
                calendar.clone(),
                cli.output,
                &opts,
            )
        }
        Commands::Today {
            ref calendar,
            ref sort,
            limit,
            ref after,
            ref before,
        } => {
            let opts = DisplayOpts {
                sort: sort.as_deref(),
                limit,
                after: after.as_deref(),
                before: before.as_deref(),
                ..base_opts
            };
            commands::today::run(&store, calendar.clone(), cli.output, &opts)
        }
        Commands::Upcoming {
            days,
            ref calendar,
            ref sort,
            limit,
            ref after,
            ref before,
        } => {
            let opts = DisplayOpts {
                sort: sort.as_deref(),
                limit,
                after: after.as_deref(),
                before: before.as_deref(),
                ..base_opts
            };
            commands::upcoming::run(&store, days, calendar.clone(), cli.output, &opts)
        }
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
            repeat_interval,
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
            repeat_interval,
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
        Commands::Show { ref event_id } => {
            commands::show::run(&store, event_id, cli.output, &base_opts)
        }
        Commands::Search {
            ref query,
            ref from,
            ref to,
            ref sort,
            limit,
            ref after,
            ref before,
        } => {
            let opts = DisplayOpts {
                sort: sort.as_deref(),
                limit,
                after: after.as_deref(),
                before: before.as_deref(),
                ..base_opts
            };
            commands::search::run(&store, query, from.clone(), to.clone(), cli.output, &opts)
        }
        Commands::Next { ref calendar } => {
            commands::next::run(&store, calendar.clone(), cli.output, &base_opts)
        }
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
