//! Feature Extractor - Master coordinator for all feature trackers
//!
//! Coordinates all trackers and extracts comprehensive features at zone entry time:
//! - Price action (returns, velocity, consecutive ticks)
//! - Volume (rolling sums, delta, spike ratio)
//! - Volatility (ATR, std dev, percentile)
//! - Session context (RTH/ETH, time of day)
//! - Multi-timeframe alignment

use serde::{Deserialize, Serialize};

use crate::bar_builder::Timeframe;
use crate::mtf_tracker::{MtfTracker, Phase};
use crate::price_tracker::PriceTracker;
use crate::session_tracker::SessionTracker;
use crate::volatility_tracker::VolatilityTracker;
use crate::volume_tracker::VolumeTracker;
use crate::{Bar, EnhancedTick};

/// Comprehensive feature vector for ML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSnapshot {
    // === Price Action Features ===
    pub ret_1s: f64,
    pub ret_5s: f64,
    pub ret_10s: f64,
    pub ret_30s: f64,
    pub ret_1m: f64,
    pub ret_5m: f64,
    pub tick_velocity: f64,
    pub tick_acceleration: f64,
    pub consecutive_ticks: u32,

    // === Session-relative Features ===
    pub dist_from_session_high: f64,
    pub dist_from_session_low: f64,
    pub dist_from_prior_close: Option<f64>,
    pub dist_from_open: Option<f64>,
    pub intraday_range_position: f64,

    // === Volume Features ===
    pub volume_1m: u64,
    pub volume_5m: u64,
    pub volume_spike_ratio: f64,
    pub cumulative_delta_1m: i64,
    pub cumulative_delta_5m: i64,
    pub large_trade_count_1m: u32,

    // === Order Flow / DOM Features ===
    pub delta_flipped_bullish_30s: bool, // Delta crossed from - to + in last 30s
    pub delta_flipped_bearish_30s: bool, // Delta crossed from + to - in last 30s
    pub delta_flipped_bullish_60s: bool, // Delta crossed from - to + in last 60s
    pub delta_flipped_bearish_60s: bool, // Delta crossed from + to - in last 60s
    pub ms_since_delta_flip: Option<i64>, // Time since last flip
    pub absorption_ratio: f64,           // Volume / price range (high = absorption)
    pub volume_30s: u64,                 // Volume in last 30s
    pub price_range_30s: f64,            // Price range in ticks over 30s
    pub delta_exhaustion: f64,           // Delta weakening signal
    pub delta_momentum: i64,             // Delta acceleration
    pub delta_supports_long: bool,       // Composite: delta positive + flip/momentum
    pub delta_supports_short: bool,      // Composite: delta negative + flip/momentum

    // === NEW QUANT FEATURES ===
    pub delta_reversal_magnitude: i64, // Absolute delta swing across flip (contracts)
    pub delta_reversal_normalized: f64, // Reversal magnitude / 5m volume
    pub vwap: f64,                     // Session VWAP
    pub vwap_distance: f64,            // Current price - VWAP
    pub vwap_distance_pct: f64,        // (price - VWAP) / VWAP * 100
    pub volume_price_correlation: f64, // Negative = absorption, Positive = trending
    pub absorption_score: f64,         // Composite absorption metric (0-1)

    // === RESEARCH-BASED FEATURES (from user's research) ===
    // Trade rate / tape speed
    pub tick_rate_1s: f64,       // Ticks per second (current)
    pub tick_rate_5m_avg: f64,   // Average tick rate over 5 min
    pub tick_rate_ratio: f64,    // Current / avg (>1 = speeding up)
    pub tick_rate_change: f64,   // Change from previous second
    pub tape_slowdown: bool,     // Tick rate < 50% of avg
    pub tape_acceleration: bool, // Tick rate > 200% of avg

    // Stacked imbalances (footprint-style)
    pub stacked_bullish_levels: u32, // Consecutive levels with ask > bid
    pub stacked_bearish_levels: u32, // Consecutive levels with bid > ask
    pub imbalance_ratio: f64,        // (ask - bid) / total (-1 to +1)
    pub imbalance_supports_long: bool, // Imbalance favors longs
    pub imbalance_supports_short: bool, // Imbalance favors shorts

    // Order flow entropy
    pub order_flow_entropy: f64, // 0 = one-sided, 1 = mixed
    pub flow_is_one_sided: bool, // Entropy < 0.5 (trending)
    pub flow_is_two_sided: bool, // Entropy > 0.8 (absorption/ranging)

    // Delta divergence
    pub bullish_delta_divergence: bool, // Price low + positive delta
    pub bearish_delta_divergence: bool, // Price high + negative delta
    pub divergence_supports_long: bool, // Bullish divergence present
    pub divergence_supports_short: bool, // Bearish divergence present

    // Volume concentration
    pub volume_concentration: f64, // % of volume in tightest 2-tick range
    pub high_volume_concentration: bool, // >50% concentration

    // Composite signals
    pub absorption_detected: bool, // High vol concentration + two-sided flow
    pub momentum_breakout: bool,   // One-sided flow + tape acceleration

    // === Volatility Features ===
    pub atr_14: f64,
    pub std_1m: f64,
    pub std_5m: f64,
    pub std_15m: f64,
    pub vol_percentile_session: f64,
    pub range_ratio: f64,

    // === Session Context Features ===
    pub is_rth: bool,
    pub minutes_since_open: f64,
    pub minutes_to_close: f64,
    pub day_of_week: u8,
    pub is_first_hour: bool,
    pub is_last_hour: bool,
    pub is_monday_open: bool,
    pub is_friday_close: bool,

    // === Multi-TF Context Features ===
    pub htf_trend: f64,
    pub htf_dm_distance: f64,
    pub tf_alignment: f64,
    pub bullish_scenario_count: usize,
    pub bearish_scenario_count: usize,
}

