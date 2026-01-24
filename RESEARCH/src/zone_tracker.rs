//! Zone Tracker - Tracks price interactions with cluster zones
//!
//! Tracks:
//! - Touch count: how many times price touched a zone
//! - Failed breakouts: price broke through zone but reversed
//! - Time at price: how long price dwelled near a level
//! - Zone strength: composite metric for zone reliability

use std::collections::VecDeque;

/// A zone touch event
#[derive(Debug, Clone, Copy)]
struct ZoneTouch {
    timestamp_ms: i64,
    price: f64,
    zone_low: f64,
    zone_high: f64,
    touch_type: TouchType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TouchType {
    Enter,     // Price entered zone
    Exit,      // Price exited zone
    Rejection, // Price touched zone boundary and reversed
    Breakout,  // Price broke through zone
}

/// Failed breakout event
#[derive(Debug, Clone, Copy)]
struct FailedBreakout {
    timestamp_ms: i64,
    breakout_price: f64,
    reversal_price: f64,
    zone_low: f64,
    zone_high: f64,
    broke_above: bool, // true = broke above zone, false = broke below
}

/// Price dwell tracking for a specific level
#[derive(Debug, Clone, Copy)]
struct DwellSample {
    timestamp_ms: i64,
    price: f64,
}

/// Zone tracker for tracking price interactions with zones
#[derive(Debug)]
pub struct ZoneTracker {
    // Recent zone touches (last 1 hour)
    zone_touches: VecDeque<ZoneTouch>,

    // Failed breakouts (last 1 hour)
    failed_breakouts: VecDeque<FailedBreakout>,

    // Price dwell tracking (last 5 minutes)
    dwell_samples: VecDeque<DwellSample>,

    // Current zone tracking state
    current_zone: Option<(f64, f64)>, // (low, high) of zone we're tracking
    in_zone: bool,
    zone_entry_time: Option<i64>,
    zone_entry_price: Option<f64>,

    // Breakout tracking
    last_breakout_above: Option<(i64, f64)>, // (time, price)
    last_breakout_below: Option<(i64, f64)>,

    // Configuration
    rejection_threshold_ticks: f64, // How far price must move for rejection
    breakout_threshold_ticks: f64,  // How far past zone for breakout
    failed_breakout_window_ms: i64, // Time window for failed breakout detection
}

impl ZoneTracker {
    pub fn new(tick_size: f64) -> Self {
        Self {
            zone_touches: VecDeque::with_capacity(500),
            failed_breakouts: VecDeque::with_capacity(100),
            dwell_samples: VecDeque::with_capacity(3000), // ~5 min at 10 samples/sec
            current_zone: None,
            in_zone: false,
            zone_entry_time: None,
            zone_entry_price: None,
            last_breakout_above: None,
            last_breakout_below: None,
            rejection_threshold_ticks: 4.0 * tick_size,
            breakout_threshold_ticks: 2.0 * tick_size,
            failed_breakout_window_ms: 60_000, // 1 minute
        }
    }

    /// Set the current zone to track
    pub fn set_zone(&mut self, low: f64, high: f64) {
        self.current_zone = Some((low, high));
    }

    /// Clear current zone tracking
    pub fn clear_zone(&mut self) {
        self.current_zone = None;
        self.in_zone = false;
        self.zone_entry_time = None;
        self.zone_entry_price = None;
    }

    /// Process a tick - updates all tracking
    pub fn on_tick(&mut self, timestamp_ms: i64, price: f64) {
        // Add dwell sample
        self.dwell_samples.push_back(DwellSample {
            timestamp_ms,
            price,
        });

        // Expire old data
        self.expire_old_data(timestamp_ms);

        // Track zone interactions if we have a zone
        if let Some((zone_low, zone_high)) = self.current_zone {
            self.track_zone_interaction(timestamp_ms, price, zone_low, zone_high);
        }
    }

