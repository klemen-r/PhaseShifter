//! Volume Tracker - Rolling volume and delta features
//!
//! Tracks volume-related features:
//! - Rolling volume sums (1m, 5m)
//! - Volume spike detection (vs 15m average)
//! - Cumulative delta (buying vs selling pressure)
//! - Large trade detection
//! - Delta flip detection (for DOM/footprint style entries)
//! - Absorption detection (high volume, tight price range)
//! - Delta exhaustion (strong delta weakening)

use std::collections::VecDeque;

use crate::EnhancedTick;

/// A volume observation with timestamp
#[derive(Debug, Clone, Copy)]
struct VolumePoint {
    timestamp_ms: i64,
    volume: u32,
    delta: i32, // positive = buying, negative = selling
}

/// A price observation for absorption detection
#[derive(Debug, Clone, Copy)]
struct PricePoint {
    timestamp_ms: i64,
    price: f64,
    volume: u32,
}

/// Sample for volume-price correlation
#[derive(Debug, Clone, Copy)]
struct PriceVolSample {
    timestamp_ms: i64,
    price_change: f64, // Price change over sample period
    volume: u64,       // Volume over sample period
}

/// Tick timing sample for trade rate calculation
#[derive(Debug, Clone, Copy)]
struct TickTimingSample {
    timestamp_ms: i64,
    tick_count: u32,
}

/// Price level imbalance for stacked imbalance detection
#[derive(Debug, Clone, Copy)]
struct PriceLevelImbalance {
    price_level: i64, // Price in ticks (for grouping)
    bid_volume: u64,
    ask_volume: u64,
    timestamp_ms: i64,
}

/// Delta flip state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeltaSign {
    Positive,
    Negative,
    Neutral,
}

/// Volume tracker for rolling volume and delta features
///
/// Uses separate queues for each time window with O(1) amortized sum maintenance.
/// When data expires from a window, we subtract it from the running sum.
#[derive(Debug)]
pub struct VolumeTracker {
    // Separate queues for each time window (enables O(1) expiration per window)
    queue_1m: VecDeque<VolumePoint>,
    queue_5m: VecDeque<VolumePoint>,
    queue_15m: VecDeque<VolumePoint>,

    // Running sums (add on tick, subtract on expire - O(1) operations)
    sum_volume_1m: u64,
    sum_volume_5m: u64,
    sum_volume_15m: u64,
    sum_delta_1m: i64,
    sum_delta_5m: i64,

    // Large trade tracking
    large_trades_1m: VecDeque<i64>,
    large_trade_threshold: u32,

    // === Delta Flip Detection ===
    // Track when 5m delta crosses zero
    prev_delta_5m_sign: DeltaSign,
    delta_flip_time_ms: Option<i64>,
    delta_flip_direction: Option<DeltaSign>, // What it flipped TO
    delta_at_flip: i64,                      // Delta value just before flip (for magnitude)
    delta_after_flip: i64,                   // Delta value just after flip

    // === Absorption Detection ===
    // High volume with tight price range = absorption
    price_queue_30s: VecDeque<PricePoint>,
    sum_volume_30s: u64,

    // === Delta Exhaustion Detection ===
    // Track rolling delta over different windows to detect weakening
    prev_delta_1m: i64,        // Previous 1m delta for comparison
    delta_sample_time_ms: i64, // When we last sampled delta

    // === Volume-Price Correlation ===
    // Track volume-weighted price for correlation calculation
    vwap_sum_pv: f64,        // Sum of price * volume
    vwap_sum_v: u64,         // Sum of volume
    vwap_session_start: i64, // When current session started

    // For correlation: track recent price changes vs volume
    price_vol_samples: VecDeque<PriceVolSample>,

    // === Trade Rate / Tape Speed ===
    tick_timestamps: VecDeque<i64>, // Individual tick times for rate calc
    tick_rate_samples: VecDeque<TickTimingSample>, // 1-second tick counts
    prev_tick_rate_1s: f64,         // Previous second's tick rate (for slowdown detection)

    // === Stacked Imbalances ===
    price_level_imbalances: VecDeque<PriceLevelImbalance>, // Recent price level bid/ask volume
    tick_size_for_levels: f64,                             // Tick size for price level grouping

    // === Order Flow Entropy ===
    recent_deltas: VecDeque<i32>, // Recent per-tick deltas for entropy calc

    // === Delta Divergence ===
    price_at_delta_sample: f64,   // Price when we last sampled delta
    delta_at_price_sample: i64,   // Delta when we sampled
    price_high_since_sample: f64, // Highest price since sample
    price_low_since_sample: f64,  // Lowest price since sample
}

