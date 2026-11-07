//! Subset of RFC 5545 RRULE: FREQ, INTERVAL, COUNT, UNTIL, BYDAY (with ordinals) and BYMONTHDAY.
//! Unsupported parts are ignored. Expansion is lazy in the period count and capped for safety.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

use super::DomainError;
use super::datetime::{days_in_month, monday_of};

/// Upper bound on periods visited in one expansion, so bad rules cannot loop forever.
const MAX_PERIODS: i64 = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrenceRule {
    pub freq: Freq,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<NaiveDate>,
    /// `(ordinal, weekday)`. `None` means every such weekday; `Some(-1)` means the last one.
    pub by_day: Vec<(Option<i32>, Weekday)>,
    pub by_month_day: Vec<u32>,
}

impl RecurrenceRule {
    /// Simple rule with no extra constraints, used by the editor's presets.
    pub fn simple(freq: Freq) -> Self {
        Self {
            freq,
            interval: 1,
            count: None,
            until: None,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
        }
    }

    pub fn parse(input: &str) -> Result<Self, DomainError> {
        let trimmed = input.trim();
        let body = trimmed.strip_prefix("RRULE:").unwrap_or(trimmed);

        let mut freq = None;
        let mut interval = 1;
        let mut count = None;
        let mut until = None;
        let mut by_day = Vec::new();
        let mut by_month_day = Vec::new();

        for part in body.split(';').filter(|p| !p.is_empty()) {
            let (key, value) = part
                .split_once('=')
                .ok_or(DomainError::InvalidRule("parte sem '='"))?;
            match key.to_ascii_uppercase().as_str() {
                "FREQ" => {
                    freq = Some(match value {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => return Err(DomainError::InvalidRule("FREQ desconhecida")),
                    })
                }
                "INTERVAL" => {
                    interval = value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| *v >= 1)
                        .ok_or(DomainError::InvalidRule("INTERVAL deve ser um inteiro maior que zero"))?;
                }
                "COUNT" => {
                    count = Some(
                        value
                            .parse::<u32>()
                            .map_err(|_| DomainError::InvalidRule("COUNT inválido"))?,
                    );
                }
                "UNTIL" => {
                    until = Some(
                        NaiveDate::parse_from_str(value.get(..8).unwrap_or(""), "%Y%m%d")
                            .map_err(|_| DomainError::InvalidRule("UNTIL inválido"))?,
                    );
                }
                "BYDAY" => {
                    for item in value.split(',') {
                        by_day.push(parse_by_day(item)?);
                    }
                }
                "BYMONTHDAY" => {
                    for item in value.split(',') {
                        let day: u32 = item
                            .parse()
                            .map_err(|_| DomainError::InvalidRule("BYMONTHDAY inválido"))?;
                        if !(1..=31).contains(&day) {
                            return Err(DomainError::InvalidRule("BYMONTHDAY fora de 1..31"));
                        }
                        by_month_day.push(day);
                    }
                }
                _ => {}
            }
        }

        Ok(Self {
            freq: freq.ok_or(DomainError::InvalidRule("FREQ ausente"))?,
            interval,
            count,
            until,
            by_day,
            by_month_day,
        })
    }

    pub fn to_rrule_string(&self) -> String {
        let mut parts = vec![format!(
            "FREQ={}",
            match self.freq {
                Freq::Daily => "DAILY",
                Freq::Weekly => "WEEKLY",
                Freq::Monthly => "MONTHLY",
                Freq::Yearly => "YEARLY",
            }
        )];
        if self.interval != 1 {
            parts.push(format!("INTERVAL={}", self.interval));
        }
        if let Some(count) = self.count {
            parts.push(format!("COUNT={count}"));
        }
        if let Some(until) = self.until {
            parts.push(format!("UNTIL={}", until.format("%Y%m%d")));
        }
        if !self.by_day.is_empty() {
            let items: Vec<String> = self
                .by_day
                .iter()
                .map(|(ord, wd)| match ord {
                    Some(n) => format!("{n}{}", weekday_code(*wd)),
                    None => weekday_code(*wd).to_string(),
                })
                .collect();
            parts.push(format!("BYDAY={}", items.join(",")));
        }
        if !self.by_month_day.is_empty() {
            let items: Vec<String> = self.by_month_day.iter().map(u32::to_string).collect();
            parts.push(format!("BYMONTHDAY={}", items.join(",")));
        }
        parts.join(";")
    }
}

