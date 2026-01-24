//! Session Tracker - RTH/ETH detection and session-relative features
//!
//! Tracks trading session boundaries and computes session-relative features:
//! - RTH (Regular Trading Hours): 9:30 AM - 4:00 PM Eastern Time
//! - ETH (Extended Trading Hours): All other times
//! - Session high/low/open prices
//! - Prior day close
//! - Time-of-day features

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::America::New_York;

/// Session tracker for RTH/ETH and session-relative features
#[derive(Debug)]
pub struct SessionTracker {
    // Current session date (Eastern Time)
    current_session_date: Option<NaiveDate>,

    // RTH boundaries for current session (in Unix ms)
    rth_open_ms: Option<i64>,
    rth_close_ms: Option<i64>,
    next_session_roll_ms: Option<i64>,

    // Session prices
    session_high: f64,
    session_low: f64,
    session_open: Option<f64>,

    // Prior session close (for gap calculations)
    prior_close: Option<f64>,
    last_rth_close: Option<f64>,

    // Current state
    is_rth: bool,
    session_is_weekend: bool,
    last_timestamp_ms: i64,

    // Cached day of week (0 = Monday, 4 = Friday) - avoids timezone conversion
    cached_day_of_week: u8,

    // RTH times (Eastern)
    rth_open_time: NaiveTime,
    rth_close_time: NaiveTime,
    session_roll_time: NaiveTime,
}

impl SessionTracker {
    /// Create a new session tracker
    /// RTH is 9:30 AM - 4:00 PM Eastern for CME equity index futures
    pub fn new() -> Self {
        Self {
            current_session_date: None,
            rth_open_ms: None,
            rth_close_ms: None,
            next_session_roll_ms: None,
            session_high: f64::NEG_INFINITY,
            session_low: f64::INFINITY,
            session_open: None,
            prior_close: None,
            last_rth_close: None,
            is_rth: false,
            session_is_weekend: false,
            last_timestamp_ms: 0,
            cached_day_of_week: 0,
            rth_open_time: NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            rth_close_time: NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            session_roll_time: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        }
    }

    /// Process a tick and update session state
    pub fn on_tick(&mut self, timestamp_ms: i64, price: f64) {
        // New session started (CME session rolls at 6 PM Eastern)
        if self.current_session_date.is_none()
            || self
                .next_session_roll_ms
                .map_or(true, |roll| timestamp_ms >= roll)
        {
            self.start_new_session_from_timestamp(timestamp_ms, price);
        }

        // Update session high/low
        if price > self.session_high {
            self.session_high = price;
        }
        if price < self.session_low {
            self.session_low = price;
        }

        // Check RTH status
        let was_rth = self.is_rth;
        self.is_rth = match (self.rth_open_ms, self.rth_close_ms) {
            (Some(open_ms), Some(close_ms)) => {
                timestamp_ms >= open_ms && timestamp_ms < close_ms && !self.session_is_weekend
            }
            _ => false,
        };

        // Transitioning from RTH to ETH - capture RTH close
        if was_rth && !self.is_rth {
            self.last_rth_close = Some(price);
        }

        // Set session open on first tick of session
        if self.session_open.is_none() {
            self.session_open = Some(price);
        }

        self.last_timestamp_ms = timestamp_ms;
    }

    fn start_new_session_from_timestamp(&mut self, timestamp_ms: i64, first_price: f64) {
        // Convert to Eastern Time once per session roll
        let utc_dt = DateTime::from_timestamp_millis(timestamp_ms)
            .unwrap_or_else(|| Utc.timestamp_millis_opt(0).unwrap());
        let eastern_dt = utc_dt.with_timezone(&New_York);
        let eastern_date = eastern_dt.date_naive();
        let eastern_time = eastern_dt.time();

        let session_date = if eastern_time >= self.session_roll_time {
            eastern_date.succ_opt().unwrap_or(eastern_date)
        } else {
            eastern_date
        };

        self.start_new_session(session_date, first_price);

        self.next_session_roll_ms = session_date
            .and_time(self.session_roll_time)
            .and_local_timezone(New_York)
            .single()
            .map(|dt| dt.timestamp_millis());
    }

    /// Start a new trading session
    fn start_new_session(&mut self, session_date: NaiveDate, first_price: f64) {
        // Save prior close
        if self.session_high > f64::NEG_INFINITY {
            // Use last RTH close if available, otherwise last price of session
            self.prior_close = self.last_rth_close.or(Some(self.session_high));
        }

        self.current_session_date = Some(session_date);
        self.session_is_weekend = self.is_weekend(session_date);
        self.cached_day_of_week = session_date.weekday().num_days_from_monday() as u8;
        self.session_high = first_price;
        self.session_low = first_price;
        self.session_open = Some(first_price);
        self.last_rth_close = None;
        self.is_rth = false;

        // Calculate RTH boundaries for this session
        if let Some(rth_open) = session_date
            .and_time(self.rth_open_time)
            .and_local_timezone(New_York)
            .single()
        {
            self.rth_open_ms = Some(rth_open.timestamp_millis());
        }
        if let Some(rth_close) = session_date
            .and_time(self.rth_close_time)
            .and_local_timezone(New_York)
            .single()
        {
            self.rth_close_ms = Some(rth_close.timestamp_millis());
        }
    }

