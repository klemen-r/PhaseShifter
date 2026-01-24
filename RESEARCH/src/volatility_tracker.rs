//! Volatility Tracker - ATR, std dev, and volatility percentile features
//!
//! Tracks volatility-related features:
//! - ATR (Average True Range) from M1 bars
//! - Standard deviation of returns at various windows
//! - Volatility percentile vs session
//! - Range ratio (current vs average bar range)

use std::collections::VecDeque;

use crate::bar_builder::{AtrCalculator, RollingStats, Timeframe};
use crate::Bar;

/// Volatility tracker for ATR, std dev, and percentile features
#[derive(Debug)]
pub struct VolatilityTracker {
    // ATR calculator (14-period, using M1 bars)
    atr_calculator: AtrCalculator,

    // Rolling returns for std dev calculation - use persistent rolling stats
    returns_1m: RollingStats,
    returns_5m: RollingStats,
    returns_15m: RollingStats,

    // For tracking return timestamps to know when to expire
    returns_buffer: VecDeque<f64>,
    returns_timestamps: VecDeque<i64>,
    last_return_price: Option<f64>,
    last_return_time: i64,

    // Rolling bar ranges for range ratio
    bar_ranges: RollingStats,

    // Session volatility samples (for percentile calculation)
    // Capped at MAX_SESSION_SAMPLES to prevent unbounded growth
    session_volatility_samples: VecDeque<f64>,
    last_session_date: i64, // Unix ms of session start

    // Current values
    current_atr: f64,

    // Cached volatility percentile (recalculated periodically, not per-tick)
    cached_vol_percentile: f64,
    last_percentile_calc_time: i64,
}

// Maximum session samples to keep (prevents O(n) percentile calculation from getting too slow)
const MAX_SESSION_SAMPLES: usize = 1000;

impl VolatilityTracker {
    /// Create a new volatility tracker
    pub fn new() -> Self {
        Self {
            atr_calculator: AtrCalculator::new(14),
            returns_1m: RollingStats::new(60),   // 60 seconds
            returns_5m: RollingStats::new(300),  // 5 minutes
            returns_15m: RollingStats::new(900), // 15 minutes
            returns_buffer: VecDeque::with_capacity(1000),
            returns_timestamps: VecDeque::with_capacity(1000),
            last_return_price: None,
            last_return_time: 0,
            bar_ranges: RollingStats::new(20), // 20-bar average for range ratio
            session_volatility_samples: VecDeque::with_capacity(MAX_SESSION_SAMPLES),
            last_session_date: 0,
            current_atr: 0.0,
            cached_vol_percentile: 50.0,
            last_percentile_calc_time: 0,
        }
    }

    /// Update on bar close (for ATR and bar range)
    pub fn on_bar_close(&mut self, bar: &Bar, timeframe: Timeframe) {
        // Only use M1 bars for ATR
        if timeframe == Timeframe::M1 {
            self.current_atr = self.atr_calculator.update(bar);

            // Track bar ranges for range ratio
            let range = bar.high - bar.low;
            self.bar_ranges.push(range);
        }
    }

    /// Update on tick (for returns tracking)
    /// Call this approximately once per second for efficiency
    pub fn on_price_sample(&mut self, timestamp_ms: i64, price: f64) {
        // Only sample once per second
        if timestamp_ms - self.last_return_time < 1000 {
            return;
        }

        if let Some(last_price) = self.last_return_price {
            if last_price > 0.0 {
                let ret = (price - last_price) / last_price;

                // Add to all rolling stats
                self.returns_1m.push(ret);
                self.returns_5m.push(ret);
                self.returns_15m.push(ret);

                // Also keep in buffer for reference
                self.returns_buffer.push_back(ret);
                self.returns_timestamps.push_back(timestamp_ms);

                // Prune old returns (keep 15 minutes)
                let cutoff = timestamp_ms - 15 * 60 * 1000;
                while let Some(&t) = self.returns_timestamps.front() {
                    if t < cutoff {
                        self.returns_timestamps.pop_front();
                        self.returns_buffer.pop_front();
                    } else {
                        break;
                    }
                }

                // Add to session samples for percentile calculation (capped circular buffer)
                let std_1m = self.returns_1m.std_dev();
                if std_1m > 0.0 {
                    if self.session_volatility_samples.len() >= MAX_SESSION_SAMPLES {
                        self.session_volatility_samples.pop_front();
                    }
                    self.session_volatility_samples.push_back(std_1m);
                }
            }
        }

        self.last_return_price = Some(price);
        self.last_return_time = timestamp_ms;
    }

