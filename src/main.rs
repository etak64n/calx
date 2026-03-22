mod cli;
mod commands;
mod dateparse;
mod error;
mod output;
mod state;
mod store;

use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use clap::Parser;
use cli::{Cli, Commands, TemplateCommand};
use commands::events::{DisplayOpts, validate_date_range, validate_opts};
use commands::search::resolve_search_range;
use error::AppError;
use serde::Serialize;
use serde_json::json;
use std::io;

fn main() {
    let cli = Cli::parse();
    let output = cli.output.resolve_for_stdout();

    // Pre-validate inputs before requesting calendar access
    if let Err(e) = pre_validate(&cli) {
        print_error(&cli, &e);
        std::process::exit(e.exit_code());
    }

    if let Commands::Completions { shell } = cli.command {
        commands::completions::run(shell);
        return;
    }

    if matches!(cli.command, Commands::Doctor) {
        let result = commands::doctor::run(output, cli.no_color);
        if let Err(e) = result {
            print_error(&cli, &e);
            std::process::exit(e.exit_code());
        }
        return;
    }

    if let Commands::Template { ref command } = cli.command {
        let result = match command {
            TemplateCommand::List => commands::template::list(output),
            TemplateCommand::Show { name } => commands::template::show(name, output),
            TemplateCommand::Delete { name } => commands::template::delete(name, output),
            TemplateCommand::Save { .. } | TemplateCommand::Add { .. } => Ok(()),
        };
        if let Err(e) = result {
            print_error(&cli, &e);
            std::process::exit(e.exit_code());
        }
        if matches!(
            command,
            TemplateCommand::List | TemplateCommand::Show { .. } | TemplateCommand::Delete { .. }
        ) {
            return;
        }
    }

    let mut claimed_undo = if matches!(cli.command, Commands::Undo) {
        match state::take_undo() {
            Ok(Some(record)) => {
                if let state::UndoAction::Unavailable { reason } = record.action.clone() {
                    restore_undo_record_best_effort(record, output);
                    let error = AppError::InvalidArgument(format!(
                        "Undo unavailable for the last action: {reason}"
                    ));
                    print_error(&cli, &error);
                    std::process::exit(error.exit_code());
                }
                Some(record)
            }
            Ok(None) => {
                let error = AppError::InvalidArgument("No undoable action recorded.".to_string());
                print_error(&cli, &error);
                std::process::exit(error.exit_code());
            }
            Err(e) => {
                print_error(&cli, &e);
                std::process::exit(e.exit_code());
            }
        }
    } else {
        None
    };

    let store = match store::CalendarStore::new() {
        Ok(s) => s,
        Err(e) => {
            if let Some(record) = claimed_undo.take() {
                restore_undo_record_best_effort(record, output);
            }
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
        Commands::Calendars => commands::calendars::run(&store, output, &base_opts),
        Commands::Agenda { ref calendar } => {
            commands::agenda::run(&store, calendar.clone(), output, base_opts.no_color)
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
                output,
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
            commands::today::run(&store, calendar.clone(), output, &opts)
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
            commands::upcoming::run(&store, days, calendar.clone(), output, &opts)
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
            output,
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
            output,
            base_opts.no_color,
            base_opts.no_header,
        ),
        Commands::Conflicts {
            ref start,
            ref end,
            all_day,
            ref calendar,
            ref sort,
            limit,
        } => {
            let opts = DisplayOpts {
                sort: sort.as_deref(),
                limit,
                ..base_opts
            };
            commands::conflicts::run(
                &store,
                start,
                end,
                calendar.as_deref(),
                all_day,
                output,
                &opts,
            )
        }
        Commands::Update {
            ref event_id,
            ref query,
            exact,
            interactive,
            ref in_calendar,
            ref from,
            ref to,
            ref title,
            ref start,
            ref end,
            ref location,
            clear_location,
            ref url,
            clear_url,
            ref notes,
            clear_notes,
            ref alert,
            clear_alerts,
            ref calendar,
            all_day,
            scope,
        } => commands::update::run(
            &store,
            event_id.as_deref(),
            query.as_deref(),
            exact,
            interactive,
            in_calendar.as_deref(),
            from.as_deref(),
            to.as_deref(),
            title.as_deref(),
            start.as_deref(),
            end.as_deref(),
            location.as_deref(),
            clear_location,
            url.as_deref(),
            clear_url,
            notes.as_deref(),
            clear_notes,
            alert,
            clear_alerts,
            calendar.as_deref(),
            all_day,
            scope.map(map_scope),
            output,
        ),
        Commands::Delete {
            ref event_id,
            ref query,
            exact,
            interactive,
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
            interactive,
            in_calendar.as_deref(),
            from.as_deref(),
            to.as_deref(),
            dry_run,
            scope.map(map_scope),
            output,
        ),
        Commands::Show {
            ref event_id,
            ref query,
            exact,
            interactive,
            ref in_calendar,
            ref from,
            ref to,
        } => commands::show::run(
            &store,
            event_id.as_deref(),
            query.as_deref(),
            exact,
            interactive,
            in_calendar.as_deref(),
            from.as_deref(),
            to.as_deref(),
            output,
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
                output,
                &opts,
            )
        }
        Commands::Next { ref calendar } => {
            commands::next::run(&store, calendar.clone(), output, &base_opts)
        }
        Commands::Duplicate {
            ref event_id,
            ref query,
            exact,
            interactive,
            ref in_calendar,
            ref from,
            ref to,
            ref title,
            ref start,
            ref end,
            ref calendar,
            keep_recurrence,
        } => commands::duplicate::run(
            &store,
            event_id.as_deref(),
            query.as_deref(),
            exact,
            interactive,
            in_calendar.as_deref(),
            from.as_deref(),
            to.as_deref(),
            title.as_deref(),
            start.as_deref(),
            end.as_deref(),
            calendar.as_deref(),
            keep_recurrence,
            output,
        ),
        Commands::Template { ref command } => match command {
            TemplateCommand::Save {
                name,
                force,
                event_id,
                query,
                exact,
                interactive,
                in_calendar,
                from,
                to,
            } => commands::template::save(
                &store,
                name,
                *force,
                event_id.as_deref(),
                query.as_deref(),
                *exact,
                *interactive,
                in_calendar.as_deref(),
                from.as_deref(),
                to.as_deref(),
                output,
            ),
            TemplateCommand::Add {
                name,
                title,
                start,
                end,
                calendar,
                drop_recurrence,
            } => commands::template::add(
                &store,
                name,
                title.as_deref(),
                start,
                end.as_deref(),
                calendar.as_deref(),
                *drop_recurrence,
                output,
            ),
            TemplateCommand::List
            | TemplateCommand::Show { .. }
            | TemplateCommand::Delete { .. } => unreachable!(),
        },
        Commands::Undo => commands::undo::run(
            &store,
            output,
            claimed_undo
                .take()
                .expect("claimed undo record must exist for undo command"),
        ),
        Commands::Doctor => unreachable!(),
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

    fn validate_repeat_args(
        repeat: Option<&str>,
        repeat_count: Option<u32>,
        repeat_interval: Option<u32>,
    ) -> Result<(), AppError> {
        validate_repeat_opt(repeat)?;

        if repeat.is_none() {
            if repeat_count.is_some() {
                return Err(AppError::InvalidArgument(
                    "--repeat-count requires --repeat".to_string(),
                ));
            }
            if repeat_interval.is_some() {
                return Err(AppError::InvalidArgument(
                    "--repeat-interval requires --repeat".to_string(),
                ));
            }
            return Ok(());
        }

        if repeat_count == Some(0) {
            return Err(AppError::InvalidArgument(
                "--repeat-count must be greater than 0".to_string(),
            ));
        }
        if repeat_interval == Some(0) {
            return Err(AppError::InvalidArgument(
                "--repeat-interval must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_alerts(alerts: &[i64]) -> Result<(), AppError> {
        if let Some(minutes) = alerts.iter().find(|&&minutes| minutes < 0) {
            return Err(AppError::InvalidArgument(format!(
                "--alert expects minutes before the event; got {minutes}"
            )));
        }
        Ok(())
    }

    fn validate_url_opt(url: Option<&str>) -> Result<(), AppError> {
        if let Some(url) = url {
            store::validate_url_string(url)?;
        }
        Ok(())
    }

    fn validate_non_empty(value: &str, label: &str) -> Result<(), AppError> {
        if value.trim().is_empty() {
            return Err(AppError::InvalidArgument(format!(
                "{label} must not be empty"
            )));
        }
        Ok(())
    }

    fn validate_optional_non_empty(value: Option<&str>, label: &str) -> Result<(), AppError> {
        if let Some(value) = value {
            validate_non_empty(value, label)?;
        }
        Ok(())
    }

    fn validate_flexible_datetime(value: &str) -> Result<(), AppError> {
        if dateparse::parse_datetime(value).is_some()
            || dateparse::parse_all_day_date(value).is_some()
        {
            Ok(())
        } else {
            Err(AppError::InvalidDate(value.to_string()))
        }
    }

    fn validate_event_id_opt(event_id: &Option<String>) -> Result<(), AppError> {
        if let Some(event_id) = event_id {
            validate_non_empty(event_id, "EVENT_ID")?;
        }
        Ok(())
    }

    fn validate_fields(cli: &Cli) -> Result<(), AppError> {
        let Some(fields) = cli.fields.as_deref() else {
            return Ok(());
        };

        if matches!(cli.output.resolve_for_stdout(), cli::OutputFormat::Human) {
            return Err(AppError::InvalidArgument(
                "--fields requires a structured output format".to_string(),
            ));
        }

        if !matches!(
            cli.command,
            Commands::Events { .. }
                | Commands::Today { .. }
                | Commands::Upcoming { .. }
                | Commands::Search { .. }
                | Commands::Next { .. }
                | Commands::Conflicts { .. }
        ) {
            return Err(AppError::InvalidArgument(
                "--fields is only supported for events, today, upcoming, search, next, and conflicts"
                    .to_string(),
            ));
        }

        commands::events::validate_field_list(fields)?;

        Ok(())
    }

    validate_fields(cli)?;

    match &cli.command {
        Commands::Agenda { calendar } => {
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
        }
        Commands::Events {
            from,
            to,
            calendar,
            sort,
            after,
            before,
            ..
        } => {
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
            let today = Local::now().date_naive();
            validate_filter_opts(sort.as_deref(), after.as_deref(), before.as_deref())?;
            let from_date = parse_date_opt(from)?.unwrap_or(today);
            let to_date = parse_date_opt(to)?.unwrap_or(from_date + Duration::days(7));
            validate_date_range(from_date, to_date)?;
        }
        Commands::Today {
            calendar,
            sort,
            after,
            before,
            ..
        }
        | Commands::Upcoming {
            calendar,
            sort,
            after,
            before,
            ..
        } => {
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
            validate_filter_opts(sort.as_deref(), after.as_deref(), before.as_deref())?;
        }
        Commands::Search {
            query,
            calendar,
            from,
            to,
            sort,
            after,
            before,
            ..
        } => {
            validate_non_empty(query, "query")?;
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
            validate_filter_opts(sort.as_deref(), after.as_deref(), before.as_deref())?;
            resolve_search_range(from.as_deref(), to.as_deref())?;
        }
        Commands::Add {
            title,
            start,
            end,
            calendar,
            all_day,
            repeat,
            repeat_count,
            repeat_interval,
            alert,
            url,
            ..
        } => {
            state::ensure_no_pending_undo()?;
            validate_non_empty(title, "--title")?;
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
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
                if e <= s {
                    return Err(AppError::InvalidDate(
                        "end time must be after start time".to_string(),
                    ));
                }
            }
            validate_repeat_args(repeat.as_deref(), *repeat_count, *repeat_interval)?;
            validate_alerts(alert)?;
            validate_url_opt(url.as_deref())?;
        }
        Commands::Update {
            event_id,
            title,
            start,
            end,
            all_day,
            query,
            in_calendar,
            from,
            to,
            location,
            clear_location,
            url,
            clear_url,
            notes,
            clear_notes,
            alert,
            clear_alerts,
            calendar,
            ..
        } => {
            state::ensure_no_pending_undo()?;
            validate_event_id_opt(event_id)?;
            validate_optional_non_empty(in_calendar.as_deref(), "--in-calendar")?;
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
            if let Some(title) = title {
                validate_non_empty(title, "--title")?;
            }
            if let Some(query) = query {
                validate_non_empty(query, "--query")?;
            }
            if !commands::update::has_requested_changes(
                title.as_deref(),
                start.as_deref(),
                end.as_deref(),
                location.as_deref(),
                *clear_location,
                url.as_deref(),
                *clear_url,
                notes.as_deref(),
                *clear_notes,
                !alert.is_empty(),
                *clear_alerts,
                calendar.as_deref(),
                *all_day,
            ) {
                return Err(AppError::InvalidArgument(
                    "No changes specified for update.".to_string(),
                ));
            }
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
                    if e <= s {
                        return Err(AppError::InvalidDate(
                            "end time must be after start time".to_string(),
                        ));
                    }
                }
            }
            if query.is_some() {
                resolve_search_range(from.as_deref(), to.as_deref())?;
            }
            if !clear_alerts {
                validate_alerts(alert)?;
            }
            if !clear_url {
                validate_url_opt(url.as_deref())?;
            }
        }
        Commands::Free {
            from,
            to,
            calendar,
            after,
            before,
            ..
        } => {
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
            let today = Local::now().date_naive();
            let from_date = match from {
                Some(s) => dateparse::parse_date(s).ok_or(AppError::InvalidDate(s.clone()))?,
                None => today,
            };
            let to_date = match to {
                Some(s) => dateparse::parse_date(s).ok_or(AppError::InvalidDate(s.clone()))?,
                None => from_date + Duration::days(5),
            };
            commands::free::validate_range(from_date, to_date)?;

            let after_time = after
                .as_ref()
                .map(|a| commands::free::parse_hhmm_validate(a))
                .transpose()?;
            let before_time = before
                .as_ref()
                .map(|b| commands::free::parse_hhmm_validate(b))
                .transpose()?;
            commands::free::validate_time_window(after_time, before_time)?;
        }
        Commands::Conflicts {
            start,
            end,
            all_day,
            calendar,
            sort,
            ..
        } => {
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
            validate_filter_opts(sort.as_deref(), None, None)?;
            if *all_day {
                let start_date = dateparse::parse_all_day_date(start)
                    .ok_or_else(|| AppError::InvalidDate(start.clone()))?;
                let end_date = dateparse::parse_all_day_date(end)
                    .ok_or_else(|| AppError::InvalidDate(end.clone()))?;
                validate_date_range(start_date, end_date)?;
            } else {
                let start_dt = dateparse::parse_datetime(start)
                    .ok_or_else(|| AppError::InvalidDate(start.clone()))?;
                let end_dt = dateparse::parse_datetime(end)
                    .ok_or_else(|| AppError::InvalidDate(end.clone()))?;
                if end_dt <= start_dt {
                    return Err(AppError::InvalidDate(
                        "end time must be after start time".to_string(),
                    ));
                }
            }
        }
        Commands::Show {
            event_id,
            query,
            in_calendar,
            from,
            to,
            ..
        } => {
            validate_event_id_opt(event_id)?;
            validate_optional_non_empty(in_calendar.as_deref(), "--in-calendar")?;
            if query.is_some() {
                validate_non_empty(query.as_deref().unwrap_or_default(), "--query")?;
                resolve_search_range(from.as_deref(), to.as_deref())?;
            }
        }
        Commands::Delete {
            event_id,
            query,
            in_calendar,
            from,
            to,
            dry_run,
            ..
        } => {
            if !dry_run {
                state::ensure_no_pending_undo()?;
            }
            validate_event_id_opt(event_id)?;
            validate_optional_non_empty(in_calendar.as_deref(), "--in-calendar")?;
            if query.is_some() {
                validate_non_empty(query.as_deref().unwrap_or_default(), "--query")?;
                resolve_search_range(from.as_deref(), to.as_deref())?;
            }
        }
        Commands::Next { calendar } => {
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
        }
        Commands::Duplicate {
            event_id,
            query,
            in_calendar,
            from,
            to,
            title,
            start,
            end,
            calendar,
            ..
        } => {
            state::ensure_no_pending_undo()?;
            validate_event_id_opt(event_id)?;
            validate_optional_non_empty(in_calendar.as_deref(), "--in-calendar")?;
            validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
            if let Some(query) = query {
                validate_non_empty(query, "--query")?;
                resolve_search_range(from.as_deref(), to.as_deref())?;
            }
            if let Some(title) = title {
                validate_non_empty(title, "--title")?;
            }
            if let Some(start) = start {
                validate_flexible_datetime(start)?;
            }
            if let Some(end) = end {
                validate_flexible_datetime(end)?;
            }
        }
        Commands::Template { command } => match command {
            TemplateCommand::List => {}
            TemplateCommand::Show { name } | TemplateCommand::Delete { name } => {
                validate_non_empty(name, "template name")?;
            }
            TemplateCommand::Save {
                name,
                force,
                event_id,
                query,
                in_calendar,
                from,
                to,
                ..
            } => {
                validate_non_empty(name, "template name")?;
                if !force {
                    match state::get_template(name) {
                        Ok(_) => {
                            return Err(AppError::InvalidArgument(format!(
                                "Template '{name}' already exists. Use --force to overwrite."
                            )));
                        }
                        Err(AppError::TemplateNotFound(_)) => {}
                        Err(err) => return Err(err),
                    }
                }
                validate_event_id_opt(event_id)?;
                validate_optional_non_empty(in_calendar.as_deref(), "--in-calendar")?;
                if let Some(query) = query {
                    validate_non_empty(query, "--query")?;
                    resolve_search_range(from.as_deref(), to.as_deref())?;
                }
            }
            TemplateCommand::Add {
                name,
                title,
                start,
                end,
                calendar,
                ..
            } => {
                state::ensure_no_pending_undo()?;
                validate_non_empty(name, "template name")?;
                validate_optional_non_empty(title.as_deref(), "--title")?;
                validate_optional_non_empty(calendar.as_deref(), "--calendar")?;
                validate_non_empty(start, "--start")?;
                let template = state::get_template(name)?;
                commands::duplicate::resolve_instantiation_times(
                    &template.draft,
                    Some(start),
                    end.as_deref(),
                )?;
            }
        },
        Commands::Undo => {}
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
    let output = cli.output.resolve_for_stdout();
    let report = ErrorReport {
        code: error.exit_code(),
        error: error.title().to_string(),
        why: error.why(),
        hint: error.hint(&help_hint(cli)),
    };

    match output {
        cli::OutputFormat::Human => {
            eprintln!("Error: {}", report.error);
            eprintln!("  Why: {}", report.why);
            if let Some(hint) = &report.hint {
                eprintln!("  Hint: {hint}");
            }
        }
        _ => {
            let stderr = io::stderr();
            let mut out = stderr.lock();
            if output::write_structured_output_to(output, &report, &mut out).is_err() {
                eprintln!("Error: {}", report.error);
                eprintln!("  Why: {}", report.why);
                if let Some(hint) = &report.hint {
                    eprintln!("  Hint: {hint}");
                }
            }
        }
    }
}

fn restore_undo_record_best_effort(record: state::UndoRecord, output: cli::OutputFormat) {
    if let Err(err) = state::restore_undo_record(record) {
        let warning = format!("Undo history could not be restored: {err}");
        commands::emit_warning(
            output,
            &warning,
            &json!({
                "warning": warning,
                "undo_restored": false
            }),
        );
    }
}

#[derive(Serialize)]
struct ErrorReport {
    code: i32,
    error: String,
    why: String,
    hint: Option<String>,
}

fn help_hint(cli: &Cli) -> String {
    format!(
        "Run `{}` for usage and examples.",
        command_help_path(&cli.command)
    )
}

fn command_help_path(command: &Commands) -> String {
    match command {
        Commands::Calendars => "calx calendars --help".to_string(),
        Commands::Agenda { .. } => "calx agenda --help".to_string(),
        Commands::Events { .. } => "calx events --help".to_string(),
        Commands::Today { .. } => "calx today --help".to_string(),
        Commands::Upcoming { .. } => "calx upcoming --help".to_string(),
        Commands::Add { .. } => "calx add --help".to_string(),
        Commands::Free { .. } => "calx free --help".to_string(),
        Commands::Conflicts { .. } => "calx conflicts --help".to_string(),
        Commands::Update { .. } => "calx update --help".to_string(),
        Commands::Delete { .. } => "calx delete --help".to_string(),
        Commands::Show { .. } => "calx show --help".to_string(),
        Commands::Search { .. } => "calx search --help".to_string(),
        Commands::Next { .. } => "calx next --help".to_string(),
        Commands::Doctor => "calx doctor --help".to_string(),
        Commands::Duplicate { .. } => "calx duplicate --help".to_string(),
        Commands::Template { command } => match command {
            TemplateCommand::List => "calx template list --help".to_string(),
            TemplateCommand::Show { .. } => "calx template show --help".to_string(),
            TemplateCommand::Save { .. } => "calx template save --help".to_string(),
            TemplateCommand::Delete { .. } => "calx template delete --help".to_string(),
            TemplateCommand::Add { .. } => "calx template add --help".to_string(),
        },
        Commands::Undo => "calx undo --help".to_string(),
        Commands::Completions { .. } => "calx completions --help".to_string(),
    }
}
