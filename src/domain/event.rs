use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};

use super::DomainError;
use super::recurrence::{RecurrenceRule, occurrences};

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: Option<i64>,
    pub uid: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub category_id: i64,
    pub all_day: bool,
    /// Wall-clock start. For all-day events the time is midnight.
    pub start: NaiveDateTime,
    /// Wall-clock end, exclusive (same convention as iCalendar).
    pub end: NaiveDateTime,
    pub rrule: Option<RecurrenceRule>,
    pub reminder_minutes: Option<i64>,
}

/// One concrete instance of an event (a single occurrence of a series, or a one-off event).
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub event: Event,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

impl Event {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.title.trim().is_empty() {
            return Err(DomainError::InvalidEvent("título obrigatório"));
        }
        if self.end < self.start {
            return Err(DomainError::InvalidEvent("fim anterior ao início"));
        }
        if self.reminder_minutes.is_some_and(|m| m < 0) {
            return Err(DomainError::InvalidEvent("lembrete não pode ser negativo"));
        }
        Ok(())
    }

    pub fn duration(&self) -> Duration {
        self.end - self.start
    }

    /// Copy with a new identity, as used by "duplicate". Keeps the series and its rule.
    pub fn duplicate(&self) -> Event {
        Event { id: None, uid: new_uid(), ..self.clone() }
    }

    /// Builds the all-day span from inclusive dates, storing the end exclusively.
    pub fn all_day_span(first: NaiveDate, last_inclusive: NaiveDate) -> (NaiveDateTime, NaiveDateTime) {
        let start = first.and_time(NaiveTime::MIN);
        let end = (last_inclusive + Duration::days(1)).and_time(NaiveTime::MIN);
        (start, end)
    }
}

/// Occurrences overlapping the inclusive date range `[from, to]`, minus `excluded` starts
/// (cancelled occurrences).
pub fn occurrences_in(
    event: &Event,
    excluded: &[NaiveDateTime],
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<Occurrence> {
    let duration = event.duration();
    // A series that started earlier can still overlap the window if it is longer than a day.
    let lookback = Duration::days(duration.num_days().max(0) + 1);
    let starts = match &event.rrule {
        None => vec![event.start],
        Some(rule) => occurrences(rule, event.start, from - lookback, to),
    };

    let range_start = from.and_time(NaiveTime::MIN);
    let range_end = (to + Duration::days(1)).and_time(NaiveTime::MIN);

    starts
        .into_iter()
        .filter(|start| !excluded.contains(start))
        .map(|start| Occurrence { event: event.clone(), start, end: start + duration })
        .filter(|o| o.start < range_end && (o.end > range_start || o.start >= range_start))
        .collect()
}

static UID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Globally unique-enough identifier for local events, without an external UUID crate.
pub fn new_uid() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let counter = UID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{counter:x}@oxide-calendar")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::recurrence::Freq;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    fn event(start: NaiveDateTime, end: NaiveDateTime, rrule: Option<RecurrenceRule>) -> Event {
        Event {
            id: Some(1),
            uid: "u".into(),
            title: "Reunião".into(),
            description: String::new(),
            location: String::new(),
            category_id: 1,
            all_day: false,
            start,
            end,
            rrule,
            reminder_minutes: None,
        }
    }

    #[test]
    fn validation_rules() {
        let mut e = event(at(2026, 1, 1, 9, 0), at(2026, 1, 1, 10, 0), None);
        assert!(e.validate().is_ok());
        e.title = "   ".into();
        assert!(e.validate().is_err());
        e.title = "x".into();
        e.end = at(2026, 1, 1, 8, 0);
        assert!(e.validate().is_err());
    }

    #[test]
    fn event_crossing_midnight_appears_on_both_days() {
        let e = event(at(2026, 1, 1, 23, 0), at(2026, 1, 2, 1, 0), None);
        let day1 = occurrences_in(&e, &[], NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        let day2 = occurrences_in(&e, &[], NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(), NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        assert_eq!(day1.len(), 1);
        assert_eq!(day2.len(), 1);
    }

    #[test]
    fn all_day_span_is_exclusive() {
        let (start, end) = Event::all_day_span(
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        );
        assert_eq!(start, at(2026, 3, 1, 0, 0));
        assert_eq!(end, at(2026, 3, 2, 0, 0));
    }

    #[test]
    fn cancelled_occurrence_is_removed_from_series() {
        let rule = RecurrenceRule { count: Some(3), ..RecurrenceRule::simple(Freq::Daily) };
        let e = event(at(2026, 1, 1, 9, 0), at(2026, 1, 1, 10, 0), Some(rule));
        let from = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let all = occurrences_in(&e, &[], from, to);
        let without_second = occurrences_in(&e, &[at(2026, 1, 2, 9, 0)], from, to);
        assert_eq!(all.len(), 3);
        assert_eq!(without_second.len(), 2);
    }

    #[test]
    fn duplicate_gets_new_uid_and_no_id() {
        let e = event(at(2026, 1, 1, 9, 0), at(2026, 1, 1, 10, 0), None);
        let copy = e.duplicate();
        assert_eq!(copy.id, None);
        assert_ne!(copy.uid, e.uid);
        assert_eq!(copy.title, e.title);
    }
}
