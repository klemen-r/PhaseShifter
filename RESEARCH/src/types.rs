//! Core types for the backtest system

use serde::{Deserialize, Serialize};

/// A single tick from SCID data (basic version)
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    pub timestamp_ms: i64,
    pub price: f64,
    pub volume: u32,
    pub bid: f64,
    pub ask: f64,
}

/// Enhanced tick with all SCID fields preserved for feature extraction
#[derive(Debug, Clone, Copy)]
pub struct EnhancedTick {
    pub timestamp_ms: i64,
    pub price: f64,
    pub volume: u32,
    pub bid_volume: u32,
    pub ask_volume: u32,
    pub num_trades: u32,
    pub bid: f64,
    pub ask: f64,
}

impl EnhancedTick {
    /// Convert to basic Tick (for compatibility with existing code)
    pub fn to_basic(&self) -> Tick {
        Tick {
            timestamp_ms: self.timestamp_ms,
            price: self.price,
            volume: self.volume,
            bid: self.bid,
            ask: self.ask,
        }
    }

    /// Get the signed delta for this tick (positive = buying, negative = selling)
    /// Uses bid/ask volume if available, otherwise estimates from price direction
    pub fn delta(&self) -> i32 {
        if self.bid_volume > 0 || self.ask_volume > 0 {
            // Ask volume = buying (trades at ask), bid volume = selling (trades at bid)
            self.ask_volume as i32 - self.bid_volume as i32
        } else {
            // No directional volume available
            0
        }
    }
}

/// OHLCV bar
#[derive(Debug, Clone, Copy)]
pub struct Bar {
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub tick_count: u32,
}

/// A cluster/zone from multiple scenarios
#[derive(Debug, Clone)]
pub struct Zone {
    pub id: u64,
    pub low: f64,
    pub high: f64,
    pub midpoint: f64,
    pub created_at: i64,
    pub scenario_count: usize,
    pub direction: ZoneDirection,
    /// Scenarios that contributed to this zone (timeframe, phase_window)
    pub scenarios: Vec<(String, usize)>,
    /// Current Donchian Midpoint (DM) - the anchor/equilibrium price
    pub anchor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneDirection {
    /// Zone is above current price (bearish node = expect price to come down to it)
    Above,
    /// Zone is below current price (bullish node = expect price to come up to it)
    Below,
}

/// A zone interaction event - when price touches/enters a zone
#[derive(Debug, Clone)]
pub struct ZoneInteraction {
    pub zone_id: u64,
    pub interaction_id: u64,
    pub timestamp_ms: i64,
    pub entry_price: f64,
    pub zone: Zone,
    pub approach_direction: ApproachDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApproachDirection {
    FromAbove,
    FromBelow,
}

/// Outcome of a zone interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionOutcome {
    /// Price reverted to midpoint before stop
    TargetHit,
    /// Stop hit before target
    StopHit,
    /// Neither hit within time limit
    Expired,
}

/// Feature vector for a zone interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    // Identification
    pub interaction_id: u64,
    pub zone_id: u64,
    pub timestamp_ms: i64,

    // === Zone-relative features ===
    /// Distance from entry price to zone high (ticks)
    pub dist_to_zone_high: f64,
    /// Distance from entry price to zone low (ticks)
    pub dist_to_zone_low: f64,
    /// Distance from entry price to zone midpoint (ticks)
    pub dist_to_zone_mid: f64,
    /// Zone width in ticks
    pub zone_width: f64,
    /// Price position within zone (0 = at low, 1 = at high)
    pub zone_position: f64,
    /// Number of scenarios that created this cluster
    pub scenario_count: f64,
    /// Number of prior taps on this zone
    pub prior_taps: f64,
    /// Bars since zone creation
    pub bars_since_creation: f64,
    /// Zone age in milliseconds
    pub zone_age_ms: f64,
    /// 1 if approaching from above, 0 if from below
    pub approach_from_above: f64,

    // === Price action features (at interaction) ===
    /// Micro return over last 1 second
    pub ret_1s: f64,
    /// Micro return over last 5 seconds
    pub ret_5s: f64,
    /// Micro return over last 10 seconds
    pub ret_10s: f64,
    /// Micro return over last 30 seconds
    pub ret_30s: f64,
    /// Micro return over last 1 minute
    pub ret_1m: f64,
    /// Micro return over last 5 minutes
    pub ret_5m: f64,
    /// Tick velocity (ticks per second, rolling 10s)
    pub tick_velocity: f64,
    /// Change in tick velocity
    pub tick_acceleration: f64,
    /// Consecutive ticks in same direction
    pub consecutive_ticks: f64,
    /// Distance from session high (ticks)
    pub dist_from_session_high: f64,
    /// Distance from session low (ticks)
    pub dist_from_session_low: f64,
    /// Distance from prior day close (ticks)
    pub dist_from_prior_close: f64,
    /// Distance from session open (ticks)
    pub dist_from_open: f64,
    /// Where in today's range (0 = at low, 1 = at high)
    pub intraday_range_position: f64,