impl VolumeTracker {
    /// Create a new volume tracker
    pub fn new() -> Self {
        Self {
            // Capacity estimates: 150 ticks/sec * 60/300/900 sec
            queue_1m: VecDeque::with_capacity(10000),
            queue_5m: VecDeque::with_capacity(50000),
            queue_15m: VecDeque::with_capacity(150000),
            sum_volume_1m: 0,
            sum_volume_5m: 0,
            sum_volume_15m: 0,
            sum_delta_1m: 0,
            sum_delta_5m: 0,
            large_trades_1m: VecDeque::with_capacity(1000),
            large_trade_threshold: 50, // 50+ contracts = large trade for MNQ

            // Delta flip detection
            prev_delta_5m_sign: DeltaSign::Neutral,
            delta_flip_time_ms: None,
            delta_flip_direction: None,
            delta_at_flip: 0,
            delta_after_flip: 0,

            // Absorption detection
            price_queue_30s: VecDeque::with_capacity(5000),
            sum_volume_30s: 0,

            // Delta exhaustion
            prev_delta_1m: 0,
            delta_sample_time_ms: 0,

            // VWAP / Volume-price correlation
            vwap_sum_pv: 0.0,
            vwap_sum_v: 0,
            vwap_session_start: 0,
            price_vol_samples: VecDeque::with_capacity(300), // 5 min of 1-sec samples

            // Trade rate / tape speed
            tick_timestamps: VecDeque::with_capacity(10000),
            tick_rate_samples: VecDeque::with_capacity(300),
            prev_tick_rate_1s: 0.0,

            // Stacked imbalances
            price_level_imbalances: VecDeque::with_capacity(5000),
            tick_size_for_levels: 0.25, // Default NQ tick size

            // Order flow entropy
            recent_deltas: VecDeque::with_capacity(1000),

            // Delta divergence
            price_at_delta_sample: 0.0,
            delta_at_price_sample: 0,
            price_high_since_sample: 0.0,
            price_low_since_sample: f64::MAX,
        }
    }

    /// Create with custom large trade threshold
    pub fn with_threshold(threshold: u32) -> Self {
        let mut tracker = Self::new();
        tracker.large_trade_threshold = threshold;
        tracker
    }

    /// Process an enhanced tick - O(1) amortized
    pub fn on_tick(&mut self, tick: &EnhancedTick) {
        let delta = tick.delta();
        let point = VolumePoint {
            timestamp_ms: tick.timestamp_ms,
            volume: tick.volume,
            delta,
        };

        // Add to all queues and update sums (O(1))
        self.queue_1m.push_back(point);
        self.queue_5m.push_back(point);
        self.queue_15m.push_back(point);

        self.sum_volume_1m += tick.volume as u64;
        self.sum_volume_5m += tick.volume as u64;
        self.sum_volume_15m += tick.volume as u64;
        self.sum_delta_1m += delta as i64;
        self.sum_delta_5m += delta as i64;

        // Track large trades
        if tick.volume >= self.large_trade_threshold {
            self.large_trades_1m.push_back(tick.timestamp_ms);
        }

        // Add to absorption queue
        let price_point = PricePoint {
            timestamp_ms: tick.timestamp_ms,
            price: tick.price,
            volume: tick.volume,
        };
        self.price_queue_30s.push_back(price_point);
        self.sum_volume_30s += tick.volume as u64;

        // Update VWAP tracking
        self.vwap_sum_pv += tick.price * tick.volume as f64;
        self.vwap_sum_v += tick.volume as u64;

        // Track tick timestamps for trade rate
        self.tick_timestamps.push_back(tick.timestamp_ms);

        // Track price level imbalances for stacked imbalance detection
        let price_level = (tick.price / self.tick_size_for_levels).round() as i64;
        self.price_level_imbalances.push_back(PriceLevelImbalance {
            price_level,
            bid_volume: tick.bid_volume as u64,
            ask_volume: tick.ask_volume as u64,
            timestamp_ms: tick.timestamp_ms,
        });

        // Track deltas for entropy calculation
        self.recent_deltas.push_back(delta);

        // Update price extremes for delta divergence
        self.price_high_since_sample = self.price_high_since_sample.max(tick.price);
        self.price_low_since_sample = self.price_low_since_sample.min(tick.price);

        // Expire old data from each queue (O(k) where k is expired count)
        self.expire_old_data(tick.timestamp_ms);

        // Check for delta flip after expiration (so we have current delta)
        self.check_delta_flip(tick.timestamp_ms);

        // Sample delta periodically for exhaustion detection (every 10 seconds)
        if tick.timestamp_ms - self.delta_sample_time_ms >= 10_000 {
            self.prev_delta_1m = self.sum_delta_1m;
            self.delta_sample_time_ms = tick.timestamp_ms;

            // Also sample for delta divergence detection
            self.price_at_delta_sample = tick.price;
            self.delta_at_price_sample = self.sum_delta_5m;
            self.price_high_since_sample = tick.price;
            self.price_low_since_sample = tick.price;
        }

        // Update trade rate samples (every second)
        self.update_tick_rate_samples(tick.timestamp_ms);
    }

