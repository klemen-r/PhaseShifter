//! Price Tracker - Rolling price history and micro-return features
//!
//! Tracks price history at various time windows and computes:
//! - Micro returns (1s, 5s, 10s, 30s, 1m, 5m)
//! - Tick velocity (ticks per second)
//! - Tick acceleration (change in velocity)
//! - Consecutive tick direction

use std::collections::VecDeque;

/// A price observation with timestamp
#[derive(Debug, Clone, Copy)]
struct PricePoint {
    timestamp_ms: i64,
    price: f64,
}

/// Price tracker for micro-returns and tick velocity
#[derive(Debug)]
pub struct PriceTracker {
    // Price history (sorted by time, newest at back)
    // We keep 5 minutes of history to compute all return windows
    price_history: VecDeque<PricePoint>,

    // Tick timestamps for velocity calculation (last 10 seconds)
    tick_timestamps: VecDeque<i64>,

    // Previous velocity for acceleration calculation
    prev_velocity: f64,
    prev_velocity_time: i64,
    cached_acceleration: f64, // Last computed acceleration (returned between updates)

    // Consecutive tick direction tracking
    last_price: Option<f64>,
    last_direction: Option<bool>, // true = up, false = down
    consecutive_count: u32,

    // Configuration
    max_history_ms: i64,     // 5 minutes in ms
    velocity_window_ms: i64, // 10 seconds for velocity
}

impl PriceTracker {
    /// Create a new price tracker
    pub fn new() -> Self {
        Self {
            price_history: VecDeque::with_capacity(50000), // ~5 min at 150 ticks/sec
            tick_timestamps: VecDeque::with_capacity(2000), // ~10 sec at 150 ticks/sec
            prev_velocity: 0.0,
            prev_velocity_time: 0,
            cached_acceleration: 0.0,
            last_price: None,
            last_direction: None,
            consecutive_count: 0,
            max_history_ms: 5 * 60 * 1000, // 5 minutes
            velocity_window_ms: 10 * 1000, // 10 seconds
        }
    }

    /// Process a tick
    pub fn on_tick(&mut self, timestamp_ms: i64, price: f64) {
        // Update consecutive tick tracking
        if let Some(last) = self.last_price {
            if price > last {
                // Up tick
                if self.last_direction == Some(true) {
                    self.consecutive_count += 1;
                } else {
                    self.consecutive_count = 1;
                    self.last_direction = Some(true);
                }
            } else if price < last {
                // Down tick
                if self.last_direction == Some(false) {
                    self.consecutive_count += 1;
                } else {
                    self.consecutive_count = 1;
                    self.last_direction = Some(false);
                }
            }
            // Equal price doesn't change direction or count
        }
        self.last_price = Some(price);

        // Add to price history
        self.price_history.push_back(PricePoint {
            timestamp_ms,
            price,
        });

        // Add to tick timestamps for velocity
        self.tick_timestamps.push_back(timestamp_ms);

        // Prune old data (keeps buffers bounded to the configured windows)
        self.prune_old_data(timestamp_ms);
    }

