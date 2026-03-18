use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

/// Parse a date/time string with natural language support.
/// Supports: YYYY-MM-DD, YYYY-MM-DD HH:MM, and natural language (EN/JP).
pub fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();

    // Standard formats
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return Some(dt);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0).unwrap());
    }

    let now = Local::now().naive_local();
    let today = now.date();

    // Try to split into date part + time part
    let (date, time) = split_date_time(s, today, now.time());
    date.map(|d| d.and_time(time))
}

/// Parse a date-only string with natural language support.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();

    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }

    let today = Local::now().date_naive();
    let (date, _) = split_date_time(s, today, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    date
}

fn split_date_time(
    s: &str,
    today: NaiveDate,
    default_time: NaiveTime,
) -> (Option<NaiveDate>, NaiveTime) {
    let s_lower = s.to_lowercase();

    // Japanese: 明日の3時, 来週月曜の9時
    if let Some(result) = parse_japanese(s, today) {
        return result;
    }

    // English natural language
    if let Some(result) = parse_english(&s_lower, today) {
        return result;
    }

    // Just a time: "3pm", "14:00"
    if let Some(t) = parse_time_str(&s_lower) {
        return (Some(today), t);
    }

    (None, default_time)
}

fn parse_japanese(s: &str, today: NaiveDate) -> Option<(Option<NaiveDate>, NaiveTime)> {
    let default_time = NaiveTime::from_hms_opt(0, 0, 0).unwrap();

    // Split on の to separate date and time: 明日の3時
    let (date_part, time_part) = if let Some(idx) = s.find('の') {
        let (d, t) = s.split_at(idx);
        (d.trim(), Some(t[3..].trim())) // 'の' is 3 bytes in UTF-8
    } else {
        (s.trim(), None)
    };

    let date = match date_part {
        "今日" => Some(today),
        "明日" => Some(today + Duration::days(1)),
        "明後日" => Some(today + Duration::days(2)),
        "昨日" => Some(today - Duration::days(1)),
        s if s.starts_with("来週") => {
            let weekday_str = &s["来週".len()..];
            parse_jp_weekday(weekday_str).map(|wd| next_weekday(today + Duration::weeks(1), wd))
        }
        s if s.starts_with("今週") => {
            let weekday_str = &s["今週".len()..];
            parse_jp_weekday(weekday_str).map(|wd| next_weekday_this_week(today, wd))
        }
        _ => None,
    };

    date.as_ref()?; // return None if date didn't match

    let time = time_part.and_then(parse_jp_time).unwrap_or(default_time);

    Some((date, time))
}

fn parse_english(s: &str, today: NaiveDate) -> Option<(Option<NaiveDate>, NaiveTime)> {
    let default_time = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let parts: Vec<&str> = s.splitn(3, ' ').collect();

    // "tomorrow 3pm", "today 14:00"
    let (date_str, time_str) = if parts.len() >= 2 {
        // Check if last part is a time
        if parse_time_str(parts.last().unwrap()).is_some() {
            let date_part = parts[..parts.len() - 1].join(" ");
            (date_part, Some(*parts.last().unwrap()))
        } else {
            (s.to_string(), None)
        }
    } else {
        (s.to_string(), None)
    };

    let date = match date_str.as_str() {
        "today" => Some(today),
        "tomorrow" => Some(today + Duration::days(1)),
        "yesterday" => Some(today - Duration::days(1)),
        s if s.starts_with("next ") => {
            let wd_str = &s[5..];
            parse_en_weekday(wd_str).map(|wd| next_weekday(today + Duration::days(1), wd))
        }
        s if s.starts_with("in ") => {
            parse_relative_days(&s[3..]).map(|d| today + Duration::days(d))
        }
        _ => None,
    };

    date.as_ref()?;

    let time = time_str.and_then(parse_time_str).unwrap_or(default_time);

    Some((date, time))
}

fn parse_time_str(s: &str) -> Option<NaiveTime> {
    let s = s.trim().to_lowercase();

    // "14:00", "9:30"
    if let Some((h, m)) = s.split_once(':') {
        let h: u32 = h.parse().ok()?;
        let m: u32 = m.parse().ok()?;
        return NaiveTime::from_hms_opt(h, m, 0);
    }

    // "3pm", "11am", "3 pm"
    let s = s.replace(' ', "");
    if s.ends_with("pm") || s.ends_with("am") {
        let is_pm = s.ends_with("pm");
        let num_str = &s[..s.len() - 2];
        let h: u32 = num_str.parse().ok()?;
        let h = if is_pm && h != 12 {
            h + 12
        } else if !is_pm && h == 12 {
            0
        } else {
            h
        };
        return NaiveTime::from_hms_opt(h, 0, 0);
    }

    None
}