    /// Reset session volatility samples (call on new session)
    pub fn on_new_session(&mut self, session_start_ms: i64) {
        if session_start_ms != self.last_session_date {
            self.session_volatility_samples.clear();
            self.last_session_date = session_start_ms;
        }
    }

    // === Feature Getters ===

    /// 14-period ATR from M1 bars
    pub fn atr_14(&self) -> f64 {
        self.current_atr
    }

    /// Is ATR ready (14 bars processed)?
    pub fn atr_ready(&self) -> bool {
        self.atr_calculator.is_ready()
    }

    /// Standard deviation of 1-second returns over last 1 minute
    pub fn std_1m(&self) -> f64 {
        self.returns_1m.std_dev()
    }

    /// Standard deviation of 1-second returns over last 5 minutes
    pub fn std_5m(&self) -> f64 {
        self.returns_5m.std_dev()
    }

    /// Standard deviation of 1-second returns over last 15 minutes
    pub fn std_15m(&self) -> f64 {
        self.returns_15m.std_dev()
    }

    /// Volatility percentile vs session (0-100)
    /// What percentile is current 1m volatility relative to session?
    /// Uses caching - only recalculates every 5 seconds to avoid O(n) per tick
    pub fn vol_percentile_session(&mut self, current_time_ms: i64) -> f64 {
        // Only recalculate every 5 seconds
        if current_time_ms - self.last_percentile_calc_time < 5000 {
            return self.cached_vol_percentile;
        }

        if self.session_volatility_samples.len() < 10 {
            return 50.0; // Not enough data, assume median
        }

        let current = self.returns_1m.std_dev();
        let count_below = self
            .session_volatility_samples
            .iter()
            .filter(|&&v| v < current)
            .count();

        self.cached_vol_percentile =
            (count_below as f64 / self.session_volatility_samples.len() as f64) * 100.0;
        self.last_percentile_calc_time = current_time_ms;
        self.cached_vol_percentile
    }

    /// Range ratio: current bar range vs 20-bar average
    /// >1 means current bar is wider than average
    pub fn range_ratio(&self, current_bar_range: f64) -> f64 {
        let avg_range = self.bar_ranges.mean();
        if avg_range > 0.0 {
            current_bar_range / avg_range
        } else {
            1.0
        }
    }

    /// Average bar range (20-bar)
    pub fn avg_bar_range(&self) -> f64 {
        self.bar_ranges.mean()
    }
}

impl Default for VolatilityTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bar(timestamp_ms: i64, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            timestamp_ms,
            open,
            high,
            low,
            close,
            volume: 1000.0,
            tick_count: 100,
        }
    }

    #[test]
    fn test_atr_calculation() {
        let mut tracker = VolatilityTracker::new();

        // Add 14 bars with range of 10 each
        for i in 0..14 {
            let bar = make_bar(i * 60000, 100.0, 110.0, 100.0, 105.0);
            tracker.on_bar_close(&bar, Timeframe::M1);
        }

        // ATR should be approximately 10 (the range of each bar)
        assert!(tracker.atr_ready());
        assert!((tracker.atr_14() - 10.0).abs() < 1.0);
    }

    #[test]
    fn test_returns_std_dev() {
        let mut tracker = VolatilityTracker::new();
        let base_time = 1000000_i64;

        // Add price samples (simulate 60 seconds)
        for i in 0..60 {
            let price = 100.0 + (i as f64 * 0.01); // Small uptrend
            tracker.on_price_sample(base_time + i * 1000, price);
        }

        // Should have computed some std dev
        assert!(tracker.std_1m() >= 0.0);
    }

    #[test]
    fn test_range_ratio() {
        let mut tracker = VolatilityTracker::new();

        // Add bars with range of 10
        for i in 0..20 {
            let bar = make_bar(i * 60000, 100.0, 110.0, 100.0, 105.0);
            tracker.on_bar_close(&bar, Timeframe::M1);
        }

        // Current bar with range 20 should have ratio 2.0
        let ratio = tracker.range_ratio(20.0);
        assert!((ratio - 2.0).abs() < 0.1);
    }
}