    /// Update tick rate samples for tape speed calculation
    fn update_tick_rate_samples(&mut self, current_time_ms: i64) {
        // Count ticks in the last second
        let cutoff_1s = current_time_ms - 1000;
        let tick_count = self
            .tick_timestamps
            .iter()
            .filter(|&&t| t >= cutoff_1s)
            .count() as u32;

        // Sample once per second
        if let Some(last_sample) = self.tick_rate_samples.back() {
            if current_time_ms - last_sample.timestamp_ms >= 1000 {
                self.prev_tick_rate_1s = last_sample.tick_count as f64;
                self.tick_rate_samples.push_back(TickTimingSample {
                    timestamp_ms: current_time_ms,
                    tick_count,
                });
            }
        } else {
            self.tick_rate_samples.push_back(TickTimingSample {
                timestamp_ms: current_time_ms,
                tick_count,
            });
        }
    }

    /// Expire data from each queue independently - O(k) where k is expired count
    fn expire_old_data(&mut self, current_time_ms: i64) {
        let cutoff_1m = current_time_ms - 60 * 1000;
        let cutoff_5m = current_time_ms - 5 * 60 * 1000;
        let cutoff_15m = current_time_ms - 15 * 60 * 1000;
        let cutoff_30s = current_time_ms - 30 * 1000;

        // Expire from 1m queue
        while let Some(front) = self.queue_1m.front() {
            if front.timestamp_ms < cutoff_1m {
                let removed = self.queue_1m.pop_front().unwrap();
                self.sum_volume_1m = self.sum_volume_1m.saturating_sub(removed.volume as u64);
                self.sum_delta_1m -= removed.delta as i64;
            } else {
                break;
            }
        }

        // Expire from 5m queue
        while let Some(front) = self.queue_5m.front() {
            if front.timestamp_ms < cutoff_5m {
                let removed = self.queue_5m.pop_front().unwrap();
                self.sum_volume_5m = self.sum_volume_5m.saturating_sub(removed.volume as u64);
                self.sum_delta_5m -= removed.delta as i64;
            } else {
                break;
            }
        }

        // Expire from 15m queue
        while let Some(front) = self.queue_15m.front() {
            if front.timestamp_ms < cutoff_15m {
                let removed = self.queue_15m.pop_front().unwrap();
                self.sum_volume_15m = self.sum_volume_15m.saturating_sub(removed.volume as u64);
            } else {
                break;
            }
        }

        // Expire large trades
        while let Some(&front) = self.large_trades_1m.front() {
            if front < cutoff_1m {
                self.large_trades_1m.pop_front();
            } else {
                break;
            }
        }

        // Expire from absorption queue (30s window)
        while let Some(front) = self.price_queue_30s.front() {
            if front.timestamp_ms < cutoff_30s {
                let removed = self.price_queue_30s.pop_front().unwrap();
                self.sum_volume_30s = self.sum_volume_30s.saturating_sub(removed.volume as u64);
            } else {
                break;
            }
        }

        // Expire tick timestamps (10s window for rate calculation)
        let cutoff_10s = current_time_ms - 10 * 1000;
        while let Some(&front) = self.tick_timestamps.front() {
            if front < cutoff_10s {
                self.tick_timestamps.pop_front();
            } else {
                break;
            }
        }

        // Expire tick rate samples (5min window)
        while let Some(front) = self.tick_rate_samples.front() {
            if front.timestamp_ms < cutoff_5m {
                self.tick_rate_samples.pop_front();
            } else {
                break;
            }
        }

        // Expire price level imbalances (30s window)
        while let Some(front) = self.price_level_imbalances.front() {
            if front.timestamp_ms < cutoff_30s {
                self.price_level_imbalances.pop_front();
            } else {
                break;
            }
        }

        // Expire recent deltas (1m window)
        // We don't have timestamps per delta, so use a size limit instead
        while self.recent_deltas.len() > 5000 {
            self.recent_deltas.pop_front();
        }
    }

    /// Check if delta has flipped and record the flip
    fn check_delta_flip(&mut self, current_time_ms: i64) {
        let current_sign = if self.sum_delta_5m > 0 {
            DeltaSign::Positive
        } else if self.sum_delta_5m < 0 {
            DeltaSign::Negative
        } else {
            DeltaSign::Neutral
        };

        // Detect flip (ignore transitions to/from Neutral to avoid noise)
        if current_sign != self.prev_delta_5m_sign
            && current_sign != DeltaSign::Neutral
            && self.prev_delta_5m_sign != DeltaSign::Neutral
        {
            // Record the delta value just before flip (for magnitude calculation)
            self.delta_at_flip = self.sum_delta_5m;
            self.delta_flip_time_ms = Some(current_time_ms);
            self.delta_flip_direction = Some(current_sign);
        }

        // Track delta after flip for magnitude
        if let Some(flip_time) = self.delta_flip_time_ms {
            if current_time_ms - flip_time < 5000 {
                // Within 5 seconds of flip, track the new delta
                self.delta_after_flip = self.sum_delta_5m;
            }
        }

        self.prev_delta_5m_sign = current_sign;
    }

    // === Feature Getters (O(1) using cached sums) ===

