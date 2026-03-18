use crate::cli::OutputFormat;
use crate::error::AppError;
use crate::output::print_output;
use crate::store::CalendarStore;
use serde::Serialize;

#[derive(Serialize)]
struct DoctorReport {
    permission: String,
    default_calendar: Option<String>,
    timezone: String,
    calendars: Vec<CalendarDiag>,
}

#[derive(Serialize)]
struct CalendarDiag {
    title: String,
    source: String,
    writable: bool,
}

pub fn run(format: OutputFormat, no_color: bool) -> Result<(), AppError> {
    let (bold, dim, green, red, reset) = if !no_color {
        ("\x1b[1m", "\x1b[2m", "\x1b[32m", "\x1b[31m", "\x1b[0m")
    } else {
        ("", "", "", "", "")
    };

    // 1. Permission check
    let store = match CalendarStore::new() {
        Ok(s) => {
            if matches!(format, OutputFormat::Human) {
                println!("{green}✓{reset} Calendar access: {bold}granted{reset}");
            }
            Some(s)
        }
        Err(e) => {
            if matches!(format, OutputFormat::Human) {
                println!("{red}✗{reset} Calendar access: {bold}{e}{reset}");
            }
            None
        }
    };

    // 2. Timezone
    let tz = chrono::Local::now().format("%Z (%:z)").to_string();
    if matches!(format, OutputFormat::Human) {
        println!("{green}✓{reset} Timezone: {bold}{tz}{reset}");
    }

    let Some(store) = store else {
        if !matches!(format, OutputFormat::Human) {
            let report = DoctorReport {
                permission: "denied".to_string(),
                default_calendar: None,
                timezone: tz,
                calendars: vec![],
            };
            print_output(format, &report, |_| {});
        }
        return Ok(());
    };

    // 3. Default calendar
    let default_cal = store.default_calendar_name();
    if matches!(format, OutputFormat::Human) {
        match &default_cal {
            Some(name) => println!("{green}✓{reset} Default calendar: {bold}{name}{reset}"),
            None => println!("{red}✗{reset} Default calendar: {bold}not set{reset}"),
        }
    }

    // 4. Calendar list with write status
    let calendars = store.calendars();
    let diags: Vec<CalendarDiag> = calendars
        .iter()
        .map(|c| {
            let writable = store.is_calendar_writable(&c.title);
            CalendarDiag {
                title: c.title.clone(),
                source: c.source.clone(),
                writable,
            }
        })
        .collect();

    if matches!(format, OutputFormat::Human) {
        println!();
        println!("{bold}Calendars ({}):{reset}", diags.len());
        for d in &diags {
            let status = if d.writable {
                format!("{green}rw{reset}")
            } else {
                format!("{dim}ro{reset}")
            };
            println!(
                "  [{status}] {bold}{}{reset}  {dim}({}){reset}",
                d.title, d.source
            );
        }
    } else {
        let report = DoctorReport {
            permission: "granted".to_string(),
            default_calendar: default_cal,
            timezone: tz,
            calendars: diags,
        };
        print_output(format, &report, |_| {});
    }

    Ok(())
}