    /// Track interaction with the current zone
    fn track_zone_interaction(
        &mut self,
        timestamp_ms: i64,
        price: f64,
        zone_low: f64,
        zone_high: f64,
    ) {
        let was_in_zone = self.in_zone;
        let is_in_zone = price >= zone_low && price <= zone_high;

        // Detect zone entry
        if !was_in_zone && is_in_zone {
            self.in_zone = true;
            self.zone_entry_time = Some(timestamp_ms);
            self.zone_entry_price = Some(price);

            self.zone_touches.push_back(ZoneTouch {
                timestamp_ms,
                price,
                zone_low,
                zone_high,
                touch_type: TouchType::Enter,
            });
        }

        // Detect zone exit
        if was_in_zone && !is_in_zone {
            self.in_zone = false;

            let exited_above = price > zone_high;
            let touch_type = if exited_above {
                TouchType::Breakout
            } else {
                TouchType::Breakout
            };

            self.zone_touches.push_back(ZoneTouch {
                timestamp_ms,
                price,
                zone_low,
                zone_high,
                touch_type,
            });

            // Track breakout for failed breakout detection
            if exited_above {
                self.last_breakout_above = Some((timestamp_ms, price));
            } else {
                self.last_breakout_below = Some((timestamp_ms, price));
            }

            self.zone_entry_time = None;
            self.zone_entry_price = None;
        }

        // Detect failed breakout (price broke out but came back)
        self.detect_failed_breakout(timestamp_ms, price, zone_low, zone_high);
    }

    /// Detect failed breakout patterns
    fn detect_failed_breakout(
        &mut self,
        timestamp_ms: i64,
        price: f64,
        zone_low: f64,
        zone_high: f64,
    ) {
        // Check for failed breakout above
        if let Some((breakout_time, breakout_price)) = self.last_breakout_above {
            if timestamp_ms - breakout_time <= self.failed_breakout_window_ms {
                // If price is now back in or below zone, it's a failed breakout
                if price <= zone_high {
                    self.failed_breakouts.push_back(FailedBreakout {
                        timestamp_ms,
                        breakout_price,
                        reversal_price: price,
                        zone_low,
                        zone_high,
                        broke_above: true,
                    });
                    self.last_breakout_above = None;
                }
            } else {
                // Breakout held, clear tracking
                self.last_breakout_above = None;
            }
        }

        // Check for failed breakout below
        if let Some((breakout_time, breakout_price)) = self.last_breakout_below {
            if timestamp_ms - breakout_time <= self.failed_breakout_window_ms {
                // If price is now back in or above zone, it's a failed breakout
                if price >= zone_low {
                    self.failed_breakouts.push_back(FailedBreakout {
                        timestamp_ms,
                        breakout_price,
                        reversal_price: price,
                        zone_low,
                        zone_high,
                        broke_above: false,
                    });
                    self.last_breakout_below = None;
                }
            } else {
                // Breakout held, clear tracking
                self.last_breakout_below = None;
            }
        }
    }

    /// Expire old data
    fn expire_old_data(&mut self, current_time_ms: i64) {
        let touch_cutoff = current_time_ms - 3600_000; // 1 hour
        let dwell_cutoff = current_time_ms - 300_000; // 5 minutes

        while let Some(front) = self.zone_touches.front() {
            if front.timestamp_ms < touch_cutoff {
                self.zone_touches.pop_front();
            } else {
                break;
            }
        }

        while let Some(front) = self.failed_breakouts.front() {
            if front.timestamp_ms < touch_cutoff {
                self.failed_breakouts.pop_front();
            } else {
                break;
            }
        }

        while let Some(front) = self.dwell_samples.front() {
            if front.timestamp_ms < dwell_cutoff {
                self.dwell_samples.pop_front();
            } else {
                break;
            }
        }
    }

    // === Feature Getters ===

    /// Count of zone touches in last N minutes
    pub fn touch_count(&self, current_time_ms: i64, lookback_minutes: i64) -> u32 {
        let cutoff = current_time_ms - lookback_minutes * 60_000;
        self.zone_touches
            .iter()
            .filter(|t| t.timestamp_ms >= cutoff && t.touch_type == TouchType::Enter)
            .count() as u32
    }

    /// Count of failed breakouts in last N minutes
    pub fn failed_breakout_count(&self, current_time_ms: i64, lookback_minutes: i64) -> u32 {
        let cutoff = current_time_ms - lookback_minutes * 60_000;
        self.failed_breakouts
            .iter()
            .filter(|fb| fb.timestamp_ms >= cutoff)
            .count() as u32
    }