    /// Rolling volume over last 1 minute
    pub fn volume_1m(&self, _current_time_ms: i64) -> u64 {
        self.sum_volume_1m
    }

    /// Rolling volume over last 5 minutes
    pub fn volume_5m(&self, _current_time_ms: i64) -> u64 {
        self.sum_volume_5m
    }

    /// Volume spike ratio: current 1m volume vs 15m average per minute
    pub fn volume_spike_ratio(&self, _current_time_ms: i64) -> f64 {
        let vol_1m = self.sum_volume_1m as f64;

        // 15-minute average per minute
        let avg_1m = self.sum_volume_15m as f64 / 15.0;

        if avg_1m > 0.0 {
            vol_1m / avg_1m
        } else {
            1.0
        }
    }

    /// Cumulative delta over last 1 minute (positive = net buying)
    pub fn cumulative_delta_1m(&self, _current_time_ms: i64) -> i64 {
        self.sum_delta_1m
    }

    /// Cumulative delta over last 5 minutes
    pub fn cumulative_delta_5m(&self, _current_time_ms: i64) -> i64 {
        self.sum_delta_5m
    }

    /// Count of large trades in last 1 minute
    pub fn large_trade_count_1m(&self, current_time_ms: i64) -> u32 {
        let cutoff = current_time_ms - 60 * 1000;
        count_since(&self.large_trades_1m, cutoff) as u32
    }

    /// Get the large trade threshold
    pub fn large_trade_threshold(&self) -> u32 {
        self.large_trade_threshold
    }

    /// Set the large trade threshold
    pub fn set_large_trade_threshold(&mut self, threshold: u32) {
        self.large_trade_threshold = threshold;
    }

    // === Delta Flip Features ===

    /// Returns true if delta flipped bullish (from negative to positive) within last N seconds
    pub fn delta_flipped_bullish(&self, current_time_ms: i64, lookback_seconds: i64) -> bool {
        if let (Some(flip_time), Some(direction)) =
            (self.delta_flip_time_ms, self.delta_flip_direction)
        {
            let cutoff = current_time_ms - lookback_seconds * 1000;
            flip_time >= cutoff && direction == DeltaSign::Positive
        } else {
            false
        }
    }

    /// Returns true if delta flipped bearish (from positive to negative) within last N seconds
    pub fn delta_flipped_bearish(&self, current_time_ms: i64, lookback_seconds: i64) -> bool {
        if let (Some(flip_time), Some(direction)) =
            (self.delta_flip_time_ms, self.delta_flip_direction)
        {
            let cutoff = current_time_ms - lookback_seconds * 1000;
            flip_time >= cutoff && direction == DeltaSign::Negative
        } else {
            false
        }
    }

    /// Time since last delta flip in milliseconds (None if no flip recorded)
    pub fn ms_since_delta_flip(&self, current_time_ms: i64) -> Option<i64> {
        self.delta_flip_time_ms
            .map(|flip_time| current_time_ms - flip_time)
    }

    /// Current delta sign
    pub fn current_delta_sign(&self) -> DeltaSign {
        self.prev_delta_5m_sign
    }

    // === Absorption Detection Features ===

    /// Absorption ratio: volume relative to price range in last 30s
    /// High ratio = lots of volume absorbed in tight range (potential reversal)
    pub fn absorption_ratio(&self, tick_size: f64) -> f64 {
        if self.price_queue_30s.len() < 2 || self.sum_volume_30s == 0 {
            return 0.0;
        }

        // Find price range in 30s window
        let mut min_price = f64::MAX;
        let mut max_price = f64::MIN;
        for point in &self.price_queue_30s {
            min_price = min_price.min(point.price);
            max_price = max_price.max(point.price);
        }

        let range_ticks = ((max_price - min_price) / tick_size).max(1.0);
        // Volume per tick of range - high = absorption
        self.sum_volume_30s as f64 / range_ticks
    }

    /// Detects absorption: high volume relative to typical, but tight price range
    /// Returns true if absorption_ratio is above 2x the historical average
    pub fn is_absorbing(&self, tick_size: f64, avg_volume_per_tick: f64) -> bool {
        if avg_volume_per_tick <= 0.0 {
            return false;
        }
        let ratio = self.absorption_ratio(tick_size);
        ratio > avg_volume_per_tick * 2.0
    }

    /// Volume in last 30 seconds
    pub fn volume_30s(&self) -> u64 {
        self.sum_volume_30s
    }

    /// Price range in last 30 seconds (in ticks)
    pub fn price_range_30s(&self, tick_size: f64) -> f64 {
        if self.price_queue_30s.len() < 2 {
            return 0.0;
        }

        let mut min_price = f64::MAX;
        let mut max_price = f64::MIN;
        for point in &self.price_queue_30s {
            min_price = min_price.min(point.price);
            max_price = max_price.max(point.price);
        }

        (max_price - min_price) / tick_size
    }

    // === Delta Exhaustion Features ===

