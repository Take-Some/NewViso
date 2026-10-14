use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CalendarDate {
    pub fn new(year: i32, month: u32, day: u32) -> Result<Self, String> {
        let date = Self { year, month, day };
        date.validate()?;
        Ok(date)
    }
    pub fn is_leap_year(year: i32) -> bool {
        year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
    }
    pub fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            2 => {
                if Self::is_leap_year(year) {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        }
    }
    pub fn validate(self) -> Result<(), String> {
        if !(1..=9999).contains(&self.year)
            || self.day == 0
            || self.day > Self::days_in_month(self.year, self.month)
        {
            return Err("calendar date is outside the Gregorian calendar range".into());
        }
        Ok(())
    }
    fn days_before_year(year: i32) -> i64 {
        let y = i64::from(year - 1);
        y * 365 + y / 4 - y / 100 + y / 400
    }
    /// Zero-based absolute day since 0001-01-01. The weekday uses Sunday=0.
    pub fn serial_day(self) -> i64 {
        Self::days_before_year(self.year)
            + (1..self.month)
                .map(|m| i64::from(Self::days_in_month(self.year, m)))
                .sum::<i64>()
            + i64::from(self.day - 1)
    }
    pub fn weekday(self) -> u32 {
        (self.serial_day() + 1).rem_euclid(7) as u32
    }
    pub fn add_days(&mut self, delta: i64) -> Result<(), String> {
        self.validate()?;
        let serial = self
            .serial_day()
            .checked_add(delta)
            .ok_or("calendar day overflow")?;
        if serial < 0 || serial >= Self::days_before_year(10000) {
            return Err("calendar day outside 1..=9999".into());
        }
        let (mut lo, mut hi) = (1, 10000);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if Self::days_before_year(mid) <= serial {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let mut rem = serial - Self::days_before_year(lo);
        let mut month = 1;
        while rem >= i64::from(Self::days_in_month(lo, month)) {
            rem -= i64::from(Self::days_in_month(lo, month));
            month += 1;
        }
        *self = Self {
            year: lo,
            month,
            day: rem as u32 + 1,
        };
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameClock {
    pub date: CalendarDate,
    pub day_seconds: f64,
    pub milliseconds_per_game_minute: f64,
    pub time_scale: f64,
    pub paused: bool,
    pub fixed: bool,
    pub elapsed_days: f64,
}
impl GameClock {
    pub fn new(
        date: CalendarDate,
        hours: f64,
        milliseconds_per_game_minute: f64,
    ) -> Result<Self, String> {
        let mut clock = Self {
            date,
            day_seconds: 0.0,
            milliseconds_per_game_minute,
            time_scale: 1.0,
            paused: false,
            fixed: false,
            elapsed_days: 0.0,
        };
        clock.set_time(hours * 3600.0)?;
        clock.validate()?;
        Ok(clock)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.date.validate()?;
        if !self.day_seconds.is_finite()
            || !(0.0..86400.0).contains(&self.day_seconds)
            || !self.milliseconds_per_game_minute.is_finite()
            || self.milliseconds_per_game_minute <= 0.0
            || !self.time_scale.is_finite()
            || self.time_scale.abs() > 100000.0
            || !self.elapsed_days.is_finite()
            || (60000.0 / self.milliseconds_per_game_minute * self.time_scale).abs() > 100000.0
        {
            return Err("invalid game clock state".into());
        }
        Ok(())
    }
    pub fn set_date(&mut self, date: CalendarDate) -> Result<(), String> {
        date.validate()?;
        self.elapsed_days += (date.serial_day() - self.date.serial_day()) as f64;
        self.date = date;
        Ok(())
    }
    pub fn hours(&self) -> f64 {
        self.day_seconds / 3600.0
    }
    pub fn phase(&self) -> f64 {
        self.day_seconds / 86400.0
    }
    pub fn rate(&self) -> f64 {
        if self.paused || self.fixed {
            0.0
        } else {
            60000.0 / self.milliseconds_per_game_minute * self.time_scale
        }
    }
    pub fn set_time(&mut self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds.abs() > 1.0e12 {
            return Err("game clock time is invalid".into());
        }
        let next = seconds.rem_euclid(86400.0);
        self.elapsed_days += (next - self.day_seconds) / 86400.0;
        self.day_seconds = next;
        Ok(())
    }
    pub fn add_time(&mut self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds.abs() > 1.0e12 {
            return Err("game clock increment is invalid".into());
        }
        let total = self.day_seconds + seconds;
        let days = total.div_euclid(86400.0) as i64;
        let mut date = self.date;
        date.add_days(days)?;
        self.date = date;
        self.day_seconds = total.rem_euclid(86400.0);
        self.elapsed_days += seconds / 86400.0;
        Ok(())
    }
    pub fn advance_to(&mut self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || !(0.0..86400.0).contains(&seconds) {
            return Err("advance-to time is invalid".into());
        }
        self.add_time((seconds - self.day_seconds).rem_euclid(86400.0))
    }
    pub fn step(&mut self, dt: f64) -> Result<(), String> {
        super::valid_dt(dt)?;
        self.validate()?;
        self.add_time(dt * self.rate())
    }
    pub fn is_in_hour_range(&self, start: u32, end: u32) -> bool {
        let h = self.hours();
        if start <= end {
            h >= f64::from(start) && h < f64::from(end)
        } else {
            h >= f64::from(start) || h < f64::from(end)
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"year":self.date.year,"month":self.date.month,"day":self.date.day,"weekday":self.date.weekday(),
            "hour":self.day_seconds as u32/3600,"minute":self.day_seconds as u32/60%60,"second":self.day_seconds as u32%60,
            "day_seconds":self.day_seconds,"elapsed_days":self.elapsed_days,"rate":self.rate(),"paused":self.paused,"fixed":self.fixed})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn leap_midnight_and_reverse() {
        let mut c = GameClock::new(CalendarDate::new(2024, 2, 28).unwrap(), 23.99, 2000.0).unwrap();
        c.add_time(3600.0).unwrap();
        assert_eq!(c.date, CalendarDate::new(2024, 2, 29).unwrap());
        c.add_time(86400.0).unwrap();
        assert_eq!(c.date, CalendarDate::new(2024, 3, 1).unwrap());
        c.add_time(-86400.0).unwrap();
        assert_eq!(c.date.day, 29);
    }
    #[test]
    fn cadence_pause_and_skip() {
        let mut c = GameClock::new(CalendarDate::new(2026, 1, 1).unwrap(), 23.0, 2000.0).unwrap();
        c.step(120.0).unwrap();
        assert_eq!(c.date.day, 2);
        assert!(c.hours().abs() < 1e-9);
        c.paused = true;
        c.step(100.0).unwrap();
        assert_eq!(c.day_seconds, 0.0);
        c.advance_to(23.0 * 3600.0).unwrap();
        c.advance_to(2.0 * 3600.0).unwrap();
        assert_eq!(c.date.day, 3);
        assert_eq!(c.hours(), 2.0);
    }
    #[test]
    fn calendar_roundtrip() {
        for year in [1, 1900, 2000, 2024, 9999] {
            for month in 1..=12 {
                let d = CalendarDate::new(year, month, 1).unwrap();
                let mut next = d;
                next.add_days(0).unwrap();
                assert_eq!(d, next);
            }
        }
        assert!(!CalendarDate::is_leap_year(1900));
        assert!(CalendarDate::is_leap_year(2000));
    }
}