fn weekday_code(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

fn parse_weekday(code: &str) -> Option<Weekday> {
    Some(match code {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}

fn parse_by_day(item: &str) -> Result<(Option<i32>, Weekday), DomainError> {
    let invalid = DomainError::InvalidRule("BYDAY inválido");
    let split = item.len().checked_sub(2).ok_or(invalid.clone())?;
    let code = item.get(split..).ok_or(invalid.clone())?;
    let prefix = item.get(..split).ok_or(invalid.clone())?;
    let weekday = parse_weekday(code).ok_or(invalid.clone())?;
    let ordinal = if prefix.is_empty() {
        None
    } else {
        let n: i32 = prefix.parse().map_err(|_| invalid.clone())?;
        if n == 0 || !(-53..=53).contains(&n) {
            return Err(invalid);
        }
        Some(n)
    };
    Ok((ordinal, weekday))
}

/// Start times of every occurrence whose date lies in `[from, to]`, in chronological order.
///
/// The first occurrence is always `start` (when it matches the rule), so COUNT and UNTIL are
/// counted from the beginning of the series, even when the window starts later.
pub fn occurrences(
    rule: &RecurrenceRule,
    start: NaiveDateTime,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<NaiveDateTime> {
    let anchor = start.date();
    let interval = i64::from(rule.interval.max(1));
    let mut out = Vec::new();
    let mut produced: u32 = 0;

    for period in 0..MAX_PERIODS {
        let dates = period_dates(rule, anchor, period * interval);
        for date in dates {
            if date < anchor {
                continue;
            }
            if rule.until.is_some_and(|until| date > until) || date > to {
                return out;
            }
            produced = produced.saturating_add(1);
            if rule.count.is_some_and(|count| produced > count) {
                return out;
            }
            if date >= from {
                out.push(date.and_time(start.time()));
            }
        }
    }
    out
}

/// Candidate dates for one period, sorted. `offset` is in the rule's unit (days, weeks, months or years).
fn period_dates(rule: &RecurrenceRule, anchor: NaiveDate, offset: i64) -> Vec<NaiveDate> {
    match rule.freq {
        Freq::Daily => vec![anchor + Duration::days(offset)],
        Freq::Weekly => {
            let monday = monday_of(anchor) + Duration::weeks(offset);
            let mut days: Vec<NaiveDate> = if rule.by_day.is_empty() {
                vec![anchor.weekday()]
            } else {
                rule.by_day.iter().map(|(_, wd)| *wd).collect()
            }
            .into_iter()
            .map(|wd| monday + Duration::days(i64::from(wd.num_days_from_monday())))
            .collect();
            days.sort();
            days.dedup();
            days
        }
        Freq::Monthly => {
            let month_index = i64::from(anchor.month0()) + offset;
            let year = anchor.year() + month_index.div_euclid(12) as i32;
            let month = month_index.rem_euclid(12) as u32 + 1;
            month_candidates(rule, anchor, year, month)
        }
        Freq::Yearly => {
            let year = anchor.year() + offset as i32;
            month_candidates(rule, anchor, year, anchor.month())
        }
    }
}

fn month_candidates(rule: &RecurrenceRule, anchor: NaiveDate, year: i32, month: u32) -> Vec<NaiveDate> {
    let dim = days_in_month(year, month);
    let mut days = Vec::new();
    if !rule.by_month_day.is_empty() {
        for &day in &rule.by_month_day {
            if day <= dim {
                days.extend(NaiveDate::from_ymd_opt(year, month, day));
            }
        }
    } else if !rule.by_day.is_empty() {
        for &(ordinal, wd) in &rule.by_day {
            match ordinal {
                None => {
                    for day in 1..=dim {
                        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
                            && date.weekday() == wd
                        {
                            days.push(date);
                        }
                    }
                }
                Some(n) => days.extend(nth_weekday(year, month, wd, n)),
            }
        }
    } else {
        days.extend(NaiveDate::from_ymd_opt(year, month, anchor.day()));
    }
    days.sort();
    days.dedup();
    days
}

/// The `n`-th weekday of a month (`n < 0` counts from the end).
fn nth_weekday(year: i32, month: u32, wd: Weekday, n: i32) -> Option<NaiveDate> {
    let dim = days_in_month(year, month);
    if n > 0 {
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let offset = (i64::from(wd.num_days_from_monday())
            - i64::from(first.weekday().num_days_from_monday())
            + 7)
            % 7;
        let day = 1 + offset + 7 * (i64::from(n) - 1);
        NaiveDate::from_ymd_opt(year, month, u32::try_from(day).ok()?).filter(|_| day <= i64::from(dim))
    } else {
        let last = NaiveDate::from_ymd_opt(year, month, dim)?;
        let back = (i64::from(last.weekday().num_days_from_monday())
            - i64::from(wd.num_days_from_monday())
            + 7)
            % 7;
        let day = i64::from(dim) - back - 7 * (i64::from(-n) - 1);
        NaiveDate::from_ymd_opt(year, month, u32::try_from(day).ok()?).filter(|_| day >= 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn dt(y: i32, m: u32, d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_time(NaiveTime::from_hms_opt(9, 0, 0).unwrap())
    }

    fn all(rule: &str, start: NaiveDateTime, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        let rule = RecurrenceRule::parse(rule).unwrap();
        occurrences(&rule, start, from, to).into_iter().map(|d| d.date()).collect()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn daily_with_count_and_interval() {
        let got = all("FREQ=DAILY;COUNT=3", dt(2026, 1, 1), d(2020, 1, 1), d(2030, 1, 1));
        assert_eq!(got, vec![d(2026, 1, 1), d(2026, 1, 2), d(2026, 1, 3)]);

        let got = all("FREQ=DAILY;INTERVAL=2;COUNT=3", dt(2026, 1, 1), d(2020, 1, 1), d(2030, 1, 1));
        assert_eq!(got, vec![d(2026, 1, 1), d(2026, 1, 3), d(2026, 1, 5)]);
    }

    #[test]
    fn window_only_returns_dates_inside_range() {
        let got = all("FREQ=DAILY", dt(2026, 1, 1), d(2026, 1, 3), d(2026, 1, 4));
        assert_eq!(got, vec![d(2026, 1, 3), d(2026, 1, 4)]);
    }

    #[test]
    fn until_stops_the_series() {
        let got = all("FREQ=DAILY;UNTIL=20260103", dt(2026, 1, 1), d(2026, 1, 1), d(2026, 2, 1));
        assert_eq!(got, vec![d(2026, 1, 1), d(2026, 1, 2), d(2026, 1, 3)]);
    }

    #[test]
    fn weekly_by_day_mon_wed() {
        // 2026-01-05 is a Monday.
        let got = all("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=4", dt(2026, 1, 5), d(2026, 1, 1), d(2026, 2, 1));
        assert_eq!(got, vec![d(2026, 1, 5), d(2026, 1, 7), d(2026, 1, 12), d(2026, 1, 14)]);
    }

    #[test]
    fn monthly_day_31_skips_short_months() {
        let got = all("FREQ=MONTHLY;BYMONTHDAY=31;COUNT=4", dt(2026, 1, 31), d(2026, 1, 1), d(2027, 1, 1));
        assert_eq!(got, vec![d(2026, 1, 31), d(2026, 3, 31), d(2026, 5, 31), d(2026, 7, 31)]);
    }

    #[test]
    fn monthly_default_uses_start_day_and_skips_missing_days() {
        let got = all("FREQ=MONTHLY;COUNT=3", dt(2026, 1, 30), d(2026, 1, 1), d(2027, 1, 1));
        assert_eq!(got, vec![d(2026, 1, 30), d(2026, 3, 30), d(2026, 4, 30)]);
    }

    #[test]
    fn last_friday_of_month() {
        // Last Friday of Jan 2026 is the 30th; of Feb 2026 is the 27th.
        let got = all("FREQ=MONTHLY;BYDAY=-1FR;COUNT=2", dt(2026, 1, 30), d(2026, 1, 1), d(2027, 1, 1));
        assert_eq!(got, vec![d(2026, 1, 30), d(2026, 2, 27)]);
    }

    #[test]
    fn second_monday_of_month() {
        // Second Monday of Jan 2026 is the 12th.
        let got = all("FREQ=MONTHLY;BYDAY=2MO;COUNT=1", dt(2026, 1, 12), d(2026, 1, 1), d(2027, 1, 1));
        assert_eq!(got, vec![d(2026, 1, 12)]);
    }

    #[test]
    fn yearly_feb_29_only_on_leap_years() {
        let got = all("FREQ=YEARLY;COUNT=3", dt(2024, 2, 29), d(2020, 1, 1), d(2040, 1, 1));
        assert_eq!(got, vec![d(2024, 2, 29), d(2028, 2, 29), d(2032, 2, 29)]);
    }

    #[test]
    fn yearly_keeps_start_month_and_day() {
        let got = all("FREQ=YEARLY;COUNT=2", dt(2026, 5, 10), d(2026, 1, 1), d(2030, 1, 1));
        assert_eq!(got, vec![d(2026, 5, 10), d(2027, 5, 10)]);
    }

    #[test]
    fn round_trip_and_errors() {
        let rule = RecurrenceRule::parse("RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,-1FR;COUNT=5").unwrap();
        assert_eq!(rule.to_rrule_string(), "FREQ=WEEKLY;INTERVAL=2;COUNT=5;BYDAY=MO,-1FR");
        assert!(RecurrenceRule::parse("INTERVAL=2").is_err());
        assert!(RecurrenceRule::parse("FREQ=HOURLY").is_err());
        assert!(RecurrenceRule::parse("FREQ=DAILY;INTERVAL=0").is_err());
        assert!(RecurrenceRule::parse("FREQ=MONTHLY;BYMONTHDAY=40").is_err());
        assert!(RecurrenceRule::parse("FREQ=WEEKLY;BYDAY=ñ").is_err());
        assert!(RecurrenceRule::parse("FREQ=DAILY;COUNT").is_err());
    }
}
