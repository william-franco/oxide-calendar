use chrono::{Datelike, Duration, NaiveDate};

/// Number of days in the given month (1-12). Falls back to 31 for invalid input.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1);
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    match (first, next) {
        (Some(a), Some(b)) => (b - a).num_days() as u32,
        _ => 31,
    }
}

/// Sunday on or before `date`: the start of the Sunday-first week.
pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_sunday() as i64)
}

/// Monday on or before `date`: RFC 5545 default week start, used for WEEKLY recurrence.
pub fn monday_of(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// Six full weeks (42 days) covering the month, starting on a Sunday.
pub fn month_grid(year: i32, month: u32) -> Vec<NaiveDate> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let start = week_start(first);
    (0..42).map(|i| start + Duration::days(i)).collect()
}

/// Date of the month `month` in `year`, clamped to the month's last day.
pub fn clamped_date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day.min(days_in_month(year, month)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn month_lengths_including_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn grid_starts_on_sunday_and_covers_the_month() {
        // 1 Nov 2026 is a Sunday.
        let grid = month_grid(2026, 11);
        assert_eq!(grid.len(), 42);
        assert_eq!(grid[0], d(2026, 11, 1));
        // 1 Feb 2026 is a Sunday; 1 Mar 2026 is a Sunday.
        let grid = month_grid(2026, 3);
        assert_eq!(grid[0], week_start(d(2026, 3, 1)));
        assert!(grid.contains(&d(2026, 3, 31)));
    }

    #[test]
    fn clamps_day_to_month_end() {
        assert_eq!(clamped_date(2026, 2, 31), Some(d(2026, 2, 28)));
        assert_eq!(clamped_date(2024, 2, 31), Some(d(2024, 2, 29)));
    }
}
