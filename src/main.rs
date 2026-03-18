mod cli;
mod commands;
mod dateparse;
mod error;
mod output;
mod store;

use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use clap::Parser;
use cli::{Cli, Commands};
use commands::events::{DisplayOpts, validate_date_range, validate_opts};
use commands::search::resolve_search_range;
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
            ref alert,
            check_conflicts,
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
            alert,
            check_conflicts,
            cli.output,
        ),
        Commands::Free {
            ref from,
            ref to,
            ref calendar,
            duration,
            ref after,
            ref before,
            limit,
        } => commands::free::run(
            &store,
            from.clone(),
            to.clone(),
            calendar.clone(),
            duration,
            after.as_deref(),
            before.as_deref(),
            limit,
            cli.output,
            base_opts.no_color,
            base_opts.no_header,
        ),
        Commands::Update {
            ref event_id,
            ref query,
            exact,
            ref in_calendar,
            ref from,
            ref to,
            ref title,
            ref start,
            ref end,
            ref location,
            ref url,
            ref notes,
            ref calendar,
            all_day,
            scope,
        } => commands::update::run(
            &store,
            event_id.as_deref(),
            query.as_deref(),
            exact,
            in_calendar.as_deref(),
            from.as_deref(),
            to.as_deref(),
            title.as_deref(),
            start.as_deref(),
            end.as_deref(),
            location.as_deref(),
            url.as_deref(),
            notes.as_deref(),
            calendar.as_deref(),
            all_day,
            scope.map(map_scope),
            cli.output,
        ),
        Commands::Delete {
            ref event_id,
            ref query,
            exact,
            ref in_calendar,
            ref from,
            ref to,
            dry_run,
            scope,
        } => commands::delete::run(
            &store,
            event_id.as_deref(),
            query.as_deref(),
            exact,
            in_calendar.as_deref(),
            from.as_deref(),
            to.as_deref(),
            dry_run,
            scope.map(map_scope),
            cli.output,
        ),
        Commands::Show {
            ref event_id,
            ref query,
            exact,
            ref in_calendar,
            ref from,
            ref to,
        } => commands::show::run(
            &store,
            event_id.as_deref(),
            query.as_deref(),
            exact,
            in_calendar.as_deref(),
            from.as_deref(),
            to.as_deref(),
            cli.output,
            base_opts.no_color,
        ),
        Commands::Search {
            ref query,
            exact,
            ref calendar,
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
            commands::search::run(
                &store,
                query,
                exact,
                calendar.clone(),
                from.clone(),
                to.clone(),
                cli.output,
                &opts,
            )
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

    fn parse_date_opt(s: &Option<String>) -> Result<Option<NaiveDate>, AppError> {
        s.as_ref()
            .map(|s| dateparse::parse_date(s).ok_or(AppError::InvalidDate(s.clone())))
            .transpose()
    }

    fn parse_datetime_opt(s: &Option<String>) -> Result<Option<NaiveDateTime>, AppError> {
        s.as_ref()
            .map(|s| dateparse::parse_datetime(s).ok_or(AppError::InvalidDate(s.clone())))
            .transpose()
    }

    fn parse_all_day_opt(s: &Option<String>) -> Result<Option<NaiveDate>, AppError> {
        s.as_ref()
            .map(|s| dateparse::parse_all_day_date(s).ok_or(AppError::InvalidDate(s.clone())))
            .transpose()
    }

    fn validate_repeat_opt(repeat: Option<&str>) -> Result<(), AppError> {
        match repeat {
            None | Some("daily" | "weekly" | "monthly" | "yearly") => Ok(()),
            Some(freq) => Err(AppError::InvalidArgument(format!(
                "Unknown repeat frequency: {freq}. Use daily, weekly, monthly, or yearly."
            ))),
        }
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
            let today = Local::now().date_naive();
            validate_filter_opts(sort.as_deref(), after.as_deref(), before.as_deref())?;
            let from_date = parse_date_opt(from)?.unwrap_or(today);
            let to_date = parse_date_opt(to)?.unwrap_or(from_date + Duration::days(7));
            validate_date_range(from_date, to_date)?;
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
            resolve_search_range(from.as_deref(), to.as_deref())?;
        }
        Commands::Add {
            start,
            end,
            all_day,
            repeat,
            ..
        } => {
            if *all_day {
                let s = dateparse::parse_all_day_date(start)
                    .ok_or_else(|| AppError::InvalidDate(start.clone()))?;
                let e = dateparse::parse_all_day_date(end)
                    .ok_or_else(|| AppError::InvalidDate(end.clone()))?;
                if e < s {
                    return Err(AppError::InvalidDate(
                        "end date must be on or after start date".to_string(),
                    ));
                }
            } else {
                let s = dateparse::parse_datetime(start)
                    .ok_or_else(|| AppError::InvalidDate(start.clone()))?;
                let e = dateparse::parse_datetime(end)
                    .ok_or_else(|| AppError::InvalidDate(end.clone()))?;
                if e < s {
                    return Err(AppError::InvalidDate(
                        "end time must be after start time".to_string(),
                    ));
                }
            }
            validate_repeat_opt(repeat.as_deref())?;
        }
        Commands::Update {
            start,
            end,
            all_day,
            query,
            from,
            to,
            ..
        } => {
            if *all_day == Some(true) {
                let start_date = parse_all_day_opt(start)?;
                let end_date = parse_all_day_opt(end)?;
                if let (Some(s), Some(e)) = (start_date, end_date) {
                    if e < s {
                        return Err(AppError::InvalidDate(
                            "end date must be on or after start date".to_string(),
                        ));
                    }
                }
            } else {
                let start_dt = parse_datetime_opt(start)?;
                let end_dt = parse_datetime_opt(end)?;
                if let (Some(s), Some(e)) = (start_dt, end_dt) {
                    if e < s {
                        return Err(AppError::InvalidDate(
                            "end time must be after start time".to_string(),
                        ));
                    }
                }
            }
            if query.is_some() {
                resolve_search_range(from.as_deref(), to.as_deref())?;
            }
        }
        Commands::Free {
            from,
            to,
            after,
            before,
            ..
        } => {
            if let Some(s) = from {
                dateparse::parse_date(s).ok_or(AppError::InvalidDate(s.clone()))?;
            }
            if let Some(s) = to {
                dateparse::parse_date(s).ok_or(AppError::InvalidDate(s.clone()))?;
            }
            if let Some(a) = after {
                commands::free::parse_hhmm_validate(a)?;
            }
            if let Some(b) = before {
                commands::free::parse_hhmm_validate(b)?;
            }
        }
        Commands::Show {
            query, from, to, ..
        }
        | Commands::Delete {
            query, from, to, ..
        } => {
            if query.is_some() {
                resolve_search_range(from.as_deref(), to.as_deref())?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn map_scope(scope: cli::RecurrenceScope) -> store::RecurrenceScope {
    match scope {
        cli::RecurrenceScope::This => store::RecurrenceScope::This,
        cli::RecurrenceScope::Future => store::RecurrenceScope::Future,
    }
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
