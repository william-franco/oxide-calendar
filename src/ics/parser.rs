use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

use super::{IcsError, IcsEvent, ImportReport, unescape_text};
use crate::domain::event::{Event, new_uid};
use crate::domain::recurrence::RecurrenceRule;

/// A logical content line (after unfolding) with the physical line number where it starts.
struct Line {
    number: usize,
    text: String,
}

fn unfold(input: &str) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for (index, raw) in input.lines().enumerate() {
        let raw = raw.trim_end_matches('\r');
        if let Some(rest) = raw.strip_prefix(' ').or_else(|| raw.strip_prefix('\t'))
            && let Some(last) = lines.last_mut()
        {
            last.text.push_str(rest);
            continue;
        }
        lines.push(Line { number: index + 1, text: raw.to_string() });
    }
    lines
}

#[derive(Default)]
struct Draft {
    uid: Option<String>,
    summary: String,
    description: String,
    location: String,
    category: String,
    start: Option<(NaiveDateTime, bool)>,
    end: Option<(NaiveDateTime, bool)>,
    rrule: Option<RecurrenceRule>,
    exdates: Vec<NaiveDateTime>,
    reminder: Option<i64>,
    invalid: bool,
}

/// Parses an iCalendar document. Structural problems fail the whole file with a line number;
/// invalid individual events are skipped and counted in `ImportReport::skipped`.
pub fn parse(input: &str) -> Result<ImportReport, IcsError> {
    let mut events = Vec::new();
    let mut skipped = 0;
    let mut current: Option<(usize, Draft)> = None;
    let mut in_alarm = false;

    for line in unfold(input) {
        let Some((name, params, value)) = split_property(&line.text) else {
            if line.text.trim().is_empty() {
                continue;
            }
            return Err(IcsError { line: line.number, message: "linha sem ':'".into() });
        };
        let upper = name.to_ascii_uppercase();

        match (upper.as_str(), value) {
            ("BEGIN", "VEVENT") => {
                if current.is_some() {
                    return Err(IcsError { line: line.number, message: "VEVENT dentro de outro VEVENT".into() });
                }
                current = Some((line.number, Draft::default()));
                continue;
            }
            ("END", "VEVENT") => {
                let Some((_, draft)) = current.take() else {
                    return Err(IcsError { line: line.number, message: "END:VEVENT sem BEGIN:VEVENT".into() });
                };
                match finish(draft) {
                    Some(item) => events.push(item),
                    None => skipped += 1,
                }
                continue;
            }
            ("BEGIN", "VALARM") => {
                in_alarm = true;
                continue;
            }
            ("END", "VALARM") => {
                in_alarm = false;
                continue;
            }
            _ => {}
        }

        let Some((_, draft)) = current.as_mut() else {
            continue;
        };
        if in_alarm {
            if upper == "TRIGGER" {
                draft.reminder = parse_trigger(value);
                draft.invalid |= draft.reminder.is_none();
            }
            continue;
        }

        match upper.as_str() {
            "UID" => draft.uid = Some(unescape_text(value)),
            "SUMMARY" => draft.summary = unescape_text(value),
            "DESCRIPTION" => draft.description = unescape_text(value),
            "LOCATION" => draft.location = unescape_text(value),
            "CATEGORIES" => {
                draft.category = unescape_text(value.split(',').next().unwrap_or(""));
            }
            "DTSTART" => draft.start = parse_datetime(value, params).or_else(|| {
                draft.invalid = true;
                None
            }),
            "DTEND" => draft.end = parse_datetime(value, params).or_else(|| {
                draft.invalid = true;
                None
            }),
            "RRULE" => match RecurrenceRule::parse(value) {
                Ok(rule) => draft.rrule = Some(rule),
                Err(_) => draft.invalid = true,
            },
            "EXDATE" => {
                for item in value.split(',') {
                    match parse_datetime(item, params) {
                        Some((date, _)) => draft.exdates.push(date),
                        None => draft.invalid = true,
                    }
                }
            }
            _ => {}
        }
    }

    if let Some((number, _)) = current {
        return Err(IcsError { line: number, message: "BEGIN:VEVENT sem END:VEVENT".into() });
    }

    Ok(ImportReport { events, skipped })
}

/// Splits `NAME;PARAM=...:value`. Returns `(name, params, value)`.
fn split_property(text: &str) -> Option<(&str, &str, &str)> {
    let (head, value) = text.split_once(':')?;
    let (name, params) = head.split_once(';').unwrap_or((head, ""));
    Some((name.trim(), params, value.trim()))
}

fn finish(draft: Draft) -> Option<IcsEvent> {
    let (start, start_is_date) = draft.start?;
    if draft.invalid {
        return None;
    }
    let all_day = start_is_date;
    let end = match draft.end {
        Some((end, _)) => end,
        None if all_day => start + chrono::Duration::days(1),
        None => start,
    };
    let title = if draft.summary.trim().is_empty() {
        "(sem título)".to_string()
    } else {
        draft.summary
    };
    let event = Event {
        id: None,
        uid: draft.uid.unwrap_or_else(new_uid),
        title,
        description: draft.description,
        location: draft.location,
        category_id: 0,
        all_day,
        start,
        end,
        rrule: draft.rrule,
        reminder_minutes: draft.reminder,
    };
    if event.validate().is_err() {
        return None;
    }
    Some(IcsEvent { event, category: draft.category, exdates: draft.exdates })
}

/// Reads a DATE or DATE-TIME value. The flag is true for DATE values.
fn parse_datetime(value: &str, params: &str) -> Option<(NaiveDateTime, bool)> {
    let value = value.trim();
    let is_date_param = params.to_ascii_uppercase().contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME");
    if value.len() == 8 || is_date_param {
        let date = NaiveDate::parse_from_str(value.get(..8)?, "%Y%m%d").ok()?;
        return Some((date.and_time(NaiveTime::MIN), true));
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let naive = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        let local = Utc.from_utc_datetime(&naive).with_timezone(&Local).naive_local();
        return Some((local, false));
    }
    NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok().map(|dt| (dt, false))
}

/// Reads a relative TRIGGER such as `-PT15M`, `-PT1H` or `-P1D`. Returns minutes before the start.
fn parse_trigger(value: &str) -> Option<i64> {
    let body = value.trim().strip_prefix('-')?;
    let body = body.strip_prefix('P')?;
    if let Some(days) = body.strip_suffix('D') {
        return days.parse::<i64>().ok().map(|d| d * 24 * 60);
    }
    let time = body.strip_prefix('T')?;
    if let Some(hours) = time.strip_suffix('H') {
        return hours.parse::<i64>().ok().map(|h| h * 60);
    }
    time.strip_suffix('M')?.parse::<i64>().ok()
}