impl Default for FeatureSnapshot {
    fn default() -> Self {
        Self {
            ret_1s: 0.0,
            ret_5s: 0.0,
            ret_10s: 0.0,
            ret_30s: 0.0,
            ret_1m: 0.0,
            ret_5m: 0.0,
            tick_velocity: 0.0,
            tick_acceleration: 0.0,
            consecutive_ticks: 0,
            dist_from_session_high: 0.0,
            dist_from_session_low: 0.0,
            dist_from_prior_close: None,
            dist_from_open: None,
            intraday_range_position: 0.5,
            volume_1m: 0,
            volume_5m: 0,
            volume_spike_ratio: 1.0,
            cumulative_delta_1m: 0,
            cumulative_delta_5m: 0,
            large_trade_count_1m: 0,
            // Order flow features
            delta_flipped_bullish_30s: false,
            delta_flipped_bearish_30s: false,
            delta_flipped_bullish_60s: false,
            delta_flipped_bearish_60s: false,
            ms_since_delta_flip: None,
            absorption_ratio: 0.0,
            volume_30s: 0,
            price_range_30s: 0.0,
            delta_exhaustion: 0.0,
            delta_momentum: 0,
            delta_supports_long: false,
            delta_supports_short: false,
            // New quant features
            delta_reversal_magnitude: 0,
            delta_reversal_normalized: 0.0,
            vwap: 0.0,
            vwap_distance: 0.0,
            vwap_distance_pct: 0.0,
            volume_price_correlation: 0.0,
            absorption_score: 0.0,
            tick_rate_1s: 0.0,
            tick_rate_5m_avg: 0.0,
            tick_rate_ratio: 1.0,
            tick_rate_change: 0.0,
            tape_slowdown: false,
            tape_acceleration: false,
            stacked_bullish_levels: 0,
            stacked_bearish_levels: 0,
            imbalance_ratio: 0.0,
            imbalance_supports_long: false,
            imbalance_supports_short: false,
            order_flow_entropy: 0.5,
            flow_is_one_sided: false,
            flow_is_two_sided: false,
            bullish_delta_divergence: false,
            bearish_delta_divergence: false,
            divergence_supports_long: false,
            divergence_supports_short: false,
            volume_concentration: 0.0,
            high_volume_concentration: false,
            absorption_detected: false,
            momentum_breakout: false,
            atr_14: 0.0,
            std_1m: 0.0,
            std_5m: 0.0,
            std_15m: 0.0,
            vol_percentile_session: 50.0,
            range_ratio: 1.0,
            is_rth: false,
            minutes_since_open: 0.0,
            minutes_to_close: 0.0,
            day_of_week: 0,
            is_first_hour: false,
            is_last_hour: false,
            is_monday_open: false,
            is_friday_close: false,
            htf_trend: 0.0,
            htf_dm_distance: 0.0,
            tf_alignment: 0.5,
            bullish_scenario_count: 0,
            bearish_scenario_count: 0,
        }
    }
}