    /// Delta exhaustion: detects when strong delta is weakening
    /// Positive = bullish delta weakening, Negative = bearish delta weakening
    /// Magnitude indicates how much weaker current delta is vs 10s ago
    pub fn delta_exhaustion(&self) -> f64 {
        // Compare current 1m delta to what it was 10s ago
        // If delta was +500 and now +200, exhaustion = +300 (bullish exhaustion)
        // If delta was -500 and now -200, exhaustion = -300 (bearish exhaustion)
        (self.prev_delta_1m - self.sum_delta_1m) as f64
    }

    /// Returns true if bullish delta is exhausting (was strongly positive, now weaker)
    pub fn is_bullish_exhausting(&self, threshold: i64) -> bool {
        self.prev_delta_1m > threshold && self.sum_delta_1m < self.prev_delta_1m
    }

    /// Returns true if bearish delta is exhausting (was strongly negative, now weaker)
    pub fn is_bearish_exhausting(&self, threshold: i64) -> bool {
        self.prev_delta_1m < -threshold && self.sum_delta_1m > self.prev_delta_1m
    }

    /// Delta momentum: current delta vs previous sample
    /// Positive = delta getting more positive (buying accelerating)
    /// Negative = delta getting more negative (selling accelerating)
    pub fn delta_momentum(&self) -> i64 {
        self.sum_delta_1m - self.prev_delta_1m
    }

    // === Combined Entry Signals ===

    /// Delta supports long entry: delta positive AND (flipped bullish recently OR bullish acceleration)
    pub fn delta_supports_long(&self, current_time_ms: i64, flip_lookback_seconds: i64) -> bool {
        if self.sum_delta_5m <= 0 {
            return false;
        }
        // Either: recent bullish flip OR positive delta momentum
        self.delta_flipped_bullish(current_time_ms, flip_lookback_seconds)
            || self.delta_momentum() > 0
    }

    /// Delta supports short entry: delta negative AND (flipped bearish recently OR bearish acceleration)
    pub fn delta_supports_short(&self, current_time_ms: i64, flip_lookback_seconds: i64) -> bool {
        if self.sum_delta_5m >= 0 {
            return false;
        }
        // Either: recent bearish flip OR negative delta momentum
        self.delta_flipped_bearish(current_time_ms, flip_lookback_seconds)
            || self.delta_momentum() < 0
    }

    // === NEW QUANT FEATURES ===

    /// Delta reversal magnitude: how strong was the delta flip
    /// Returns the absolute change in delta across the flip (contracts)
    /// Higher = stronger reversal signal
    pub fn delta_reversal_magnitude(&self) -> i64 {
        // Magnitude is the swing from before to after the flip
        // e.g., if delta went from -500 to +300, magnitude = 800
        (self.delta_after_flip - self.delta_at_flip).abs()
    }

    /// Normalized delta reversal (relative to recent volume)
    /// Returns magnitude as a ratio of 5m volume (0.0 to ~2.0 typically)
    pub fn delta_reversal_normalized(&self) -> f64 {
        if self.sum_volume_5m == 0 {
            return 0.0;
        }
        self.delta_reversal_magnitude() as f64 / self.sum_volume_5m as f64
    }

    /// Volume-weighted average price (VWAP) for the session
    pub fn vwap(&self) -> f64 {
        if self.vwap_sum_v == 0 {
            return 0.0;
        }
        self.vwap_sum_pv / self.vwap_sum_v as f64
    }

    /// Distance from VWAP in price terms
    /// Positive = above VWAP, Negative = below VWAP
    pub fn vwap_distance(&self, current_price: f64) -> f64 {
        let vwap = self.vwap();
        if vwap == 0.0 {
            return 0.0;
        }
        current_price - vwap
    }

    /// Distance from VWAP as percentage
    pub fn vwap_distance_pct(&self, current_price: f64) -> f64 {
        let vwap = self.vwap();
        if vwap == 0.0 {
            return 0.0;
        }
        (current_price - vwap) / vwap * 100.0
    }

    /// Volume-price correlation over last 30s
    /// Negative correlation = absorption (high volume, tight range)
    /// Positive correlation = trending (volume confirms price move)
    pub fn volume_price_correlation(&self, tick_size: f64) -> f64 {
        if self.price_queue_30s.len() < 10 {
            return 0.0;
        }

        // Calculate price range and volume
        let mut min_price = f64::MAX;
        let mut max_price = f64::MIN;
        let mut total_volume = 0u64;

        for point in &self.price_queue_30s {
            min_price = min_price.min(point.price);
            max_price = max_price.max(point.price);
            total_volume += point.volume as u64;
        }

        let range_ticks = (max_price - min_price) / tick_size;
        if range_ticks < 1.0 || total_volume == 0 {
            return 0.0;
        }

        // Efficiency ratio: how much price moved per unit volume
        // Low efficiency = absorption (lots of volume, little movement)
        // High efficiency = trending (volume drives price)
        let efficiency = range_ticks / (total_volume as f64 / 1000.0); // Normalize by 1000 contracts

        // Convert to correlation-like metric (-1 to +1 range)
        // Below 1.0 efficiency = negative (absorption)
        // Above 1.0 efficiency = positive (trending)
        (efficiency - 1.0).clamp(-1.0, 1.0)
    }