    /// Remove data older than max_history_ms
    fn prune_old_data(&mut self, current_time_ms: i64) {
        let cutoff = current_time_ms - self.max_history_ms;
        while let Some(front) = self.price_history.front() {
            if front.timestamp_ms < cutoff {
                self.price_history.pop_front();
            } else {
                break;
            }
        }

        let velocity_cutoff = current_time_ms - self.velocity_window_ms;
        while let Some(&front) = self.tick_timestamps.front() {
            if front < velocity_cutoff {
                self.tick_timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Get price N milliseconds ago (or earliest if not enough history)
    fn price_at_offset(&self, current_time_ms: i64, offset_ms: i64) -> Option<f64> {
        if self.price_history.is_empty() {
            return None;
        }

        let target_time = current_time_ms - offset_ms;

        // Binary search for the price at or just before target_time
        // VecDeque is sorted by time (oldest to newest)
        let len = self.price_history.len();

        // Quick bounds check
        if let Some(front) = self.price_history.front() {
            if target_time < front.timestamp_ms {
                return Some(front.price); // Target is before our history
            }
        }
        if let Some(back) = self.price_history.back() {
            if target_time >= back.timestamp_ms {
                return Some(back.price);
            }
        }

        // Binary search
        let mut lo = 0;
        let mut hi = len;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.price_history[mid].timestamp_ms <= target_time {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        // lo is now the first index where timestamp > target_time
        // We want the element just before that (at or before target)
        if lo > 0 {
            Some(self.price_history[lo - 1].price)
        } else {
            self.price_history.front().map(|p| p.price)
        }
    }

    /// Calculate return over a time window
    fn calculate_return(&self, current_price: f64, current_time_ms: i64, offset_ms: i64) -> f64 {
        if let Some(past_price) = self.price_at_offset(current_time_ms, offset_ms) {
            if past_price > 0.0 {
                return (current_price - past_price) / past_price;
            }
        }
        0.0
    }

    // === Feature Getters ===

    /// Return over last 1 second
    pub fn ret_1s(&self, current_price: f64, current_time_ms: i64) -> f64 {
        self.calculate_return(current_price, current_time_ms, 1000)
    }

    /// Return over last 5 seconds
    pub fn ret_5s(&self, current_price: f64, current_time_ms: i64) -> f64 {
        self.calculate_return(current_price, current_time_ms, 5000)
    }

    /// Return over last 10 seconds
    pub fn ret_10s(&self, current_price: f64, current_time_ms: i64) -> f64 {
        self.calculate_return(current_price, current_time_ms, 10000)
    }

    /// Return over last 30 seconds
    pub fn ret_30s(&self, current_price: f64, current_time_ms: i64) -> f64 {
        self.calculate_return(current_price, current_time_ms, 30000)
    }

    /// Return over last 1 minute
    pub fn ret_1m(&self, current_price: f64, current_time_ms: i64) -> f64 {
        self.calculate_return(current_price, current_time_ms, 60000)
    }

    /// Return over last 5 minutes
    pub fn ret_5m(&self, current_price: f64, current_time_ms: i64) -> f64 {
        self.calculate_return(current_price, current_time_ms, 300000)
    }

    /// Tick velocity (ticks per second over 10-second window)
    pub fn tick_velocity(&self, current_time_ms: i64) -> f64 {
        // Count ticks in last 10 seconds
        let cutoff = current_time_ms - self.velocity_window_ms;
        let tick_count = count_since(&self.tick_timestamps, cutoff);

        // Return ticks per second
        tick_count as f64 / (self.velocity_window_ms as f64 / 1000.0)
    }

    /// Tick acceleration (change in velocity per second)
    /// Returns cached value between updates (every 1 second)
    pub fn tick_acceleration(&mut self, current_time_ms: i64) -> f64 {
        let current_velocity = self.tick_velocity(current_time_ms);

        // Update velocity every second
        let time_diff_ms = current_time_ms - self.prev_velocity_time;
        if time_diff_ms >= 1000 {
            self.cached_acceleration = if time_diff_ms > 0 {
                (current_velocity - self.prev_velocity) / (time_diff_ms as f64 / 1000.0)
            } else {
                0.0
            };

            self.prev_velocity = current_velocity;
            self.prev_velocity_time = current_time_ms;
        }

        // Return cached acceleration (updated every second)
        self.cached_acceleration
    }

    /// Number of consecutive ticks in the same direction
    pub fn consecutive_ticks(&self) -> u32 {
        self.consecutive_count
    }

    /// Last tick direction (true = up, false = down, None = unknown)
    pub fn last_direction(&self) -> Option<bool> {
        self.last_direction
    }

    /// Get current price (last observed)
    pub fn current_price(&self) -> Option<f64> {
        self.last_price
    }
}

impl Default for PriceTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn count_since(timestamps: &VecDeque<i64>, cutoff: i64) -> usize {
    let (front, back) = timestamps.as_slices();
    let count_front = front.len() - front.partition_point(|&t| t < cutoff);
    let count_back = back.len() - back.partition_point(|&t| t < cutoff);
    count_front + count_back
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_returns() {
        let mut tracker = PriceTracker::new();
        let base_time = 1000000_i64;

        // Add prices at different times
        tracker.on_tick(base_time, 100.0);
        tracker.on_tick(base_time + 500, 100.5); // 0.5 sec later
        tracker.on_tick(base_time + 1000, 101.0); // 1 sec later
        tracker.on_tick(base_time + 2000, 102.0); // 2 sec later

        let current_price = 102.0;
        let current_time = base_time + 2000;

        // 1s return: (102 - 101) / 101 ≈ 0.0099
        let ret = tracker.ret_1s(current_price, current_time);
        assert!((ret - 0.0099).abs() < 0.001);
    }

    #[test]
    fn test_consecutive_ticks() {
        let mut tracker = PriceTracker::new();
        let base_time = 1000000_i64;

        tracker.on_tick(base_time, 100.0);
        tracker.on_tick(base_time + 100, 101.0); // up
        tracker.on_tick(base_time + 200, 102.0); // up
        tracker.on_tick(base_time + 300, 103.0); // up

        assert_eq!(tracker.consecutive_ticks(), 3);
        assert_eq!(tracker.last_direction(), Some(true));

        tracker.on_tick(base_time + 400, 102.5); // down - resets
        assert_eq!(tracker.consecutive_ticks(), 1);
        assert_eq!(tracker.last_direction(), Some(false));
    }

    #[test]
    fn test_tick_velocity() {
        let mut tracker = PriceTracker::new();
        let base_time = 1000000_i64;

        // Add 100 ticks over 10 seconds (10 ticks/sec)
        for i in 0..100 {
            tracker.on_tick(base_time + i * 100, 100.0 + i as f64 * 0.01);
        }

        let velocity = tracker.tick_velocity(base_time + 9900);
        // Should be approximately 10 ticks/sec
        assert!((velocity - 10.0).abs() < 1.0);
    }
}