/// Master feature extractor coordinating all trackers
#[derive(Debug)]
pub struct FeatureExtractor {
    pub price_tracker: PriceTracker,
    pub session_tracker: SessionTracker,
    pub volume_tracker: VolumeTracker,
    pub volatility_tracker: VolatilityTracker,
    pub mtf_tracker: MtfTracker,

    tick_size: f64,
    last_price_sample_time: i64,
}

impl FeatureExtractor {
    /// Create a new feature extractor for a symbol
    pub fn new(tick_size: f64) -> Self {
        let mut volume_tracker = VolumeTracker::new();
        volume_tracker.set_tick_size(tick_size);
        Self {
            price_tracker: PriceTracker::new(),
            session_tracker: SessionTracker::new(),
            volume_tracker,
            volatility_tracker: VolatilityTracker::new(),
            mtf_tracker: MtfTracker::new(),
            tick_size,
            last_price_sample_time: 0,
        }
    }

    /// Create for NQ/MNQ (tick size 0.25)
    pub fn for_nq() -> Self {
        Self::new(0.25)
    }

    /// Process an enhanced tick - updates all trackers
    pub fn on_tick(&mut self, tick: &EnhancedTick) {
        // Update price tracker
        self.price_tracker.on_tick(tick.timestamp_ms, tick.price);

        // Update session tracker
        self.session_tracker.on_tick(tick.timestamp_ms, tick.price);

        // Update volume tracker
        self.volume_tracker.on_tick(tick);

        // Update volatility tracker (sample once per second)
        if tick.timestamp_ms - self.last_price_sample_time >= 1000 {
            self.volatility_tracker
                .on_price_sample(tick.timestamp_ms, tick.price);
            self.last_price_sample_time = tick.timestamp_ms;
        }
    }

    /// Process a bar close - updates volatility tracker
    pub fn on_bar_close(&mut self, bar: &Bar, timeframe: Timeframe) {
        self.volatility_tracker.on_bar_close(bar, timeframe);
    }

    /// Process a phase update from a scenario
    pub fn on_phase_update(
        &mut self,
        timeframe: Timeframe,
        phase_window: usize,
        is_bullish: bool,
        anchor: f64,
        timestamp_ms: i64,
    ) {
        // Only track <=5m scenarios for confirmation context
        if !matches!(timeframe, Timeframe::S15 | Timeframe::M1 | Timeframe::M5) {
            return;
        }
        let phase = if is_bullish {
            Phase::Bullish
        } else {
            Phase::Bearish
        };
        self.mtf_tracker
            .on_phase_update(timeframe, phase_window, phase, anchor, timestamp_ms);
    }