    /// Absorption score: composite metric for absorption detection
    /// Higher = more absorption (good for reversal entries)
    /// Combines: high volume + tight range + delta exhaustion
    pub fn absorption_score(&self, tick_size: f64) -> f64 {
        let absorption = self.absorption_ratio(tick_size);
        let exhaustion = self.delta_exhaustion().abs();
        let vol_spike = self.volume_spike_ratio(0); // timestamp not used

        // Normalize components and combine
        // absorption_ratio typically 0-500, normalize to 0-1
        let norm_absorption = (absorption / 200.0).min(1.0);
        // exhaustion typically 0-1000, normalize to 0-1
        let norm_exhaustion = (exhaustion / 500.0).min(1.0);
        // vol_spike typically 0.5-3.0, convert to 0-1
        let norm_vol = ((vol_spike - 0.5) / 2.5).clamp(0.0, 1.0);

        // Weighted combination
        norm_absorption * 0.5 + norm_exhaustion * 0.3 + norm_vol * 0.2
    }

    // ==========================================================================
    // === NEW RESEARCH-BASED FEATURES (Trade Rate, Imbalances, Entropy, etc.) ===
    // ==========================================================================

    /// Trade rate: ticks per second in last 1 second
    /// High = fast tape, low = slow tape
    pub fn tick_rate_1s(&self) -> f64 {
        if let Some(sample) = self.tick_rate_samples.back() {
            sample.tick_count as f64
        } else {
            0.0
        }
    }

    /// Average tick rate over last 5 minutes
    pub fn tick_rate_5m_avg(&self) -> f64 {
        if self.tick_rate_samples.is_empty() {
            return 0.0;
        }
        let sum: u32 = self.tick_rate_samples.iter().map(|s| s.tick_count).sum();
        sum as f64 / self.tick_rate_samples.len() as f64
    }

    /// Tick rate ratio: current rate vs 5m average
    /// >1.0 = tape speeding up, <1.0 = tape slowing down
    pub fn tick_rate_ratio(&self) -> f64 {
        let avg = self.tick_rate_5m_avg();
        if avg <= 0.0 {
            return 1.0;
        }
        self.tick_rate_1s() / avg
    }

    /// Tape slowdown: true if current tick rate is significantly below average
    /// Slowdown at key levels often precedes reversals
    pub fn tape_slowdown(&self) -> bool {
        self.tick_rate_ratio() < 0.5
    }

    /// Tape acceleration: true if current tick rate is significantly above average
    pub fn tape_acceleration(&self) -> bool {
        self.tick_rate_ratio() > 2.0
    }

    /// Change in tick rate from previous second (for momentum)
    pub fn tick_rate_change(&self) -> f64 {
        self.tick_rate_1s() - self.prev_tick_rate_1s
    }

    // === Stacked Imbalances ===

    /// Count of consecutive price levels with bullish imbalance (ask > bid)
    /// High count = stacked buying, supports long
    pub fn stacked_bullish_levels(&self) -> u32 {
        self.count_stacked_imbalances(true)
    }

    /// Count of consecutive price levels with bearish imbalance (bid > ask)
    /// High count = stacked selling, supports short
    pub fn stacked_bearish_levels(&self) -> u32 {
        self.count_stacked_imbalances(false)
    }

    /// Helper to count consecutive imbalanced levels
    fn count_stacked_imbalances(&self, bullish: bool) -> u32 {
        use std::collections::HashMap;

        if self.price_level_imbalances.is_empty() {
            return 0;
        }

        // Aggregate volume by price level
        let mut level_volumes: HashMap<i64, (u64, u64)> = HashMap::new();
        for imb in &self.price_level_imbalances {
            let entry = level_volumes.entry(imb.price_level).or_insert((0, 0));
            entry.0 += imb.bid_volume;
            entry.1 += imb.ask_volume;
        }

        // Sort levels
        let mut levels: Vec<_> = level_volumes.into_iter().collect();
        levels.sort_by_key(|(level, _)| *level);

        // Count consecutive imbalanced levels
        let mut max_stack = 0u32;
        let mut current_stack = 0u32;

        for (_level, (bid, ask)) in &levels {
            let is_imbalanced = if bullish {
                *ask > *bid * 3 / 2 // Ask volume > 1.5x bid = bullish imbalance
            } else {
                *bid > *ask * 3 / 2 // Bid volume > 1.5x ask = bearish imbalance
            };

            if is_imbalanced {
                current_stack += 1;
                max_stack = max_stack.max(current_stack);
            } else {
                current_stack = 0;
            }
        }

        max_stack
    }

