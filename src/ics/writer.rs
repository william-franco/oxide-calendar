use chrono::{NaiveDateTime, Utc};

use super::{IcsEvent, escape_text};
use crate::domain::recurrence::RecurrenceRule;

const MAX_LINE_OCTETS: usize = 75;

/// Serializes events as a VCALENDAR document with CRLF line endings and folded lines.
pub fn write(items: &[IcsEvent]) -> String {
    let mut out = String::new();
    push_line(&mut out, "BEGIN:VCALENDAR");
    push_line(&mut out, "VERSION:2.0");
    push_line(&mut out, "PRODID:-//Oxide//Oxide Calendar//PT");
    push_line(&mut out, "CALSCALE:GREGORIAN");

    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    for item in items {
        let event = &item.event;
        push_line(&mut out, "BEGIN:VEVENT");
        push_line(&mut out, &format!("UID:{}", escape_text(&event.uid)));
        push_line(&mut out, &format!("DTSTAMP:{stamp}"));

        if event.all_day {
            push_line(&mut out, &format!("DTSTART;VALUE=DATE:{}", event.start.format("%Y%m%d")));
            // DTEND is exclusive for all-day events, as stored.
            push_line(&mut out, &format!("DTEND;VALUE=DATE:{}", event.end.format("%Y%m%d")));
        } else {
            push_line(&mut out, &format!("DTSTART:{}", local(event.start)));
            push_line(&mut out, &format!("DTEND:{}", local(event.end)));
        }

        push_line(&mut out, &format!("SUMMARY:{}", escape_text(&event.title)));
        if !event.description.is_empty() {
            push_line(&mut out, &format!("DESCRIPTION:{}", escape_text(&event.description)));
        }
        if !event.location.is_empty() {
            push_line(&mut out, &format!("LOCATION:{}", escape_text(&event.location)));
        }
        if !item.category.is_empty() {
            push_line(&mut out, &format!("CATEGORIES:{}", escape_text(&item.category)));
        }
        if let Some(rule) = &event.rrule {
            push_line(&mut out, &format!("RRULE:{}", rule_string(rule)));
        }
        if !item.exdates.is_empty() {
            let values: Vec<String> = item
                .exdates
                .iter()
                .map(|d| if event.all_day { d.format("%Y%m%d").to_string() } else { local(*d) })
                .collect();
            let prefix = if event.all_day { "EXDATE;VALUE=DATE" } else { "EXDATE" };
            push_line(&mut out, &format!("{prefix}:{}", values.join(",")));
        }
        if let Some(minutes) = event.reminder_minutes {
            push_line(&mut out, "BEGIN:VALARM");
            push_line(&mut out, &format!("TRIGGER:-PT{minutes}M"));
            push_line(&mut out, "ACTION:DISPLAY");
            push_line(&mut out, "DESCRIPTION:Lembrete");
            push_line(&mut out, "END:VALARM");
        }
        push_line(&mut out, "END:VEVENT");
    }

    push_line(&mut out, "END:VCALENDAR");
    out
}

fn local(value: NaiveDateTime) -> String {
    value.format("%Y%m%dT%H%M%S").to_string()
}

fn rule_string(rule: &RecurrenceRule) -> String {
    rule.to_rrule_string()
}

/// Appends one content line, folding it so no physical line exceeds 75 octets.
/// Folding never splits a UTF-8 character; continuation lines start with a space.
fn push_line(out: &mut String, line: &str) {
    let mut used = 0;
    let mut limit = MAX_LINE_OCTETS;
    for ch in line.chars() {
        let len = ch.len_utf8();
        if used + len > limit {
            out.push_str("\r\n ");
            used = 0;
            limit = MAX_LINE_OCTETS - 1;
        }
        out.push(ch);
        used += len;
    }
    out.push_str("\r\n");
}
