mod cli;
mod commands;
mod dateparse;
mod error;
mod output;
mod store;

use clap::Parser;
use cli::{Cli, Commands};
use commands::events::{DisplayOpts, validate_opts};
use error::AppError;

fn main() {
    let cli = Cli::parse();

    if let Commands::Completions { shell } = cli.command {
        commands::completions::run(shell);
        return;
    }

    // Pre-validate inputs before requesting calendar access
    if let Err(e) = pre_validate(&cli) {
        print_error(&cli, &e);
        std::process::exit(e.exit_code());
    }

    let store = match store::CalendarStore::new() {
        Ok(s) => s,
        Err(e) => {
            print_error(&cli, &e);
            std::process::exit(e.exit_code());
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
        Commands::Calendars => commands::calendars::run(&store, cli.output, &base_opts),
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
        Commands::Delete {
            ref event_id,
            dry_run,
        } => commands::delete::run(&store, event_id, dry_run, cli.output),
        Commands::Show { ref event_id } => {
            commands::show::run(&store, event_id, cli.output, base_opts.no_color)
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
        std::process::exit(e.exit_code());
    }
}

/// Validate inputs that don't require calendar access.
/// This runs before CalendarStore::new() so validation errors
/// are reported even when calendar permission is denied.
fn pre_validate(cli: &Cli) -> Result<(), AppError> {
    fn validate_filter_opts(
        sort: Option<&str>,
        after: Option<&str>,
        before: Option<&str>,
    ) -> Result<(), AppError> {
        validate_opts(&DisplayOpts {
            sort,
            after,
            before,
            ..Default::default()
        })
    }

    fn validate_date_opt(s: &Option<String>) -> Result<(), AppError> {
        if let Some(s) = s {
            dateparse::parse_date(s).ok_or(AppError::InvalidDate(s.clone()))?;
        }
        Ok(())
    }

    match &cli.command {
        Commands::Events {
            from,
            to,
            sort,
            after,
            before,
            ..
        } => {
            validate_filter_opts(sort.as_deref(), after.as_deref(), before.as_deref())?;
            validate_date_opt(from)?;
            validate_date_opt(to)?;
        }
        Commands::Today {
            sort,
            after,
            before,
            ..
        }
        | Commands::Upcoming {
            sort,
            after,
            before,
            ..
        } => {
            validate_filter_opts(sort.as_deref(), after.as_deref(), before.as_deref())?;
        }
        Commands::Search {
            from,
            to,
            sort,
            after,
            before,
            ..
        } => {
            validate_filter_opts(sort.as_deref(), after.as_deref(), before.as_deref())?;
            validate_date_opt(from)?;
            validate_date_opt(to)?;
        }
        Commands::Add { start, end, .. } => {
            let s = dateparse::parse_datetime(start)
                .ok_or_else(|| AppError::InvalidDate(start.clone()))?;
            let e =
                dateparse::parse_datetime(end).ok_or_else(|| AppError::InvalidDate(end.clone()))?;
            if e < s {
                return Err(AppError::InvalidDate(
                    "end time must be after start time".to_string(),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn print_error(cli: &Cli, error: &AppError) {
    match cli.output {
        cli::OutputFormat::Human => eprintln!("Error: {error}"),
        _ => {
            let err = serde_json::json!({ "error": error.to_string() });
            if let Ok(s) = serde_json::to_string_pretty(&err) {
                eprintln!("{s}");
            } else {
                eprintln!("Error: {error}");
            }
        }
    }
}