    /// Imbalance ratio: (ask_vol - bid_vol) / total_vol in last 30s
    /// Positive = more aggressive buying, Negative = more aggressive selling
    pub fn imbalance_ratio(&self) -> f64 {
        let mut total_bid = 0u64;
        let mut total_ask = 0u64;

        for imb in &self.price_level_imbalances {
            total_bid += imb.bid_volume;
            total_ask += imb.ask_volume;
        }

        let total = total_bid + total_ask;
        if total == 0 {
            return 0.0;
        }

        (total_ask as f64 - total_bid as f64) / total as f64
    }

    /// Imbalance supports trade direction
    pub fn imbalance_supports_long(&self) -> bool {
        self.imbalance_ratio() > 0.1 // More buying than selling
    }

    pub fn imbalance_supports_short(&self) -> bool {
        self.imbalance_ratio() < -0.1 // More selling than buying
    }

    // === Order Flow Entropy ===

    /// Order flow entropy: measure of how mixed vs one-sided flow is
    /// Low entropy = one-sided (trending), High entropy = mixed (ranging/absorption)
    /// Range: 0.0 (completely one-sided) to 1.0 (perfectly mixed)
    pub fn order_flow_entropy(&self) -> f64 {
        if self.recent_deltas.len() < 10 {
            return 0.5; // Default to neutral
        }

        // Count positive vs negative deltas
        let mut pos_count = 0u64;
        let mut neg_count = 0u64;

        for &delta in &self.recent_deltas {
            if delta > 0 {
                pos_count += 1;
            } else if delta < 0 {
                neg_count += 1;
            }
        }

        let total = pos_count + neg_count;
        if total == 0 {
            return 0.5;
        }

        let p_pos = pos_count as f64 / total as f64;
        let p_neg = neg_count as f64 / total as f64;

        // Binary entropy: -p*log2(p) - (1-p)*log2(1-p)
        // Max at p=0.5 (entropy=1), min at p=0 or p=1 (entropy=0)
        if p_pos <= 0.0 || p_pos >= 1.0 {
            return 0.0;
        }

        -(p_pos * p_pos.log2() + p_neg * p_neg.log2())
    }

    /// True if flow is one-sided (entropy < 0.5)
    pub fn flow_is_one_sided(&self) -> bool {
        self.order_flow_entropy() < 0.5
    }

    /// True if flow is mixed/two-sided (entropy > 0.8)
    pub fn flow_is_two_sided(&self) -> bool {
        self.order_flow_entropy() > 0.8
    }

    // === Delta Divergence ===

    /// Delta divergence: price makes new high but delta is negative (or vice versa)
    /// Bearish divergence: price made new high since sample, but delta is negative
    pub fn bearish_delta_divergence(&self) -> bool {
        if self.price_at_delta_sample <= 0.0 {
            return false;
        }
        // Price made new high but delta is now negative
        self.price_high_since_sample > self.price_at_delta_sample && self.sum_delta_5m < 0
    }

    /// Bullish divergence: price made new low since sample, but delta is positive
    pub fn bullish_delta_divergence(&self) -> bool {
        if self.price_at_delta_sample <= 0.0 {
            return false;
        }
        // Price made new low but delta is now positive
        self.price_low_since_sample < self.price_at_delta_sample && self.sum_delta_5m > 0
    }

    /// Divergence supports trade (bullish div for long, bearish div for short)
    pub fn divergence_supports_long(&self) -> bool {
        self.bullish_delta_divergence()
    }

    pub fn divergence_supports_short(&self) -> bool {
        self.bearish_delta_divergence()
    }

    // === Volume Cluster Detection ===

    /// Volume concentration: what % of 30s volume occurred in the tightest 2-tick range
    /// High concentration = absorption at a level
    pub fn volume_concentration(&self) -> f64 {
        use std::collections::HashMap;

        if self.price_level_imbalances.is_empty() {
            return 0.0;
        }

        // Sum volume by price level
        let mut level_volumes: HashMap<i64, u64> = HashMap::new();
        let mut total_volume = 0u64;

        for imb in &self.price_level_imbalances {
            *level_volumes.entry(imb.price_level).or_insert(0) += imb.bid_volume + imb.ask_volume;
            total_volume += imb.bid_volume + imb.ask_volume;
        }

        if total_volume == 0 {
            return 0.0;
        }

        // Find max volume in any 2 consecutive levels
        let mut levels: Vec<_> = level_volumes.into_iter().collect();
        levels.sort_by_key(|(level, _)| *level);

        let mut max_cluster_vol = 0u64;
        for i in 0..levels.len() {
            let mut cluster_vol = levels[i].1;
            if i + 1 < levels.len() && levels[i + 1].0 == levels[i].0 + 1 {
                cluster_vol += levels[i + 1].1;
            }
            max_cluster_vol = max_cluster_vol.max(cluster_vol);
        }

        max_cluster_vol as f64 / total_volume as f64
    }

    /// High volume concentration (>50% of volume in 2 ticks)
    pub fn high_volume_concentration(&self) -> bool {
        self.volume_concentration() > 0.5
    }

    // === Composite Signals ===