    /// Check if a date is a weekend
    fn is_weekend(&self, date: NaiveDate) -> bool {
        matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
    }

    // === Feature Getters ===

    /// Is the current time within RTH?
    pub fn is_rth(&self) -> bool {
        self.is_rth
    }

    /// Minutes since RTH open (negative if before open, 0 during ETH)
    pub fn minutes_since_open(&self, timestamp_ms: i64) -> f64 {
        if let Some(rth_open) = self.rth_open_ms {
            if self.is_rth {
                return (timestamp_ms - rth_open) as f64 / 60_000.0;
            }
        }
        0.0
    }

    /// Minutes until RTH close (negative if after close, 0 during ETH)
    pub fn minutes_to_close(&self, timestamp_ms: i64) -> f64 {
        if let Some(rth_close) = self.rth_close_ms {
            if self.is_rth {
                return (rth_close - timestamp_ms) as f64 / 60_000.0;
            }
        }
        0.0
    }

    /// RTH close timestamp for the current session (Eastern), if known
    pub fn rth_close_ms(&self) -> Option<i64> {
        self.rth_close_ms
    }

    /// Day of week (0 = Monday, 4 = Friday)
    /// Uses cached value from session start to avoid timezone conversion
    pub fn day_of_week(&self, _timestamp_ms: i64) -> u8 {
        self.cached_day_of_week
    }

    /// Is this the first hour of RTH? (9:30-10:30 ET)
    pub fn is_first_hour(&self, timestamp_ms: i64) -> bool {
        if !self.is_rth {
            return false;
        }
        self.minutes_since_open(timestamp_ms) <= 60.0
    }

    /// Is this the last hour of RTH? (3:00-4:00 PM ET)
    pub fn is_last_hour(&self, timestamp_ms: i64) -> bool {
        if !self.is_rth {
            return false;
        }
        self.minutes_to_close(timestamp_ms) <= 60.0
    }

    /// Is this Monday open? (first 30 min of Monday RTH)
    pub fn is_monday_open(&self, timestamp_ms: i64) -> bool {
        if !self.is_rth {
            return false;
        }
        self.day_of_week(timestamp_ms) == 0 && self.minutes_since_open(timestamp_ms) <= 30.0
    }

    /// Is this Friday close? (last 30 min of Friday RTH)
    pub fn is_friday_close(&self, timestamp_ms: i64) -> bool {
        if !self.is_rth {
            return false;
        }
        self.day_of_week(timestamp_ms) == 4 && self.minutes_to_close(timestamp_ms) <= 30.0
    }

    /// Distance from session high in ticks
    pub fn dist_from_session_high(&self, price: f64, tick_size: f64) -> f64 {
        if self.session_high > f64::NEG_INFINITY {
            (self.session_high - price) / tick_size
        } else {
            0.0
        }
    }

    /// Distance from session low in ticks
    pub fn dist_from_session_low(&self, price: f64, tick_size: f64) -> f64 {
        if self.session_low < f64::INFINITY {
            (price - self.session_low) / tick_size
        } else {
            0.0
        }
    }

    /// Distance from prior day close in ticks
    pub fn dist_from_prior_close(&self, price: f64, tick_size: f64) -> Option<f64> {
        self.prior_close.map(|pc| (price - pc) / tick_size)
    }

    /// Distance from session open in ticks
    pub fn dist_from_open(&self, price: f64, tick_size: f64) -> Option<f64> {
        self.session_open.map(|open| (price - open) / tick_size)
    }

    /// Position within today's range (0 = at low, 1 = at high)
    pub fn intraday_range_position(&self, price: f64) -> f64 {
        let range = self.session_high - self.session_low;
        if range > 0.0 {
            (price - self.session_low) / range
        } else {
            0.5 // No range yet, assume middle
        }
    }

    /// Get session high
    pub fn session_high(&self) -> f64 {
        self.session_high
    }

    /// Get session low
    pub fn session_low(&self) -> f64 {
        self.session_low
    }

    /// Get session open
    pub fn session_open(&self) -> Option<f64> {
        self.session_open
    }

    /// Get prior close
    pub fn prior_close(&self) -> Option<f64> {
        self.prior_close
    }
}

impl Default for SessionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rth_detection() {
        let mut tracker = SessionTracker::new();

        // 10:00 AM ET on a Monday (RTH)
        // Jan 6, 2025 10:00 AM ET = 1736175600000 ms
        let rth_time = 1736175600000_i64;
        tracker.on_tick(rth_time, 18000.0);
        assert!(tracker.is_rth());

        // 5:00 AM ET (ETH)
        let eth_time = rth_time - 5 * 3600 * 1000; // 5 AM
        let mut tracker2 = SessionTracker::new();
        tracker2.on_tick(eth_time, 18000.0);
        assert!(!tracker2.is_rth());
    }

    #[test]
    fn test_session_high_low() {
        let mut tracker = SessionTracker::new();
        let base_time = 1736175600000_i64;

        tracker.on_tick(base_time, 18000.0);
        tracker.on_tick(base_time + 1000, 18050.0);
        tracker.on_tick(base_time + 2000, 17950.0);
        tracker.on_tick(base_time + 3000, 18025.0);

        assert_eq!(tracker.session_high(), 18050.0);
        assert_eq!(tracker.session_low(), 17950.0);
    }
}
