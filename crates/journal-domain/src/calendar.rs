use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::{Holiday, WeekStart};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CalendarViewMode {
    Week,
    #[default]
    Month,
    Semester,
    AcademicYear,
    Custom,
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

pub fn prev_month(year: i32, month: u32) -> (i32, u32) {
    if month <= 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

pub fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month >= 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

pub fn month_bounds(year: i32, month: u32) -> Option<(NaiveDate, NaiveDate)> {
    let last_day = days_in_month(year, month);
    if last_day == 0 {
        return None;
    }
    let start = NaiveDate::from_ymd_opt(year, month, 1)?;
    let end = NaiveDate::from_ymd_opt(year, month, last_day)?;
    Some((start, end))
}

pub fn week_bounds(anchor: NaiveDate, week_start: WeekStart) -> (NaiveDate, NaiveDate) {
    let offset = match week_start {
        WeekStart::Monday => anchor.weekday().num_days_from_monday() as i64,
        WeekStart::Sunday => anchor.weekday().num_days_from_sunday() as i64,
    };
    let start = anchor - Duration::days(offset);
    let end = start + Duration::days(6);
    (start, end)
}

/// Default academic year bounds: September 1 through August 31.
pub fn academic_year_bounds(anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start_year = if anchor.month() >= 9 {
        anchor.year()
    } else {
        anchor.year() - 1
    };
    let start = NaiveDate::from_ymd_opt(start_year, 9, 1).expect("valid Sept 1");
    let end = NaiveDate::from_ymd_opt(start_year + 1, 8, 31).expect("valid Aug 31");
    (start, end)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarDay {
    pub date: NaiveDate,
    pub day: u32,
    pub weekday: Weekday,
    pub is_today: bool,
    pub is_weekend: bool,
    pub holiday_name: Option<String>,
}

pub fn build_date_days(
    start: NaiveDate,
    end: NaiveDate,
    today: NaiveDate,
    holidays: &[Holiday],
) -> Vec<CalendarDay> {
    if start > end {
        return Vec::new();
    }
    let holiday_map: HashMap<NaiveDate, String> = holidays
        .iter()
        .map(|h| (h.date, h.name.clone()))
        .collect();

    let total_days = (end - start).num_days() as usize + 1;
    let mut days = Vec::with_capacity(total_days);
    let mut cur = start;
    while cur <= end {
        let wd = cur.weekday();
        days.push(CalendarDay {
            date: cur,
            day: cur.day(),
            weekday: wd,
            is_today: cur == today,
            is_weekend: matches!(wd, Weekday::Sat | Weekday::Sun),
            holiday_name: holiday_map.get(&cur).cloned(),
        });
        cur += Duration::days(1);
    }
    days
}

pub fn build_month_calendar(
    year: i32,
    month: u32,
    today: NaiveDate,
    holidays: &[Holiday],
) -> Vec<CalendarDay> {
    match month_bounds(year, month) {
        Some((start, end)) => build_date_days(start, end, today, holidays),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_year_rules_verified() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2025));
        assert!(!is_leap_year(2026));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2100));

        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 11), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn month_navigation_and_week_bounds() {
        assert_eq!(prev_month(2026, 1), (2025, 12));
        assert_eq!(next_month(2026, 12), (2027, 1));

        let thu = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap(); // Thursday
        let (mon, sun) = week_bounds(thu, WeekStart::Monday);
        assert_eq!(mon, NaiveDate::from_ymd_opt(2026, 11, 23).unwrap());
        assert_eq!(sun, NaiveDate::from_ymd_opt(2026, 11, 29).unwrap());

        let (sun_start, sat_end) = week_bounds(thu, WeekStart::Sunday);
        assert_eq!(sun_start, NaiveDate::from_ymd_opt(2026, 11, 22).unwrap());
        assert_eq!(sat_end, NaiveDate::from_ymd_opt(2026, 11, 28).unwrap());
    }

    #[test]
    fn calendar_marks_today_weekends_and_holidays() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let holidays = vec![Holiday {
            id: uuid::Uuid::new_v4(),
            profile_id: None,
            date: today,
            name: "День народного единства".into(),
        }];
        let days = build_month_calendar(2026, 11, today, &holidays);
        assert_eq!(days.len(), 30);
        assert!(days[3].is_today);
        assert_eq!(
            days[3].holiday_name.as_deref(),
            Some("День народного единства")
        );
        assert!(days[0].is_weekend); // Nov 1 2026 is Sunday
    }
}