    /// Absorption detected: combines multiple signals
    /// True when: high volume concentration + flow is two-sided + tape slowdown
    pub fn absorption_detected(&self) -> bool {
        self.high_volume_concentration() && self.flow_is_two_sided()
            || self.absorption_score(self.tick_size_for_levels) > 0.6
    }

    /// Momentum breakout detected: one-sided flow + tape acceleration
    pub fn momentum_breakout(&self) -> bool {
        self.flow_is_one_sided() && self.tape_acceleration()
    }

    /// Set tick size for price level grouping
    pub fn set_tick_size(&mut self, tick_size: f64) {
        self.tick_size_for_levels = tick_size;
    }

    /// Reset VWAP for new session
    pub fn reset_vwap(&mut self) {
        self.vwap_sum_pv = 0.0;
        self.vwap_sum_v = 0;
    }

    /// Check if we should reset VWAP (new session detected)
    pub fn maybe_reset_vwap(&mut self, timestamp_ms: i64, is_session_start: bool) {
        if is_session_start && timestamp_ms > self.vwap_session_start {
            self.reset_vwap();
            self.vwap_session_start = timestamp_ms;
        }
    }
}

impl Default for VolumeTracker {
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

    fn make_tick(timestamp_ms: i64, volume: u32, bid_vol: u32, ask_vol: u32) -> EnhancedTick {
        EnhancedTick {
            timestamp_ms,
            price: 18000.0,
            volume,
            bid_volume: bid_vol,
            ask_volume: ask_vol,
            num_trades: 1,
            bid: 17999.75,
            ask: 18000.25,
        }
    }

    #[test]
    fn test_volume_1m() {
        let mut tracker = VolumeTracker::new();
        let base_time = 1000000_i64;

        // Add volume over 2 minutes
        for i in 0..120 {
            let tick = make_tick(base_time + i * 1000, 10, 5, 5);
            tracker.on_tick(&tick);
        }

        // Volume in last minute should be ~60 ticks * 10 vol = 600
        let vol = tracker.volume_1m(base_time + 119 * 1000);
        assert!(vol >= 590 && vol <= 610); // Allow small variance from timing
    }

    #[test]
    fn test_cumulative_delta() {
        let mut tracker = VolumeTracker::new();
        let base_time = 1000000_i64;

        // More buying pressure (ask_volume > bid_volume)
        for i in 0..60 {
            let tick = make_tick(base_time + i * 1000, 10, 2, 8); // delta = +6 each
            tracker.on_tick(&tick);
        }

        let delta = tracker.cumulative_delta_1m(base_time + 59 * 1000);
        assert_eq!(delta, 360); // 60 * 6
    }

    #[test]
    fn test_large_trade_count() {
        let mut tracker = VolumeTracker::with_threshold(50);
        let base_time = 1000000_i64;

        // Mix of small and large trades
        tracker.on_tick(&make_tick(base_time, 10, 5, 5));
        tracker.on_tick(&make_tick(base_time + 1000, 100, 50, 50)); // large
        tracker.on_tick(&make_tick(base_time + 2000, 20, 10, 10));
        tracker.on_tick(&make_tick(base_time + 3000, 75, 35, 40)); // large
        tracker.on_tick(&make_tick(base_time + 4000, 30, 15, 15));

        let count = tracker.large_trade_count_1m(base_time + 4000);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_expiration() {
        let mut tracker = VolumeTracker::new();
        let base_time = 1000000_i64;

        // Add 120 seconds of data
        for i in 0..120 {
            let tick = make_tick(base_time + i * 1000, 10, 5, 5);
            tracker.on_tick(&tick);
        }

        // At t=119s, 1m volume should only include last 60 seconds
        let vol = tracker.volume_1m(base_time + 119 * 1000);
        // Should be ~60 ticks * 10 = 600, not 120 * 10 = 1200
        assert!(vol < 700);
    }

    #[test]
    fn test_volume_spike_ratio_constant_flow() {
        let mut tracker = VolumeTracker::new();
        let base_time = 1000000_i64;
        let ticks = 960_usize; // 16 minutes of 1-second ticks

        for i in 0..ticks {
            let tick = make_tick(base_time + (i as i64) * 1000, 10, 5, 5);
            tracker.on_tick(&tick);
        }

        let current_time = base_time + ((ticks - 1) as i64) * 1000;
        let cutoff_1m = current_time - 60 * 1000;
        let cutoff_15m = current_time - 15 * 60 * 1000;
        let mut count_1m = 0_u64;
        let mut count_15m = 0_u64;

        for i in 0..ticks {
            let ts = base_time + (i as i64) * 1000;
            if ts >= cutoff_1m {
                count_1m += 1;
            }
            if ts >= cutoff_15m {
                count_15m += 1;
            }
        }

        let expected_ratio = (count_1m as f64 * 10.0) / ((count_15m as f64 * 10.0) / 15.0);
        let ratio = tracker.volume_spike_ratio(current_time);
        assert!((ratio - expected_ratio).abs() < 1e-9);
    }
}