fn parse_jp_time(s: &str) -> Option<NaiveTime> {
    let s = s.trim();

    // "午後3時", "午前9時"
    if s.starts_with("午後") {
        let rest = s.trim_start_matches("午後");
        let h: u32 = rest.trim_end_matches('時').parse().ok()?;
        return NaiveTime::from_hms_opt(if h != 12 { h + 12 } else { h }, 0, 0);
    }
    if s.starts_with("午前") {
        let rest = s.trim_start_matches("午前");
        let h: u32 = rest.trim_end_matches('時').parse().ok()?;
        return NaiveTime::from_hms_opt(if h == 12 { 0 } else { h }, 0, 0);
    }

    // "15時", "3時", "15時30分"
    if s.contains('時') {
        let parts: Vec<&str> = s.split('時').collect();
        let h: u32 = parts[0].parse().ok()?;
        let m: u32 = if parts.len() > 1 && !parts[1].is_empty() {
            parts[1].trim_end_matches('分').parse().unwrap_or(0)
        } else {
            0
        };
        return NaiveTime::from_hms_opt(h, m, 0);
    }

    None
}

fn parse_jp_weekday(s: &str) -> Option<Weekday> {
    let s = s.trim_end_matches("曜日").trim_end_matches("曜");
    match s {
        "月" => Some(Weekday::Mon),
        "火" => Some(Weekday::Tue),
        "水" => Some(Weekday::Wed),
        "木" => Some(Weekday::Thu),
        "金" => Some(Weekday::Fri),
        "土" => Some(Weekday::Sat),
        "日" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_en_weekday(s: &str) -> Option<Weekday> {
    match s {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" => Some(Weekday::Tue),
        "wednesday" | "wed" => Some(Weekday::Wed),
        "thursday" | "thu" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_relative_days(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2 && (parts[1] == "days" || parts[1] == "day") {
        return parts[0].parse().ok();
    }
    None
}

fn next_weekday(from: NaiveDate, target: Weekday) -> NaiveDate {
    let current = from.weekday();
    let days_ahead =
        (target.num_days_from_monday() as i64 - current.num_days_from_monday() as i64 + 7) % 7;
    let days_ahead = if days_ahead == 0 { 7 } else { days_ahead };
    from + Duration::days(days_ahead)
}

fn next_weekday_this_week(from: NaiveDate, target: Weekday) -> NaiveDate {
    let current = from.weekday();
    let diff = target.num_days_from_monday() as i64 - current.num_days_from_monday() as i64;
    from + Duration::days(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_formats() {
        assert!(parse_datetime("2026-03-20 14:00").is_some());
        assert!(parse_datetime("2026-03-20").is_some());
    }

    #[test]
    fn test_english_natural() {
        assert!(parse_datetime("tomorrow").is_some());
        assert!(parse_datetime("today 3pm").is_some());
        assert!(parse_datetime("tomorrow 14:00").is_some());
        assert!(parse_datetime("next monday").is_some());
        assert!(parse_datetime("next friday 9am").is_some());
        assert!(parse_datetime("in 3 days").is_some());
    }

    #[test]
    fn test_japanese_natural() {
        assert!(parse_datetime("明日").is_some());
        assert!(parse_datetime("明日の3時").is_some());
        assert!(parse_datetime("明日の午後3時").is_some());
        assert!(parse_datetime("来週月曜").is_some());
        assert!(parse_datetime("来週火曜の15時").is_some());
        assert!(parse_datetime("今日").is_some());
        assert!(parse_datetime("明後日の9時30分").is_some());
    }

    #[test]
    fn test_time_only() {
        assert_eq!(parse_time_str("3pm"), NaiveTime::from_hms_opt(15, 0, 0));
        assert_eq!(parse_time_str("11am"), NaiveTime::from_hms_opt(11, 0, 0));
        assert_eq!(parse_time_str("14:00"), NaiveTime::from_hms_opt(14, 0, 0));
    }

    #[test]
    fn test_jp_time() {
        assert_eq!(parse_jp_time("午後3時"), NaiveTime::from_hms_opt(15, 0, 0));
        assert_eq!(parse_jp_time("午前9時"), NaiveTime::from_hms_opt(9, 0, 0));
        assert_eq!(
            parse_jp_time("15時30分"),
            NaiveTime::from_hms_opt(15, 30, 0)
        );
    }
}