    // === Volume features ===
    /// Tick volume at interaction
    pub tick_volume: f64,
    /// Rolling volume over last 1 minute
    pub volume_1m: f64,
    /// Rolling volume over last 5 minutes
    pub volume_5m: f64,
    /// Volume spike ratio (current vs rolling avg)
    pub volume_spike_ratio: f64,
    /// Cumulative delta estimate (upticks - downticks)
    pub cumulative_delta_1m: f64,
    /// Large trade count in last 1 minute
    pub large_trade_count_1m: f64,

    // === Volatility features ===
    /// Rolling ATR over 14 bars (1m)
    pub atr_14: f64,
    /// Rolling standard deviation of returns (1m window)
    pub std_1m: f64,
    /// Rolling standard deviation of returns (5m window)
    pub std_5m: f64,
    /// Rolling standard deviation of returns (15m window)
    pub std_15m: f64,
    /// Volatility percentile vs session
    pub vol_percentile_session: f64,
    /// Range expansion/contraction (current bar range vs avg)
    pub range_ratio: f64,

    // === Session context ===
    /// 1 if RTH, 0 if ETH
    pub is_rth: f64,
    /// Minutes since session open
    pub minutes_since_open: f64,
    /// Minutes to session close
    pub minutes_to_close: f64,
    /// Day of week (0 = Monday, 4 = Friday)
    pub day_of_week: f64,
    /// 1 if first hour of RTH
    pub is_first_hour: f64,
    /// 1 if last hour of RTH
    pub is_last_hour: f64,
    /// 1 if Monday open
    pub is_monday_open: f64,
    /// 1 if Friday close
    pub is_friday_close: f64,

    // === Multi-timeframe context ===
    /// Higher timeframe trend direction (1 = bullish, -1 = bearish, 0 = neutral)
    pub htf_trend: f64,
    /// Distance to higher TF DM
    pub htf_dm_distance: f64,
    /// Alignment score (how many TFs agree on direction)
    pub tf_alignment: f64,

    // === Label ===
    /// Outcome: 1 = target hit, 0 = stop hit, -1 = expired
    pub label: Option<i8>,
    /// Time to outcome in milliseconds
    pub time_to_outcome_ms: Option<i64>,
    /// R multiple achieved (positive = profit, negative = loss)
    pub r_multiple: Option<f64>,
}

impl Default for FeatureVector {
    fn default() -> Self {
        Self {
            interaction_id: 0,
            zone_id: 0,
            timestamp_ms: 0,
            dist_to_zone_high: 0.0,
            dist_to_zone_low: 0.0,
            dist_to_zone_mid: 0.0,
            zone_width: 0.0,
            zone_position: 0.0,
            scenario_count: 0.0,
            prior_taps: 0.0,
            bars_since_creation: 0.0,
            zone_age_ms: 0.0,
            approach_from_above: 0.0,
            ret_1s: 0.0,
            ret_5s: 0.0,
            ret_10s: 0.0,
            ret_30s: 0.0,
            ret_1m: 0.0,
            ret_5m: 0.0,
            tick_velocity: 0.0,
            tick_acceleration: 0.0,
            consecutive_ticks: 0.0,
            dist_from_session_high: 0.0,
            dist_from_session_low: 0.0,
            dist_from_prior_close: 0.0,
            dist_from_open: 0.0,
            intraday_range_position: 0.0,
            tick_volume: 0.0,
            volume_1m: 0.0,
            volume_5m: 0.0,
            volume_spike_ratio: 0.0,
            cumulative_delta_1m: 0.0,
            large_trade_count_1m: 0.0,
            atr_14: 0.0,
            std_1m: 0.0,
            std_5m: 0.0,
            std_15m: 0.0,
            vol_percentile_session: 0.0,
            range_ratio: 0.0,
            is_rth: 0.0,
            minutes_since_open: 0.0,
            minutes_to_close: 0.0,
            day_of_week: 0.0,
            is_first_hour: 0.0,
            is_last_hour: 0.0,
            is_monday_open: 0.0,
            is_friday_close: 0.0,
            htf_trend: 0.0,
            htf_dm_distance: 0.0,
            tf_alignment: 0.0,
            label: None,
            time_to_outcome_ms: None,
            r_multiple: None,
        }
    }
}

/// Tick size for NQ (0.25 points)
pub const NQ_TICK_SIZE: f64 = 0.25;

/// Tick size for MNQ (same as NQ: 0.25 points)
pub const MNQ_TICK_SIZE: f64 = 0.25;

/// Convert price difference to ticks
pub fn to_ticks(price_diff: f64, tick_size: f64) -> f64 {
    price_diff / tick_size
}
