use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::{CalendarInfo, CalendarStore};
use serde::Serialize;
use std::io::{self, Write};

#[derive(Serialize)]
struct DoctorReport {
    permission: String,
    default_calendar: Option<DefaultCalendarDiag>,
    timezone: String,
    calendars: Vec<CalendarDiag>,
}

#[derive(Serialize)]
struct DefaultCalendarDiag {
    id: String,
    title: String,
    source: String,
}

#[derive(Serialize)]
struct CalendarDiag {
    id: String,
    title: String,
    source: String,
    writable: bool,
}

pub fn run(format: OutputFormat, no_color: bool) -> Result<(), AppError> {
    let human = matches!(format, OutputFormat::Human);
    let stdout = io::stdout();
    let mut out = human.then(|| stdout.lock());
    let (bold, dim, green, red, reset) = if !no_color {
        ("\x1b[1m", "\x1b[2m", "\x1b[32m", "\x1b[31m", "\x1b[0m")
    } else {
        ("", "", "", "", "")
    };

    // 1. Permission check
    let store = match CalendarStore::new() {
        Ok(s) => {
            if human {
                writeln!(
                    out.as_mut().unwrap(),
                    "{green}✓{reset} Calendar access: {bold}granted{reset}"
                )
                .map_err(|e| AppError::Io(e.to_string()))?;
            }
            Some(s)
        }
        Err(e) => {
            if human {
                writeln!(
                    out.as_mut().unwrap(),
                    "{red}✗{reset} Calendar access: {bold}{e}{reset}"
                )
                .map_err(|e| AppError::Io(e.to_string()))?;
            }
            None
        }
    };

    // 2. Timezone
    let tz = chrono::Local::now().format("%Z (%:z)").to_string();
    if human {
        writeln!(
            out.as_mut().unwrap(),
            "{green}✓{reset} Timezone: {bold}{tz}{reset}"
        )
        .map_err(|e| AppError::Io(e.to_string()))?;
    }

    let Some(store) = store else {
        if !human {
            let report = DoctorReport {
                permission: "denied".to_string(),
                default_calendar: None,
                timezone: tz,
                calendars: vec![],
            };
            print_output(format, &report, |_, _| Ok(()))?;
        }
        return Ok(());
    };

    // 3. Default calendar
    let default_cal = store.default_calendar().map(calendar_ref);
    if human {
        match &default_cal {
            Some(cal) => writeln!(
                out.as_mut().unwrap(),
                "{green}✓{reset} Default calendar: {bold}{}{reset}  {dim}({})  {}{reset}",
                cal.title,
                cal.source,
                cal.id
            )
            .map_err(|e| AppError::Io(e.to_string()))?,
            None => writeln!(
                out.as_mut().unwrap(),
                "{red}✗{reset} Default calendar: {bold}not set{reset}"
            )
            .map_err(|e| AppError::Io(e.to_string()))?,
        }
    }

    // 4. Calendar list with write status
    let calendars = store.calendars();
    let diags: Vec<CalendarDiag> = calendars
        .iter()
        .map(|c| {
            let writable = store.is_calendar_writable(&c.id);
            CalendarDiag {
                id: c.id.clone(),
                title: c.title.clone(),
                source: c.source.clone(),
                writable,
            }
        })
        .collect();

    if human {
        writeln!(out.as_mut().unwrap()).map_err(|e| AppError::Io(e.to_string()))?;
        writeln!(
            out.as_mut().unwrap(),
            "{bold}Calendars ({}):{reset}",
            diags.len()
        )
        .map_err(|e| AppError::Io(e.to_string()))?;
        for d in &diags {
            let status = if d.writable {
                format!("{green}rw{reset}")
            } else {
                format!("{dim}ro{reset}")
            };
            writeln!(
                out.as_mut().unwrap(),
                "  [{status}] {bold}{}{reset}  {dim}({})  {}{reset}",
                d.title,
                d.source,
                d.id
            )
            .map_err(|e| AppError::Io(e.to_string()))?;
        }
    } else {
        let report = DoctorReport {
            permission: "granted".to_string(),
            default_calendar: default_cal,
            timezone: tz,
            calendars: diags,
        };
        print_output(format, &report, |_, _| Ok(()))?;
    }

    Ok(())
}

fn calendar_ref(cal: CalendarInfo) -> DefaultCalendarDiag {
    DefaultCalendarDiag {
        id: cal.id,
        title: cal.title,
        source: cal.source,
    }
}