    /// Count of failed breakouts above zone (bearish signal)
    pub fn failed_breakout_above_count(&self, current_time_ms: i64, lookback_minutes: i64) -> u32 {
        let cutoff = current_time_ms - lookback_minutes * 60_000;
        self.failed_breakouts
            .iter()
            .filter(|fb| fb.timestamp_ms >= cutoff && fb.broke_above)
            .count() as u32
    }

    /// Count of failed breakouts below zone (bullish signal)
    pub fn failed_breakout_below_count(&self, current_time_ms: i64, lookback_minutes: i64) -> u32 {
        let cutoff = current_time_ms - lookback_minutes * 60_000;
        self.failed_breakouts
            .iter()
            .filter(|fb| fb.timestamp_ms >= cutoff && !fb.broke_above)
            .count() as u32
    }

    /// Time spent near a price level in last 5 minutes (in milliseconds)
    /// "Near" = within tolerance_ticks of the level
    pub fn time_at_price(&self, level: f64, tolerance: f64, current_time_ms: i64) -> i64 {
        if self.dwell_samples.len() < 2 {
            return 0;
        }

        let mut total_time = 0i64;
        let mut prev_sample: Option<&DwellSample> = None;

        for sample in &self.dwell_samples {
            if let Some(prev) = prev_sample {
                // If price was near level during this interval, add time
                if (prev.price - level).abs() <= tolerance {
                    total_time += sample.timestamp_ms - prev.timestamp_ms;
                }
            }
            prev_sample = Some(sample);
        }

        total_time
    }

    /// Time in current zone visit (if currently in zone)
    pub fn current_zone_dwell_time(&self, current_time_ms: i64) -> Option<i64> {
        self.zone_entry_time.map(|entry| current_time_ms - entry)
    }

    /// Zone strength score: composite metric
    /// Higher = more reliable zone (more touches, more failed breakouts, longer dwell)
    pub fn zone_strength_score(&self, current_time_ms: i64) -> f64 {
        let touches = self.touch_count(current_time_ms, 60) as f64;
        let failed_bos = self.failed_breakout_count(current_time_ms, 60) as f64;

        // Normalize and combine
        let touch_score = (touches / 5.0).min(1.0);
        let failed_bo_score = (failed_bos / 3.0).min(1.0);

        // Failed breakouts are more valuable than simple touches
        touch_score * 0.4 + failed_bo_score * 0.6
    }

    /// Recent failed breakout supports direction
    /// Returns true if recent failed breakout above (good for short) or below (good for long)
    pub fn failed_breakout_supports_long(&self, current_time_ms: i64) -> bool {
        self.failed_breakout_below_count(current_time_ms, 5) > 0
    }

    pub fn failed_breakout_supports_short(&self, current_time_ms: i64) -> bool {
        self.failed_breakout_above_count(current_time_ms, 5) > 0
    }
}

impl Default for ZoneTracker {
    fn default() -> Self {
        Self::new(0.25) // NQ tick size default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_entry_exit() {
        let mut tracker = ZoneTracker::new(0.25);
        tracker.set_zone(100.0, 102.0);

        let base_time = 1000000_i64;

        // Price outside zone
        tracker.on_tick(base_time, 99.0);
        assert!(!tracker.in_zone);

        // Price enters zone
        tracker.on_tick(base_time + 1000, 101.0);
        assert!(tracker.in_zone);
        assert_eq!(tracker.touch_count(base_time + 1000, 60), 1);

        // Price exits zone
        tracker.on_tick(base_time + 2000, 103.0);
        assert!(!tracker.in_zone);
    }

    #[test]
    fn test_failed_breakout() {
        let mut tracker = ZoneTracker::new(0.25);
        tracker.set_zone(100.0, 102.0);

        let base_time = 1000000_i64;

        // Enter zone
        tracker.on_tick(base_time, 101.0);

        // Break above
        tracker.on_tick(base_time + 1000, 103.0);

        // Come back into zone (failed breakout)
        tracker.on_tick(base_time + 2000, 101.5);

        assert_eq!(tracker.failed_breakout_above_count(base_time + 2000, 5), 1);
        assert!(tracker.failed_breakout_supports_short(base_time + 2000));
    }
}
