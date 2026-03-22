use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::state::{self, UndoAction};
use crate::store::{CalendarStore, RecurrenceScope};
use serde::Serialize;

#[derive(Serialize)]
struct TemplateListItem {
    name: String,
    title: String,
    calendar: String,
    all_day: bool,
    recurring: bool,
    saved_at: chrono::DateTime<chrono::Local>,
}

#[derive(Serialize)]
struct TemplateSaveResult {
    saved: bool,
    name: String,
}

#[derive(Serialize)]
struct TemplateDeleteResult {
    deleted: bool,
    name: String,
}

#[derive(Serialize)]
struct TemplateAddResult {
    created: bool,
    event_id: String,
    title: String,
}

pub fn list(format: OutputFormat) -> Result<(), AppError> {
    let templates = state::list_templates()?;
    let items = templates
        .into_iter()
        .map(|template| TemplateListItem {
            name: template.name,
            title: template.draft.title,
            calendar: template.draft.calendar,
            all_day: template.draft.all_day,
            recurring: template.draft.recurrence_rule.is_some(),
            saved_at: template.saved_at,
        })
        .collect::<Vec<_>>();
    print_output(format, &items, |items, out| {
        if items.is_empty() {
            writeln!(out, "No templates saved.")?;
            return Ok(());
        }
        for item in items {
            let mut flags = Vec::new();
            if item.all_day {
                flags.push("all-day");
            }
            if item.recurring {
                flags.push("recurring");
            }
            let suffix = if flags.is_empty() {
                String::new()
            } else {
                format!("  {}", flags.join("  "))
            };
            writeln!(
                out,
                "{}  {}  [{}]{}",
                item.name, item.title, item.calendar, suffix
            )?;
        }
        Ok(())
    })
}

pub fn show(name: &str, format: OutputFormat) -> Result<(), AppError> {
    let template = state::get_template(name)?;
    print_output(format, &template, |template, out| {
        writeln!(out, "Template: {}", template.name)?;
        writeln!(out, "Title: {}", template.draft.title)?;
        writeln!(out, "Calendar: {}", template.draft.calendar)?;
        if template.draft.all_day {
            let end_date = if template.draft.end.time()
                == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
                && template.draft.end.date_naive() > template.draft.start.date_naive()
            {
                template.draft.end.date_naive() - chrono::Duration::days(1)
            } else {
                template.draft.end.date_naive()
            };
            if end_date > template.draft.start.date_naive() {
                writeln!(
                    out,
                    "When: {} to {} (All Day)",
                    template.draft.start.format("%Y-%m-%d"),
                    end_date.format("%Y-%m-%d")
                )?;
            } else {
                writeln!(
                    out,
                    "When: {} (All Day)",
                    template.draft.start.format("%Y-%m-%d")
                )?;
            }
        } else {
            writeln!(
                out,
                "When: {} -> {}",
                template.draft.start.format("%Y-%m-%d %H:%M"),
                template.draft.end.format("%Y-%m-%d %H:%M"),
            )?;
        }
        if let Some(location) = &template.draft.location {
            writeln!(out, "Location: {location}")?;
        }
        if let Some(url) = &template.draft.url {
            writeln!(out, "URL: {url}")?;
        }
        if let Some(notes) = &template.draft.notes {
            writeln!(out, "Notes: {notes}")?;
        }
        if !template.draft.alerts.is_empty() {
            let alerts = template
                .draft
                .alerts
                .iter()
                .map(|minutes| format!("{minutes}m"))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(out, "Alerts: {alerts}")?;
        }
        if let Some(rule) = &template.draft.recurrence_rule {
            writeln!(out, "Repeat: {}", recurrence_rule_summary(rule))?;
        }
        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
pub fn save(
    store: &CalendarStore,
    name: &str,
    force: bool,
    event_id: Option<&str>,
    query: Option<&str>,
    exact: bool,
    interactive: bool,
    in_calendar: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
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

    let event = super::select::resolve_event(
        store,
        event_id,
        query,
        exact,
        in_calendar,
        from,
        to,
        interactive,
    )?;
    let draft = store.event_draft(&event.id, event.start)?;
    state::save_template(name, draft, force)?;
    let result = TemplateSaveResult {
        saved: true,
        name: name.to_string(),
    };
    print_output(format, &result, |_, out| {
        writeln!(out, "Template saved: {name}")
    })
}

pub fn delete(name: &str, format: OutputFormat) -> Result<(), AppError> {
    state::delete_template(name)?;
    let result = TemplateDeleteResult {
        deleted: true,
        name: name.to_string(),
    };
    print_output(format, &result, |_, out| {
        writeln!(out, "Template deleted: {name}")
    })
}

#[allow(clippy::too_many_arguments)]
pub fn add(
    store: &CalendarStore,
    name: &str,
    title: Option<&str>,
    start: &str,
    end: Option<&str>,
    calendar: Option<&str>,
    drop_recurrence: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
    let template = state::get_template(name)?;
    let (start_dt, end_dt) =
        super::duplicate::resolve_instantiation_times(&template.draft, Some(start), end)?;
    state::ensure_no_pending_undo()?;
    let created = store.create_event_from_draft(
        &template.draft,
        title,
        start_dt,
        end_dt,
        calendar,
        !drop_recurrence,
    )?;
    super::save_undo_best_effort(
        UndoAction::DeleteCreated {
            event_id: created.id.clone(),
            selected_start: created.start,
            scope: created.recurring.then_some(RecurrenceScope::Future),
        },
        format,
    );

    let result = TemplateAddResult {
        created: true,
        event_id: created.id.clone(),
        title: created.title.clone(),
    };
    print_output(format, &result, |_, out| {
        writeln!(
            out,
            "Created from template: {} ({})",
            created.title, created.id
        )
    })
}

fn recurrence_rule_summary(rule: &crate::store::RecurrenceRuleInfo) -> String {
    let mut summary = if rule.interval <= 1 {
        rule.frequency.clone()
    } else {
        format!("every {} {}", rule.interval, rule.frequency)
    };

    if let Some(count) = rule.count {
        summary.push_str(&format!(" ({count} times)"));
    } else if let Some(until) = rule.until {
        summary.push_str(&format!(" until {}", until.format("%Y-%m-%d")));
    }

    summary
}