    /// Extract all features at the current moment
    pub fn extract_features(&mut self, price: f64, timestamp_ms: i64) -> FeatureSnapshot {
        // Get current bar range for range_ratio (use default if not available)
        let current_bar_range = self.volatility_tracker.avg_bar_range();

        FeatureSnapshot {
            // Price Action
            ret_1s: self.price_tracker.ret_1s(price, timestamp_ms),
            ret_5s: self.price_tracker.ret_5s(price, timestamp_ms),
            ret_10s: self.price_tracker.ret_10s(price, timestamp_ms),
            ret_30s: self.price_tracker.ret_30s(price, timestamp_ms),
            ret_1m: self.price_tracker.ret_1m(price, timestamp_ms),
            ret_5m: self.price_tracker.ret_5m(price, timestamp_ms),
            tick_velocity: self.price_tracker.tick_velocity(timestamp_ms),
            tick_acceleration: self.price_tracker.tick_acceleration(timestamp_ms),
            consecutive_ticks: self.price_tracker.consecutive_ticks(),

            // Session-relative
            dist_from_session_high: self
                .session_tracker
                .dist_from_session_high(price, self.tick_size),
            dist_from_session_low: self
                .session_tracker
                .dist_from_session_low(price, self.tick_size),
            dist_from_prior_close: self
                .session_tracker
                .dist_from_prior_close(price, self.tick_size),
            dist_from_open: self.session_tracker.dist_from_open(price, self.tick_size),
            intraday_range_position: self.session_tracker.intraday_range_position(price),

            // Volume
            volume_1m: self.volume_tracker.volume_1m(timestamp_ms),
            volume_5m: self.volume_tracker.volume_5m(timestamp_ms),
            volume_spike_ratio: self.volume_tracker.volume_spike_ratio(timestamp_ms),
            cumulative_delta_1m: self.volume_tracker.cumulative_delta_1m(timestamp_ms),
            cumulative_delta_5m: self.volume_tracker.cumulative_delta_5m(timestamp_ms),
            large_trade_count_1m: self.volume_tracker.large_trade_count_1m(timestamp_ms),

            // Order Flow / DOM features
            delta_flipped_bullish_30s: self.volume_tracker.delta_flipped_bullish(timestamp_ms, 30),
            delta_flipped_bearish_30s: self.volume_tracker.delta_flipped_bearish(timestamp_ms, 30),
            delta_flipped_bullish_60s: self.volume_tracker.delta_flipped_bullish(timestamp_ms, 60),
            delta_flipped_bearish_60s: self.volume_tracker.delta_flipped_bearish(timestamp_ms, 60),
            ms_since_delta_flip: self.volume_tracker.ms_since_delta_flip(timestamp_ms),
            absorption_ratio: self.volume_tracker.absorption_ratio(self.tick_size),
            volume_30s: self.volume_tracker.volume_30s(),
            price_range_30s: self.volume_tracker.price_range_30s(self.tick_size),
            delta_exhaustion: self.volume_tracker.delta_exhaustion(),
            delta_momentum: self.volume_tracker.delta_momentum(),
            delta_supports_long: self.volume_tracker.delta_supports_long(timestamp_ms, 60),
            delta_supports_short: self.volume_tracker.delta_supports_short(timestamp_ms, 60),

            // New quant features
            delta_reversal_magnitude: self.volume_tracker.delta_reversal_magnitude(),
            delta_reversal_normalized: self.volume_tracker.delta_reversal_normalized(),
            vwap: self.volume_tracker.vwap(),
            vwap_distance: self.volume_tracker.vwap_distance(price),
            vwap_distance_pct: self.volume_tracker.vwap_distance_pct(price),
            volume_price_correlation: self.volume_tracker.volume_price_correlation(self.tick_size),
            absorption_score: self.volume_tracker.absorption_score(self.tick_size),
            tick_rate_1s: self.volume_tracker.tick_rate_1s(),
            tick_rate_5m_avg: self.volume_tracker.tick_rate_5m_avg(),
            tick_rate_ratio: self.volume_tracker.tick_rate_ratio(),
            tick_rate_change: self.volume_tracker.tick_rate_change(),
            tape_slowdown: self.volume_tracker.tape_slowdown(),
            tape_acceleration: self.volume_tracker.tape_acceleration(),
            stacked_bullish_levels: self.volume_tracker.stacked_bullish_levels(),
            stacked_bearish_levels: self.volume_tracker.stacked_bearish_levels(),
            imbalance_ratio: self.volume_tracker.imbalance_ratio(),
            imbalance_supports_long: self.volume_tracker.imbalance_supports_long(),
            imbalance_supports_short: self.volume_tracker.imbalance_supports_short(),
            order_flow_entropy: self.volume_tracker.order_flow_entropy(),
            flow_is_one_sided: self.volume_tracker.flow_is_one_sided(),
            flow_is_two_sided: self.volume_tracker.flow_is_two_sided(),
            bullish_delta_divergence: self.volume_tracker.bullish_delta_divergence(),
            bearish_delta_divergence: self.volume_tracker.bearish_delta_divergence(),
            divergence_supports_long: self.volume_tracker.divergence_supports_long(),
            divergence_supports_short: self.volume_tracker.divergence_supports_short(),
            volume_concentration: self.volume_tracker.volume_concentration(),
            high_volume_concentration: self.volume_tracker.high_volume_concentration(),
            absorption_detected: self.volume_tracker.absorption_detected(),
            momentum_breakout: self.volume_tracker.momentum_breakout(),

            // Volatility
            atr_14: self.volatility_tracker.atr_14(),
            std_1m: self.volatility_tracker.std_1m(),
            std_5m: self.volatility_tracker.std_5m(),
            std_15m: self.volatility_tracker.std_15m(),
            vol_percentile_session: self.volatility_tracker.vol_percentile_session(timestamp_ms),
            range_ratio: self.volatility_tracker.range_ratio(current_bar_range),

            // Session Context
            is_rth: self.session_tracker.is_rth(),
            minutes_since_open: self.session_tracker.minutes_since_open(timestamp_ms),
            minutes_to_close: self.session_tracker.minutes_to_close(timestamp_ms),
            day_of_week: self.session_tracker.day_of_week(timestamp_ms),
            is_first_hour: self.session_tracker.is_first_hour(timestamp_ms),
            is_last_hour: self.session_tracker.is_last_hour(timestamp_ms),
            is_monday_open: self.session_tracker.is_monday_open(timestamp_ms),
            is_friday_close: self.session_tracker.is_friday_close(timestamp_ms),

            // Multi-TF Context
            htf_trend: self.mtf_tracker.htf_trend(),
            htf_dm_distance: self.mtf_tracker.htf_dm_distance(price, self.tick_size),
            tf_alignment: self.mtf_tracker.tf_alignment(),
            bullish_scenario_count: self.mtf_tracker.bullish_count(),
            bearish_scenario_count: self.mtf_tracker.bearish_count(),
        }
    }

