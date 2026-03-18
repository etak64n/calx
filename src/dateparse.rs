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
/// Rejects time-only inputs like "15:30" or "3pm".
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();

    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }

    // Reject time-only input (would incorrectly resolve to today)
    // Check both "3pm" and "3 pm" forms
    let s_lower = s.to_lowercase();
    let s_nospace = s_lower.replace(' ', "");
    if parse_time_str(&s_nospace).is_some()
        && !s_lower.starts_with("today")
        && !s_lower.starts_with("tomorrow")
        && !s_lower.starts_with("yesterday")
        && !s_lower.starts_with("next ")
        && !s_lower.starts_with("in ")
    {
        // If the entire input resolves to just a time, reject it
        let has_date_keyword = s_lower.contains("today")
            || s_lower.contains("tomorrow")
            || s_lower.contains("yesterday")
            || s_lower.contains("next")
            || s_lower.contains("in ");
        if !has_date_keyword {
            return None;
        }
    }

    let today = Local::now().date_naive();
    let (date, _) = split_date_time(s, today, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    date
}

/// Parse a date-only string for all-day events.
/// Rejects any explicit time component.
pub fn parse_all_day_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if contains_explicit_time(s) {
        return None;
    }
    parse_date(s)
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

fn contains_explicit_time(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }

    if NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").is_ok() {
        return true;
    }

    let lower = s.to_lowercase();
    if parse_time_str(&lower).is_some() {
        return true;
    }

    if let Some((_, tail)) = lower.rsplit_once(' ') {
        if parse_time_str(tail).is_some() {
            return true;
        }
    }

    if let Some((_, tail)) = s.split_once('の') {
        if parse_jp_time(tail.trim()).is_some() {
            return true;
        }
    }

    if parse_jp_time(s).is_some() {
        return true;
    }

    false
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

    let time = match time_part {
        Some(tp) => parse_jp_time(tp)?,
        None => default_time,
    };

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

    // Fallback: try HH:MM and 12h formats (e.g. 明日の15:30, 明日の3pm)
    parse_time_str(s)
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
    use chrono::Timelike;

    // --- Standard formats ---

    #[test]
    fn test_standard_datetime() {
        let dt = parse_datetime("2026-03-20 14:00").unwrap();
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 0);
        assert_eq!(dt.day(), 20);
    }

    #[test]
    fn test_standard_date_only() {
        let dt = parse_datetime("2026-03-20").unwrap();
        assert_eq!(dt.hour(), 0);
        assert_eq!(dt.day(), 20);
    }

    #[test]
    fn test_invalid_format_returns_none() {
        assert!(parse_datetime("not a date").is_none());
        assert!(parse_datetime("").is_none());
        assert!(parse_datetime("2026-13-01").is_none());
    }

    // --- parse_date ---

    #[test]
    fn test_parse_date_standard() {
        let d = parse_date("2026-03-20").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 3, 20).unwrap());
    }

    #[test]
    fn test_parse_date_natural() {
        assert!(parse_date("tomorrow").is_some());
        assert!(parse_date("today").is_some());
    }

    #[test]
    fn test_parse_date_invalid() {
        assert!(parse_date("nope").is_none());
    }

    #[test]
    fn test_parse_date_rejects_time_only() {
        assert!(parse_date("15:30").is_none());
        assert!(parse_date("3pm").is_none());
        assert!(parse_date("11am").is_none());
        assert!(parse_date("3 pm").is_none());
        assert!(parse_date("11 am").is_none());
    }

    #[test]
    fn test_parse_all_day_date_accepts_date_only_inputs() {
        let today = Local::now().date_naive();
        assert_eq!(
            parse_all_day_date("2026-03-20"),
            Some(NaiveDate::from_ymd_opt(2026, 3, 20).unwrap())
        );
        assert_eq!(parse_all_day_date("today"), Some(today));
        assert_eq!(parse_all_day_date("明日"), Some(today + Duration::days(1)));
    }

    #[test]
    fn test_parse_all_day_date_rejects_time_components() {
        assert!(parse_all_day_date("2026-03-20 14:00").is_none());
        assert!(parse_all_day_date("today 3pm").is_none());
        assert!(parse_all_day_date("明日の15:30").is_none());
        assert!(parse_all_day_date("明日の午後3時").is_none());
    }

    // --- Japanese with HH:MM format ---

    #[test]
    fn test_jp_with_hhmm_time() {
        let dt = parse_datetime("明日の15:30").unwrap();
        assert_eq!(dt.hour(), 15);
        assert_eq!(dt.minute(), 30);
    }

    #[test]
    fn test_jp_with_12h_time() {
        let dt = parse_datetime("明日の3pm").unwrap();
        assert_eq!(dt.hour(), 15);
    }

    #[test]
    fn test_jp_invalid_time_returns_none() {
        // "の" is present but time part is not parseable
        assert!(parse_datetime("明日のabc").is_none());
    }

    // --- English natural language ---

    #[test]
    fn test_english_today() {
        let dt = parse_datetime("today").unwrap();
        assert_eq!(dt.date(), Local::now().date_naive());
    }

    #[test]
    fn test_english_tomorrow() {
        let dt = parse_datetime("tomorrow").unwrap();
        let expected = Local::now().date_naive() + Duration::days(1);
        assert_eq!(dt.date(), expected);
    }

    #[test]
    fn test_english_yesterday() {
        let dt = parse_datetime("yesterday").unwrap();
        let expected = Local::now().date_naive() - Duration::days(1);
        assert_eq!(dt.date(), expected);
    }

    #[test]
    fn test_english_today_with_time() {
        let dt = parse_datetime("today 3pm").unwrap();
        assert_eq!(dt.date(), Local::now().date_naive());
        assert_eq!(dt.hour(), 15);
    }

    #[test]
    fn test_english_tomorrow_with_24h_time() {
        let dt = parse_datetime("tomorrow 14:00").unwrap();
        assert_eq!(dt.hour(), 14);
    }

    #[test]
    fn test_english_next_weekday() {
        let dt = parse_datetime("next monday").unwrap();
        assert_eq!(dt.weekday(), Weekday::Mon);
        assert!(dt.date() > Local::now().date_naive());
    }

    #[test]
    fn test_english_next_weekday_with_time() {
        let dt = parse_datetime("next friday 9am").unwrap();
        assert_eq!(dt.weekday(), Weekday::Fri);
        assert_eq!(dt.hour(), 9);
    }

    #[test]
    fn test_english_in_n_days() {
        let dt = parse_datetime("in 3 days").unwrap();
        let expected = Local::now().date_naive() + Duration::days(3);
        assert_eq!(dt.date(), expected);
    }

    #[test]
    fn test_english_in_1_day() {
        let dt = parse_datetime("in 1 day").unwrap();
        let expected = Local::now().date_naive() + Duration::days(1);
        assert_eq!(dt.date(), expected);
    }

    // --- Japanese natural language ---

    #[test]
    fn test_jp_today() {
        let dt = parse_datetime("今日").unwrap();
        assert_eq!(dt.date(), Local::now().date_naive());
    }

    #[test]
    fn test_jp_tomorrow() {
        let dt = parse_datetime("明日").unwrap();
        let expected = Local::now().date_naive() + Duration::days(1);
        assert_eq!(dt.date(), expected);
    }

    #[test]
    fn test_jp_day_after_tomorrow() {
        let dt = parse_datetime("明後日").unwrap();
        let expected = Local::now().date_naive() + Duration::days(2);
        assert_eq!(dt.date(), expected);
    }

    #[test]
    fn test_jp_tomorrow_with_time() {
        let dt = parse_datetime("明日の3時").unwrap();
        assert_eq!(dt.hour(), 3);
    }

    #[test]
    fn test_jp_tomorrow_with_pm() {
        let dt = parse_datetime("明日の午後3時").unwrap();
        assert_eq!(dt.hour(), 15);
    }

    #[test]
    fn test_jp_tomorrow_with_am() {
        let dt = parse_datetime("明日の午前9時").unwrap();
        assert_eq!(dt.hour(), 9);
    }

    #[test]
    fn test_jp_next_week_weekday() {
        let dt = parse_datetime("来週月曜").unwrap();
        assert_eq!(dt.weekday(), Weekday::Mon);
    }

    #[test]
    fn test_jp_next_week_with_time() {
        let dt = parse_datetime("来週火曜の15時").unwrap();
        assert_eq!(dt.weekday(), Weekday::Tue);
        assert_eq!(dt.hour(), 15);
    }

    #[test]
    fn test_jp_time_with_minutes() {
        let dt = parse_datetime("明後日の9時30分").unwrap();
        assert_eq!(dt.hour(), 9);
        assert_eq!(dt.minute(), 30);
    }

    // --- Time parsing ---

    #[test]
    fn test_time_12h() {
        assert_eq!(parse_time_str("3pm"), NaiveTime::from_hms_opt(15, 0, 0));
        assert_eq!(parse_time_str("11am"), NaiveTime::from_hms_opt(11, 0, 0));
        assert_eq!(parse_time_str("12pm"), NaiveTime::from_hms_opt(12, 0, 0));
        assert_eq!(parse_time_str("12am"), NaiveTime::from_hms_opt(0, 0, 0));
    }

    #[test]
    fn test_time_24h() {
        assert_eq!(parse_time_str("14:00"), NaiveTime::from_hms_opt(14, 0, 0));
        assert_eq!(parse_time_str("9:30"), NaiveTime::from_hms_opt(9, 30, 0));
        assert_eq!(parse_time_str("0:00"), NaiveTime::from_hms_opt(0, 0, 0));
    }

    #[test]
    fn test_time_invalid() {
        assert!(parse_time_str("abc").is_none());
        assert!(parse_time_str("25:00").is_none());
    }

    // --- Japanese time parsing ---

    #[test]
    fn test_jp_time_gozen_gogo() {
        assert_eq!(parse_jp_time("午後3時"), NaiveTime::from_hms_opt(15, 0, 0));
        assert_eq!(parse_jp_time("午前9時"), NaiveTime::from_hms_opt(9, 0, 0));
        assert_eq!(parse_jp_time("午後12時"), NaiveTime::from_hms_opt(12, 0, 0));
        assert_eq!(parse_jp_time("午前12時"), NaiveTime::from_hms_opt(0, 0, 0));
    }

    #[test]
    fn test_jp_time_24h() {
        assert_eq!(parse_jp_time("15時"), NaiveTime::from_hms_opt(15, 0, 0));
        assert_eq!(
            parse_jp_time("15時30分"),
            NaiveTime::from_hms_opt(15, 30, 0)
        );
        assert_eq!(parse_jp_time("0時"), NaiveTime::from_hms_opt(0, 0, 0));
    }

    // --- Weekday parsing ---

    #[test]
    fn test_en_weekdays() {
        assert_eq!(parse_en_weekday("monday"), Some(Weekday::Mon));
        assert_eq!(parse_en_weekday("tue"), Some(Weekday::Tue));
        assert_eq!(parse_en_weekday("friday"), Some(Weekday::Fri));
        assert_eq!(parse_en_weekday("sun"), Some(Weekday::Sun));
        assert_eq!(parse_en_weekday("xyz"), None);
    }

    #[test]
    fn test_jp_weekdays() {
        assert_eq!(parse_jp_weekday("月"), Some(Weekday::Mon));
        assert_eq!(parse_jp_weekday("火曜"), Some(Weekday::Tue));
        assert_eq!(parse_jp_weekday("水曜日"), Some(Weekday::Wed));
        assert_eq!(parse_jp_weekday("日"), Some(Weekday::Sun));
        assert_eq!(parse_jp_weekday("xyz"), None);
    }
}
