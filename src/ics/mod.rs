//! iCalendar (RFC 5545) import and export for VEVENT components.
//!
//! Supported properties: UID, DTSTART, DTEND, SUMMARY, DESCRIPTION, LOCATION, CATEGORIES, RRULE,
//! EXDATE and VALARM/TRIGGER (minutes or hours before). Date-times with `Z` are converted to local
//! time; floating and `TZID` values are read as local wall-clock time.

mod parser;
mod writer;

use std::fmt;

use chrono::NaiveDateTime;

use crate::domain::event::Event;

pub use parser::parse;
pub use writer::write;

/// An event plus the data iCalendar carries outside `Event` itself.
#[derive(Debug, Clone, PartialEq)]
pub struct IcsEvent {
    pub event: Event,
    /// Category name from CATEGORIES (empty when absent).
    pub category: String,
    /// Cancelled occurrence starts from EXDATE.
    pub exdates: Vec<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportReport {
    pub events: Vec<IcsEvent>,
    /// VEVENTs that were well-formed but invalid (bad dates, empty title, ...) and were skipped.
    pub skipped: usize,
}

/// Structural error: the file cannot be read as iCalendar at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcsError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for IcsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Arquivo .ics inválido na linha {}: {}", self.line, self.message)
    }
}

impl std::error::Error for IcsError {}

pub(crate) fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn unescape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::Event;
    use crate::domain::recurrence::{Freq, RecurrenceRule};
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    fn sample() -> IcsEvent {
        IcsEvent {
            event: Event {
                id: None,
                uid: "abc-1@test".into(),
                title: "Reunião, com; barra \\ e\nquebra".into(),
                description: "Linha 1\nLinha 2".into(),
                location: "Sala 3".into(),
                category_id: 0,
                all_day: false,
                start: at(2026, 3, 10, 9, 30),
                end: at(2026, 3, 10, 10, 30),
                rrule: Some(RecurrenceRule { count: Some(4), ..RecurrenceRule::simple(Freq::Weekly) }),
                reminder_minutes: Some(15),
            },
            category: "Trabalho".into(),
            exdates: vec![at(2026, 3, 17, 9, 30)],
        }
    }

    #[test]
    fn round_trip_preserves_event_data() {
        let original = sample();
        let text = write(std::slice::from_ref(&original));
        let report = parse(&text).unwrap();
        assert_eq!(report.skipped, 0);
        assert_eq!(report.events.len(), 1);
        let back = &report.events[0];
        assert_eq!(back.event.title, original.event.title);
        assert_eq!(back.event.description, original.event.description);
        assert_eq!(back.event.location, original.event.location);
        assert_eq!(back.event.start, original.event.start);
        assert_eq!(back.event.end, original.event.end);
        assert_eq!(back.event.rrule, original.event.rrule);
        assert_eq!(back.event.reminder_minutes, Some(15));
        assert_eq!(back.category, "Trabalho");
        assert_eq!(back.exdates, original.exdates);
        assert_eq!(back.event.uid, "abc-1@test");
    }

    #[test]
    fn all_day_round_trip() {
        let mut item = sample();
        item.event.all_day = true;
        item.event.start = at(2026, 5, 1, 0, 0);
        item.event.end = at(2026, 5, 3, 0, 0);
        item.event.rrule = None;
        item.exdates.clear();
        let text = write(std::slice::from_ref(&item));
        assert!(text.contains("DTSTART;VALUE=DATE:20260501"));
        let back = &parse(&text).unwrap().events[0];
        assert!(back.event.all_day);
        assert_eq!(back.event.start, item.event.start);
        assert_eq!(back.event.end, item.event.end);
    }

    #[test]
    fn folds_long_lines_at_75_octets() {
        let mut item = sample();
        item.event.description = "á".repeat(200);
        let text = write(std::slice::from_ref(&item));
        for line in text.split("\r\n") {
            assert!(line.len() <= 75, "line too long: {}", line.len());
        }
        let back = &parse(&text).unwrap().events[0];
        assert_eq!(back.event.description, "á".repeat(200));
    }

    #[test]
    fn parses_external_file_with_folding_utc_and_skips_invalid_events() {
        let input = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:x1\r\nSUMMARY:Almo\r\n ço\r\nDTSTART:20260101T120000\r\nDTEND:20260101T130000\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:x2\r\nSUMMARY:Sem data\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nSUMMARY:\r\nDTSTART;VALUE=DATE:20260201\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nDTSTART:20260301T100000Z\r\nDTEND:20260301T110000Z\r\nSUMMARY:UTC\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let report = parse(input).unwrap();
        // The event without DTSTART is skipped; the one without SUMMARY gets a placeholder title.
        assert_eq!(report.skipped, 1);
        assert_eq!(report.events.len(), 3);
        assert_eq!(report.events[0].event.title, "Almoço");
        assert_eq!(report.events[0].event.uid, "x1");
        assert_eq!(report.events[1].event.title, "(sem título)");
        assert_eq!(report.events[2].event.title, "UTC");
    }

    #[test]
    fn malformed_structure_reports_line() {
        let err = parse("BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:x\n").unwrap_err();
        assert!(err.line >= 1);
        let err = parse("BEGIN:VCALENDAR\nsem dois pontos\n").unwrap_err();
        assert_eq!(err.line, 2);
        let err = parse("END:VEVENT\n").unwrap_err();
        assert_eq!(err.line, 1);
    }

    #[test]
    fn escape_round_trip() {
        let raw = "a,b;c\\d\ne";
        assert_eq!(unescape_text(&escape_text(raw)), raw);
    }
}