    /// Get the tick size
    pub fn tick_size(&self) -> f64 {
        self.tick_size
    }

    /// RTH close timestamp for the current session (Eastern), if known
    pub fn rth_close_ms(&self) -> Option<i64> {
        self.session_tracker.rth_close_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick(timestamp_ms: i64, price: f64) -> EnhancedTick {
        EnhancedTick {
            timestamp_ms,
            price,
            volume: 10,
            bid_volume: 5,
            ask_volume: 5,
            num_trades: 1,
            bid: price - 0.25,
            ask: price + 0.25,
        }
    }

    #[test]
    fn test_basic_extraction() {
        let mut extractor = FeatureExtractor::for_nq();
        let base_time = 1736175600000_i64; // Some time during RTH

        // Feed some ticks
        for i in 0..100 {
            let tick = make_tick(base_time + i * 100, 18000.0 + (i as f64 * 0.25));
            extractor.on_tick(&tick);
        }

        // Extract features
        let features = extractor.extract_features(18025.0, base_time + 10000);

        // Should have some non-zero values
        assert!(features.tick_velocity > 0.0);
        assert!(features.consecutive_ticks > 0);
    }

    #[test]
    fn test_mtf_integration() {
        let mut extractor = FeatureExtractor::for_nq();
        let time = 1000000_i64;

        // Add some phase updates
        extractor.on_phase_update(Timeframe::M1, 50, true, 18000.0, time);
        extractor.on_phase_update(Timeframe::M5, 7, true, 18010.0, time);
        extractor.on_phase_update(Timeframe::M5, 20, true, 18050.0, time);

        let features = extractor.extract_features(18025.0, time);

        assert_eq!(features.htf_trend, 1.0); // Bullish
        assert_eq!(features.bullish_scenario_count, 3);
        assert_eq!(features.bearish_scenario_count, 0);
    }
}
