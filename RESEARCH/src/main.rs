//! PhaseShifter Backtest CLI
//!
//! Stream through historical SCID tick data, generate clusters, track zone interactions,
//! and extract features for ML analysis.
//!
//! Interaction Tracking (entry-agnostic):
//! 1. Price ENTERS cluster zone → start tracking
//! 2. Price EXITS cluster zone → capture exit features + exit direction
//! 3. Track post-exit path for 30m, 60m, and RTH close (max window)
//! 4. Record excursions, re-entry timing, and anchor-touch timing (no stop/entry)
//!
//! Direction Context (mean reversion):
//! - Zone ABOVE anchor = SHORT context (expect reversal down toward anchor)
//! - Zone BELOW anchor = LONG context (expect reversal up toward anchor)

use std::fs::{File, OpenOptions};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use phaseshifter_core::{Candle, EngineConfig, Phase, PhaseEngine, PhaseSide};
use serde::Serialize;
use tracing::info;
use tracing_subscriber::EnvFilter;

use phaseshifter_backtest::bar_builder::{MultiBarBuilder, Timeframe};
use phaseshifter_backtest::clusters::{
    Cluster, ClusterConfig, ClusterManager, ProjectionSide, TrackedNode,
};
use phaseshifter_backtest::scid::{find_mnq_scid_files, find_nq_scid_files, ScidReader};
use phaseshifter_backtest::Bar;
use phaseshifter_backtest::{FeatureExtractor, FeatureSnapshot, MNQ_TICK_SIZE, NQ_TICK_SIZE};

#[derive(Parser, Debug)]
#[command(name = "phaseshifter-backtest")]
#[command(about = "Interaction-only backtest for ML feature extraction")]
struct Args {
    /// Symbol to process (NQ or MNQ)
    #[arg(long, short = 's', default_value = "MNQ")]
    symbol: String,

    /// Path to Sierra Chart data directory
    #[arg(long, env = "SIERRA_DATA_DIR")]
    data_dir: PathBuf,

    /// Output file path (CSV)
    #[arg(long, short = 'o', default_value = "zone_interactions.csv")]
    output: PathBuf,

    /// Append to output CSV instead of overwrite
    #[arg(long)]
    append: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Limit number of files to process (for testing)
    #[arg(long)]
    limit_files: Option<usize>,

    /// Skip first N files
    #[arg(long, default_value = "0")]
    skip_files: usize,

}


fn create_csv_writer(path: &Path, append: bool) -> Result<csv::Writer<BufWriter<File>>> {
    let existed = path.exists();
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let file = options.open(path)?;

    let mut builder = csv::WriterBuilder::new();
    if append && existed {
        let len = std::fs::metadata(path)?.len();
        if len > 0 {
            builder.has_headers(false);
        }
    }

    Ok(builder.from_writer(BufWriter::new(file)))
}

/// Scenario configuration
struct Scenario {
    timeframe: Timeframe,
    phase_window: usize,
    engine: PhaseEngine,
}

impl Scenario {
    fn new(symbol: &str, timeframe: Timeframe, phase_window: usize) -> Self {
        let config = EngineConfig {
            side: PhaseSide::Long,
            timeframe: timeframe.as_str().to_string(),
            warmup_candles: phase_window,
            phase_window,
            depth_days: 60,
        };
        Self {
            timeframe,
            phase_window,
            engine: PhaseEngine::new(symbol, config),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum TradeDirection {
    Long,
    Short,
}


#[derive(Debug, Clone)]
struct ZoneInteraction {
    id: u64,
    direction: TradeDirection,

    // Zone info (frozen at detection)
    cluster_low: f64,
    cluster_high: f64,
    cluster_mid: f64,
    cluster_count: usize,
    cluster_unique_scenarios: usize,
    anchor_at_entry: f64,

    // Zone entry
    zone_entry_time: i64,
    zone_entry_price: f64,
    price_high_in_zone: f64,
    price_low_in_zone: f64,

    // Bar context at zone entry (for reference)
    s15_bar_at_zone_entry: Option<BarSnapshot>,
}

#[derive(Debug, Clone)]
struct WindowStats {
    max_price: f64,
    min_price: f64,
    max_time: i64,
    min_time: i64,
    max_above_zone: f64,
    max_above_time: i64,
    max_below_zone: f64,
    max_below_time: i64,
    anchor_touch_time: Option<i64>,
}

#[derive(Debug, Clone)]
struct PostExitInteraction {
    id: u64,
    direction: TradeDirection,

    // Zone info
    cluster_low: f64,
    cluster_high: f64,
    cluster_mid: f64,
    cluster_count: usize,
    cluster_unique_scenarios: usize,
    anchor_at_entry: f64,

    // Timing
    zone_entry_time: i64,
    zone_exit_time: i64,

    // Zone visit prices
    zone_entry_price: f64,
    zone_exit_price: f64,
    price_high_in_zone: f64,
    price_low_in_zone: f64,

    // Exit direction
    exited_above_zone: bool,
    exited_below_zone: bool,

    // Bar context
    s15_bar_at_zone_entry: Option<BarSnapshot>,
    m1_bar_at_exit: Option<BarSnapshot>,
    m5_bar_at_exit: Option<BarSnapshot>,

    // ML features at exit time
    exit_features: FeatureSnapshot,

    // Re-entry into zone
    reentry_time: Option<i64>,
    reentry_price: Option<f64>,

    // Post-exit windows
    window_30m_end: i64,
    window_60m_end: i64,
    window_session_end: i64,
    window_30m: WindowStats,
    window_60m: WindowStats,
    window_session: WindowStats,
}

const WINDOW_30M_MS: i64 = 30 * 60 * 1000;
const WINDOW_60M_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize)]
struct BarSnapshot {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: u64,
    bar_range: f64,
    body_size: f64,
    upper_wick: f64,
    lower_wick: f64,
    is_bullish: bool,
}

impl BarSnapshot {
    fn from_bar(bar: &Bar) -> Self {
        let body_top = bar.open.max(bar.close);
        let body_bottom = bar.open.min(bar.close);
        Self {
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume as u64,
            bar_range: bar.high - bar.low,
            body_size: (bar.close - bar.open).abs(),
            upper_wick: bar.high - body_top,
            lower_wick: body_bottom - bar.low,
            is_bullish: bar.close > bar.open,
        }
    }
}


#[derive(Debug, Clone, Serialize)]
struct InteractionRecord {
    interaction_id: u64,
    direction: TradeDirection,

    // Zone
    cluster_low: f64,
    cluster_high: f64,
    cluster_mid: f64,
    cluster_width: f64,
    cluster_width_pct: f64,
    cluster_count: usize,
    cluster_unique_scenarios: usize,
    anchor_at_entry: f64,
    zone_to_anchor_distance: f64,
    zone_to_anchor_distance_pct: f64,

    // Timing
    zone_entry_time: i64,
    zone_exit_time: i64,
    time_in_zone_ms: i64,

    // Prices
    zone_entry_price: f64,
    zone_exit_price: f64,
    price_high_in_zone: f64,
    price_low_in_zone: f64,

    // Exit direction
    exited_above_zone: bool,
    exited_below_zone: bool,
    exit_direction_aligned: bool,

    // Bar alignment at exit
    m1_direction_aligned: Option<bool>,
    m5_direction_aligned: Option<bool>,
    bars_aligned: Option<bool>,

    // --- Momentum Alignment ---
    momentum_1m_aligned: bool,
    momentum_5m_aligned: bool,
    momentum_aligned: bool,
    momentum_divergence: bool,

    // --- Trend Alignment ---
    htf_trend_aligned: bool,
    against_htf_trend: bool,

    // --- Volatility Context ---
    vol_expanding: bool,
    vol_contracting: bool,
    high_vol_environment: bool,
    low_vol_environment: bool,

    // --- Zone Quality ---
    multi_scenario_zone: bool,
    high_confluence_zone: bool,
    tight_zone: bool,
    wide_zone: bool,

    // --- Session Context ---
    in_optimal_session: bool,
    avoid_session: bool,
    high_activity_period: bool,
    low_activity_period: bool,

    // --- Price Position Context ---
    near_session_high: bool,
    near_session_low: bool,
    session_mid_zone: bool,

    // --- Delta/Flow Context ---
    delta_supports_trade: bool,
    delta_divergence: bool,

    // --- Composite Scores ---
    alignment_score: u8,
    red_flag_count: u8,

    // Bar context at ZONE ENTRY (reference)
    s15_bar_range: Option<f64>,
    s15_body_size: Option<f64>,
    s15_is_bullish: Option<bool>,

    // Bar context at EXIT (confirmation-scale)
    m1_at_exit_is_bullish: Option<bool>,
    m1_at_exit_body_size: Option<f64>,
    m5_at_exit_is_bullish: Option<bool>,
    m5_at_exit_body_size: Option<f64>,

    // === ML Features (from FeatureSnapshot at exit time) ===
    // Price Action
    ret_1s: f64,
    ret_5s: f64,
    ret_10s: f64,
    ret_30s: f64,
    ret_1m: f64,
    ret_5m: f64,
    tick_velocity: f64,
    tick_acceleration: f64,
    consecutive_ticks: u32,

    // Session-relative
    dist_from_session_high: f64,
    dist_from_session_low: f64,
    dist_from_prior_close: Option<f64>,
    dist_from_open: Option<f64>,
    intraday_range_position: f64,

    // Volume
    volume_1m: u64,
    volume_5m: u64,
    volume_spike_ratio: f64,
    cumulative_delta_1m: i64,
    cumulative_delta_5m: i64,
    large_trade_count_1m: u32,

    // Order Flow / DOM Features
    delta_flipped_bullish_30s: bool,
    delta_flipped_bearish_30s: bool,
    delta_flipped_bullish_60s: bool,
    delta_flipped_bearish_60s: bool,
    ms_since_delta_flip: Option<i64>,
    absorption_ratio: f64,
    volume_30s: u64,
    price_range_30s: f64,
    delta_exhaustion: f64,
    delta_momentum: i64,
    delta_supports_long: bool,
    delta_supports_short: bool,

    // New quant features
    delta_reversal_magnitude: i64,
    delta_reversal_normalized: f64,
    vwap: f64,
    vwap_distance: f64,
    vwap_distance_pct: f64,
    volume_price_correlation: f64,
    absorption_score: f64,

    // Research-based order flow features
    tick_rate_1s: f64,
    tick_rate_5m_avg: f64,
    tick_rate_ratio: f64,
    tick_rate_change: f64,
    tape_slowdown: bool,
    tape_acceleration: bool,

    stacked_bullish_levels: u32,
    stacked_bearish_levels: u32,
    imbalance_ratio: f64,
    imbalance_supports_long: bool,
    imbalance_supports_short: bool,

    order_flow_entropy: f64,
    flow_is_one_sided: bool,
    flow_is_two_sided: bool,

    bullish_delta_divergence: bool,
    bearish_delta_divergence: bool,
    divergence_supports_long: bool,
    divergence_supports_short: bool,

    volume_concentration: f64,
    high_volume_concentration: bool,

    absorption_detected: bool,
    momentum_breakout: bool,

    // Volatility
    atr_14: f64,
    std_1m: f64,
    std_5m: f64,
    std_15m: f64,
    vol_percentile_session: f64,
    range_ratio: f64,

    // Session Context
    is_rth: bool,
    minutes_since_open: f64,
    minutes_to_close: f64,
    day_of_week: u8,
    is_first_hour: bool,
    is_last_hour: bool,
    is_monday_open: bool,
    is_friday_close: bool,

    // Multi-TF Context
    htf_trend: f64,
    htf_dm_distance: f64,
    tf_alignment: f64,
    bullish_scenario_count: usize,
    bearish_scenario_count: usize,

    // Post-exit tracking
    reentry_time: Option<i64>,
    reentry_price: Option<f64>,
    time_to_reentry_ms: Option<i64>,

    max_up_ticks_30m: f64,
    max_down_ticks_30m: f64,
    time_to_max_up_30m_ms: i64,
    time_to_max_down_30m_ms: i64,
    max_above_zone_ticks_30m: f64,
    max_below_zone_ticks_30m: f64,
    time_to_max_above_zone_30m_ms: i64,
    time_to_max_below_zone_30m_ms: i64,
    anchor_touch_30m: bool,
    time_to_anchor_30m_ms: Option<i64>,

    max_up_ticks_60m: f64,
    max_down_ticks_60m: f64,
    time_to_max_up_60m_ms: i64,
    time_to_max_down_60m_ms: i64,
    max_above_zone_ticks_60m: f64,
    max_below_zone_ticks_60m: f64,
    time_to_max_above_zone_60m_ms: i64,
    time_to_max_below_zone_60m_ms: i64,
    anchor_touch_60m: bool,
    time_to_anchor_60m_ms: Option<i64>,

    max_up_ticks_session: f64,
    max_down_ticks_session: f64,
    time_to_max_up_session_ms: i64,
    time_to_max_down_session_ms: i64,
    max_above_zone_ticks_session: f64,
    max_below_zone_ticks_session: f64,
    time_to_max_above_zone_session_ms: i64,
    time_to_max_below_zone_session_ms: i64,
    anchor_touch_session: bool,
    time_to_anchor_session_ms: Option<i64>,
}


impl ZoneInteraction {
    fn new(
        id: u64,
        direction: TradeDirection,
        cluster: &Cluster,
        anchor: f64,
        entry_price: f64,
        entry_time: i64,
    ) -> Self {
        Self {
            id,
            direction,
            cluster_low: cluster.low,
            cluster_high: cluster.high,
            cluster_mid: (cluster.low + cluster.high) / 2.0,
            cluster_count: cluster.count,
            cluster_unique_scenarios: cluster.unique_scenarios,
            anchor_at_entry: anchor,
            zone_entry_time: entry_time,
            zone_entry_price: entry_price,
            price_high_in_zone: entry_price,
            price_low_in_zone: entry_price,
            s15_bar_at_zone_entry: None,
        }
    }

    fn price_in_zone(&self, price: f64) -> bool {
        price >= self.cluster_low && price <= self.cluster_high
    }

    fn update_in_zone(&mut self, price: f64) {
        self.price_high_in_zone = self.price_high_in_zone.max(price);
        self.price_low_in_zone = self.price_low_in_zone.min(price);
    }

    fn to_post_exit(
        self,
        exit_price: f64,
        exit_time: i64,
        exit_features: FeatureSnapshot,
        m1_bar_at_exit: Option<BarSnapshot>,
        m5_bar_at_exit: Option<BarSnapshot>,
        session_end_ms: i64,
    ) -> PostExitInteraction {
        let exited_above_zone = exit_price > self.cluster_high;
        let exited_below_zone = exit_price < self.cluster_low;

        let session_end = session_end_ms.max(exit_time);
        let window_30m_end = (exit_time.saturating_add(WINDOW_30M_MS)).min(session_end);
        let window_60m_end = (exit_time.saturating_add(WINDOW_60M_MS)).min(session_end);

        PostExitInteraction {
            id: self.id,
            direction: self.direction,
            cluster_low: self.cluster_low,
            cluster_high: self.cluster_high,
            cluster_mid: self.cluster_mid,
            cluster_count: self.cluster_count,
            cluster_unique_scenarios: self.cluster_unique_scenarios,
            anchor_at_entry: self.anchor_at_entry,
            zone_entry_time: self.zone_entry_time,
            zone_exit_time: exit_time,
            zone_entry_price: self.zone_entry_price,
            zone_exit_price: exit_price,
            price_high_in_zone: self.price_high_in_zone,
            price_low_in_zone: self.price_low_in_zone,
            exited_above_zone,
            exited_below_zone,
            s15_bar_at_zone_entry: self.s15_bar_at_zone_entry,
            m1_bar_at_exit,
            m5_bar_at_exit,
            exit_features,
            reentry_time: None,
            reentry_price: None,
            window_30m_end,
            window_60m_end,
            window_session_end: session_end,
            window_30m: WindowStats::new(exit_price, exit_time),
            window_60m: WindowStats::new(exit_price, exit_time),
            window_session: WindowStats::new(exit_price, exit_time),
        }
    }
}

impl WindowStats {
    fn new(exit_price: f64, exit_time: i64) -> Self {
        Self {
            max_price: exit_price,
            min_price: exit_price,
            max_time: exit_time,
            min_time: exit_time,
            max_above_zone: 0.0,
            max_above_time: exit_time,
            max_below_zone: 0.0,
            max_below_time: exit_time,
            anchor_touch_time: None,
        }
    }

    fn update(
        &mut self,
        price: f64,
        time: i64,
        zone_low: f64,
        zone_high: f64,
        anchor: f64,
        direction: TradeDirection,
    ) {
        if price > self.max_price {
            self.max_price = price;
            self.max_time = time;
        }
        if price < self.min_price {
            self.min_price = price;
            self.min_time = time;
        }

        if price > zone_high {
            let above = price - zone_high;
            if above > self.max_above_zone {
                self.max_above_zone = above;
                self.max_above_time = time;
            }
        }

        if price < zone_low {
            let below = zone_low - price;
            if below > self.max_below_zone {
                self.max_below_zone = below;
                self.max_below_time = time;
            }
        }

        if self.anchor_touch_time.is_none() {
            let touched = match direction {
                TradeDirection::Long => price >= anchor,
                TradeDirection::Short => price <= anchor,
            };
            if touched {
                self.anchor_touch_time = Some(time);
            }
        }
    }
}

impl PostExitInteraction {
    fn update(&mut self, price: f64, time: i64) {
        if self.reentry_time.is_none() && price >= self.cluster_low && price <= self.cluster_high {
            self.reentry_time = Some(time);
            self.reentry_price = Some(price);
        }

        if time <= self.window_30m_end {
            self.window_30m.update(
                price,
                time,
                self.cluster_low,
                self.cluster_high,
                self.anchor_at_entry,
                self.direction,
            );
        }
        if time <= self.window_60m_end {
            self.window_60m.update(
                price,
                time,
                self.cluster_low,
                self.cluster_high,
                self.anchor_at_entry,
                self.direction,
            );
        }
        if time <= self.window_session_end {
            self.window_session.update(
                price,
                time,
                self.cluster_low,
                self.cluster_high,
                self.anchor_at_entry,
                self.direction,
            );
        }
    }

    fn is_complete(&self, time: i64) -> bool {
        time >= self.window_session_end
    }

    fn to_record(&self, tick_size: f64) -> InteractionRecord {
        let cluster_width = self.cluster_high - self.cluster_low;
        let zone_to_anchor = (self.cluster_mid - self.anchor_at_entry).abs();
        let time_in_zone_ms = self.zone_exit_time - self.zone_entry_time;

        let exit_direction_aligned = match self.direction {
            TradeDirection::Short => self.exited_above_zone,
            TradeDirection::Long => self.exited_below_zone,
        };

        let m1_direction_aligned = self.m1_bar_at_exit.as_ref().map(|b| match self.direction {
            TradeDirection::Short => !b.is_bullish,
            TradeDirection::Long => b.is_bullish,
        });
        let m5_direction_aligned = self.m5_bar_at_exit.as_ref().map(|b| match self.direction {
            TradeDirection::Short => !b.is_bullish,
            TradeDirection::Long => b.is_bullish,
        });
        let bars_aligned = match (m1_direction_aligned, m5_direction_aligned) {
            (Some(a), Some(b)) => Some(a && b),
            _ => None,
        };

        let f = &self.exit_features;

        let momentum_1m_aligned = match self.direction {
            TradeDirection::Short => f.ret_1m < 0.0,
            TradeDirection::Long => f.ret_1m > 0.0,
        };
        let momentum_5m_aligned = match self.direction {
            TradeDirection::Short => f.ret_5m < 0.0,
            TradeDirection::Long => f.ret_5m > 0.0,
        };
        let momentum_aligned = momentum_1m_aligned && momentum_5m_aligned;
        let momentum_divergence = (f.ret_1m > 0.0 && f.ret_5m < 0.0)
            || (f.ret_1m < 0.0 && f.ret_5m > 0.0);

        let htf_trend_aligned = match self.direction {
            TradeDirection::Short => f.htf_trend < 0.0,
            TradeDirection::Long => f.htf_trend > 0.0,
        };
        let against_htf_trend = match self.direction {
            TradeDirection::Short => f.htf_trend > 0.0,
            TradeDirection::Long => f.htf_trend < 0.0,
        };

        let vol_expanding = f.std_5m > 0.0 && f.std_1m > f.std_5m;
        let vol_contracting = f.std_5m > 0.0 && f.std_1m < f.std_5m;
        let high_vol_environment = f.vol_percentile_session > 70.0;
        let low_vol_environment = f.vol_percentile_session < 30.0;

        let multi_scenario_zone = self.cluster_unique_scenarios >= 2;
        let high_confluence_zone = self.cluster_count >= 4;
        let tight_zone = cluster_width / self.cluster_mid * 100.0 < 0.15;
        let wide_zone = cluster_width / self.cluster_mid * 100.0 > 0.30;

        let in_optimal_session = f.is_rth && !f.is_first_hour && !f.is_last_hour;
        let avoid_session = f.minutes_since_open < 15.0 || f.minutes_to_close < 15.0;
        let high_activity_period = f.volume_spike_ratio > 1.5;
        let low_activity_period = f.volume_spike_ratio < 0.5;

        let near_session_high = f.dist_from_session_high.abs() < 0.001;
        let near_session_low = f.dist_from_session_low.abs() < 0.001;
        let session_mid_zone = f.intraday_range_position > 0.4 && f.intraday_range_position < 0.6;

        let delta_supports_trade = match self.direction {
            TradeDirection::Short => f.cumulative_delta_5m < 0,
            TradeDirection::Long => f.cumulative_delta_5m > 0,
        };
        let delta_divergence = (f.cumulative_delta_5m > 0 && f.ret_5m < 0.0)
            || (f.cumulative_delta_5m < 0 && f.ret_5m > 0.0);

        let mut alignment_score: u8 = 0;
        if exit_direction_aligned {
            alignment_score += 1;
        }
        if let (Some(m1), Some(m5)) = (&self.m1_bar_at_exit, &self.m5_bar_at_exit) {
            let aligned = match self.direction {
                TradeDirection::Short => !m1.is_bullish && !m5.is_bullish,
                TradeDirection::Long => m1.is_bullish && m5.is_bullish,
            };
            if aligned {
                alignment_score += 2;
            }
        }
        if momentum_aligned {
            alignment_score += 2;
        }
        if htf_trend_aligned {
            alignment_score += 1;
        }
        if delta_supports_trade {
            alignment_score += 1;
        }
        if in_optimal_session {
            alignment_score += 1;
        }
        if multi_scenario_zone {
            alignment_score += 1;
        }
        if high_confluence_zone {
            alignment_score += 1;
        }

        let mut red_flag_count: u8 = 0;
        let exit_wrong = match self.direction {
            TradeDirection::Short => self.exited_below_zone,
            TradeDirection::Long => self.exited_above_zone,
        };
        if exit_wrong {
            red_flag_count += 2;
        }
        if against_htf_trend {
            red_flag_count += 1;
        }
        if momentum_divergence {
            red_flag_count += 1;
        }
        if avoid_session {
            red_flag_count += 1;
        }
        if low_activity_period {
            red_flag_count += 1;
        }
        if f.vol_percentile_session > 80.0 {
            red_flag_count += 1;
        }
        if delta_divergence {
            red_flag_count += 1;
        }
        if wide_zone {
            red_flag_count += 1;
        }

        let to_ticks = |delta: f64| if tick_size > 0.0 { delta / tick_size } else { 0.0 };

        let reentry_time = self.reentry_time;
        let reentry_price = self.reentry_price;
        let time_to_reentry_ms = reentry_time.map(|t| t - self.zone_exit_time);

        let max_up_ticks_30m = to_ticks(self.window_30m.max_price - self.zone_exit_price);
        let max_down_ticks_30m = to_ticks(self.zone_exit_price - self.window_30m.min_price);
        let time_to_max_up_30m_ms = self.window_30m.max_time - self.zone_exit_time;
        let time_to_max_down_30m_ms = self.window_30m.min_time - self.zone_exit_time;
        let max_above_zone_ticks_30m = to_ticks(self.window_30m.max_above_zone);
        let max_below_zone_ticks_30m = to_ticks(self.window_30m.max_below_zone);
        let time_to_max_above_zone_30m_ms = self.window_30m.max_above_time - self.zone_exit_time;
        let time_to_max_below_zone_30m_ms = self.window_30m.max_below_time - self.zone_exit_time;
        let anchor_touch_30m = self.window_30m.anchor_touch_time.is_some();
        let time_to_anchor_30m_ms = self
            .window_30m
            .anchor_touch_time
            .map(|t| t - self.zone_exit_time);

        let max_up_ticks_60m = to_ticks(self.window_60m.max_price - self.zone_exit_price);
        let max_down_ticks_60m = to_ticks(self.zone_exit_price - self.window_60m.min_price);
        let time_to_max_up_60m_ms = self.window_60m.max_time - self.zone_exit_time;
        let time_to_max_down_60m_ms = self.window_60m.min_time - self.zone_exit_time;
        let max_above_zone_ticks_60m = to_ticks(self.window_60m.max_above_zone);
        let max_below_zone_ticks_60m = to_ticks(self.window_60m.max_below_zone);
        let time_to_max_above_zone_60m_ms = self.window_60m.max_above_time - self.zone_exit_time;
        let time_to_max_below_zone_60m_ms = self.window_60m.max_below_time - self.zone_exit_time;
        let anchor_touch_60m = self.window_60m.anchor_touch_time.is_some();
        let time_to_anchor_60m_ms = self
            .window_60m
            .anchor_touch_time
            .map(|t| t - self.zone_exit_time);

        let max_up_ticks_session = to_ticks(self.window_session.max_price - self.zone_exit_price);
        let max_down_ticks_session = to_ticks(self.zone_exit_price - self.window_session.min_price);
        let time_to_max_up_session_ms = self.window_session.max_time - self.zone_exit_time;
        let time_to_max_down_session_ms = self.window_session.min_time - self.zone_exit_time;
        let max_above_zone_ticks_session = to_ticks(self.window_session.max_above_zone);
        let max_below_zone_ticks_session = to_ticks(self.window_session.max_below_zone);
        let time_to_max_above_zone_session_ms =
            self.window_session.max_above_time - self.zone_exit_time;
        let time_to_max_below_zone_session_ms =
            self.window_session.max_below_time - self.zone_exit_time;
        let anchor_touch_session = self.window_session.anchor_touch_time.is_some();
        let time_to_anchor_session_ms = self
            .window_session
            .anchor_touch_time
            .map(|t| t - self.zone_exit_time);

        InteractionRecord {
            interaction_id: self.id,
            direction: self.direction,
            cluster_low: self.cluster_low,
            cluster_high: self.cluster_high,
            cluster_mid: self.cluster_mid,
            cluster_width,
            cluster_width_pct: cluster_width / self.cluster_mid * 100.0,
            cluster_count: self.cluster_count,
            cluster_unique_scenarios: self.cluster_unique_scenarios,
            anchor_at_entry: self.anchor_at_entry,
            zone_to_anchor_distance: zone_to_anchor,
            zone_to_anchor_distance_pct: zone_to_anchor / self.cluster_mid * 100.0,
            zone_entry_time: self.zone_entry_time,
            zone_exit_time: self.zone_exit_time,
            time_in_zone_ms,
            zone_entry_price: self.zone_entry_price,
            zone_exit_price: self.zone_exit_price,
            price_high_in_zone: self.price_high_in_zone,
            price_low_in_zone: self.price_low_in_zone,
            exited_above_zone: self.exited_above_zone,
            exited_below_zone: self.exited_below_zone,
            exit_direction_aligned,
            m1_direction_aligned,
            m5_direction_aligned,
            bars_aligned,
            momentum_1m_aligned,
            momentum_5m_aligned,
            momentum_aligned,
            momentum_divergence,
            htf_trend_aligned,
            against_htf_trend,
            vol_expanding,
            vol_contracting,
            high_vol_environment,
            low_vol_environment,
            multi_scenario_zone,
            high_confluence_zone,
            tight_zone,
            wide_zone,
            in_optimal_session,
            avoid_session,
            high_activity_period,
            low_activity_period,
            near_session_high,
            near_session_low,
            session_mid_zone,
            delta_supports_trade,
            delta_divergence,
            alignment_score,
            red_flag_count,
            s15_bar_range: self.s15_bar_at_zone_entry.as_ref().map(|b| b.bar_range),
            s15_body_size: self.s15_bar_at_zone_entry.as_ref().map(|b| b.body_size),
            s15_is_bullish: self.s15_bar_at_zone_entry.as_ref().map(|b| b.is_bullish),
            m1_at_exit_is_bullish: self.m1_bar_at_exit.as_ref().map(|b| b.is_bullish),
            m1_at_exit_body_size: self.m1_bar_at_exit.as_ref().map(|b| b.body_size),
            m5_at_exit_is_bullish: self.m5_bar_at_exit.as_ref().map(|b| b.is_bullish),
            m5_at_exit_body_size: self.m5_bar_at_exit.as_ref().map(|b| b.body_size),
            ret_1s: f.ret_1s,
            ret_5s: f.ret_5s,
            ret_10s: f.ret_10s,
            ret_30s: f.ret_30s,
            ret_1m: f.ret_1m,
            ret_5m: f.ret_5m,
            tick_velocity: f.tick_velocity,
            tick_acceleration: f.tick_acceleration,
            consecutive_ticks: f.consecutive_ticks,
            dist_from_session_high: f.dist_from_session_high,
            dist_from_session_low: f.dist_from_session_low,
            dist_from_prior_close: f.dist_from_prior_close,
            dist_from_open: f.dist_from_open,
            intraday_range_position: f.intraday_range_position,
            volume_1m: f.volume_1m,
            volume_5m: f.volume_5m,
            volume_spike_ratio: f.volume_spike_ratio,
            cumulative_delta_1m: f.cumulative_delta_1m,
            cumulative_delta_5m: f.cumulative_delta_5m,
            large_trade_count_1m: f.large_trade_count_1m,
            delta_flipped_bullish_30s: f.delta_flipped_bullish_30s,
            delta_flipped_bearish_30s: f.delta_flipped_bearish_30s,
            delta_flipped_bullish_60s: f.delta_flipped_bullish_60s,
            delta_flipped_bearish_60s: f.delta_flipped_bearish_60s,
            ms_since_delta_flip: f.ms_since_delta_flip,
            absorption_ratio: f.absorption_ratio,
            volume_30s: f.volume_30s,
            price_range_30s: f.price_range_30s,
            delta_exhaustion: f.delta_exhaustion,
            delta_momentum: f.delta_momentum,
            delta_supports_long: f.delta_supports_long,
            delta_supports_short: f.delta_supports_short,
            delta_reversal_magnitude: f.delta_reversal_magnitude,
            delta_reversal_normalized: f.delta_reversal_normalized,
            vwap: f.vwap,
            vwap_distance: f.vwap_distance,
            vwap_distance_pct: f.vwap_distance_pct,
            volume_price_correlation: f.volume_price_correlation,
            absorption_score: f.absorption_score,
            tick_rate_1s: f.tick_rate_1s,
            tick_rate_5m_avg: f.tick_rate_5m_avg,
            tick_rate_ratio: f.tick_rate_ratio,
            tick_rate_change: f.tick_rate_change,
            tape_slowdown: f.tape_slowdown,
            tape_acceleration: f.tape_acceleration,
            stacked_bullish_levels: f.stacked_bullish_levels,
            stacked_bearish_levels: f.stacked_bearish_levels,
            imbalance_ratio: f.imbalance_ratio,
            imbalance_supports_long: f.imbalance_supports_long,
            imbalance_supports_short: f.imbalance_supports_short,
            order_flow_entropy: f.order_flow_entropy,
            flow_is_one_sided: f.flow_is_one_sided,
            flow_is_two_sided: f.flow_is_two_sided,
            bullish_delta_divergence: f.bullish_delta_divergence,
            bearish_delta_divergence: f.bearish_delta_divergence,
            divergence_supports_long: f.divergence_supports_long,
            divergence_supports_short: f.divergence_supports_short,
            volume_concentration: f.volume_concentration,
            high_volume_concentration: f.high_volume_concentration,
            absorption_detected: f.absorption_detected,
            momentum_breakout: f.momentum_breakout,
            atr_14: f.atr_14,
            std_1m: f.std_1m,
            std_5m: f.std_5m,
            std_15m: f.std_15m,
            vol_percentile_session: f.vol_percentile_session,
            range_ratio: f.range_ratio,
            is_rth: f.is_rth,
            minutes_since_open: f.minutes_since_open,
            minutes_to_close: f.minutes_to_close,
            day_of_week: f.day_of_week,
            is_first_hour: f.is_first_hour,
            is_last_hour: f.is_last_hour,
            is_monday_open: f.is_monday_open,
            is_friday_close: f.is_friday_close,
            htf_trend: f.htf_trend,
            htf_dm_distance: f.htf_dm_distance,
            tf_alignment: f.tf_alignment,
            bullish_scenario_count: f.bullish_scenario_count,
            bearish_scenario_count: f.bearish_scenario_count,
            reentry_time,
            reentry_price,
            time_to_reentry_ms,
            max_up_ticks_30m,
            max_down_ticks_30m,
            time_to_max_up_30m_ms,
            time_to_max_down_30m_ms,
            max_above_zone_ticks_30m,
            max_below_zone_ticks_30m,
            time_to_max_above_zone_30m_ms,
            time_to_max_below_zone_30m_ms,
            anchor_touch_30m,
            time_to_anchor_30m_ms,
            max_up_ticks_60m,
            max_down_ticks_60m,
            time_to_max_up_60m_ms,
            time_to_max_down_60m_ms,
            max_above_zone_ticks_60m,
            max_below_zone_ticks_60m,
            time_to_max_above_zone_60m_ms,
            time_to_max_below_zone_60m_ms,
            anchor_touch_60m,
            time_to_anchor_60m_ms,
            max_up_ticks_session,
            max_down_ticks_session,
            time_to_max_up_session_ms,
            time_to_max_down_session_ms,
            max_above_zone_ticks_session,
            max_below_zone_ticks_session,
            time_to_max_above_zone_session_ms,
            time_to_max_below_zone_session_ms,
            anchor_touch_session,
            time_to_anchor_session_ms,
        }
    }
}

fn cluster_direction(cluster: &Cluster, anchor: f64) -> TradeDirection {
    let mid = (cluster.low + cluster.high) / 2.0;
    if mid > anchor {
        TradeDirection::Short
    } else {
        TradeDirection::Long
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let filter = EnvFilter::try_new(&args.log_level)?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let symbol = args.symbol.to_uppercase();
    let tick_size = match symbol.as_str() {
        "NQ" => NQ_TICK_SIZE,
        "MNQ" => MNQ_TICK_SIZE,
        _ => anyhow::bail!("Unsupported symbol: {}. Use NQ or MNQ", symbol),
    };

    info!("PhaseShifter Backtest (Interaction-Only)");
    info!("Symbol: {} (tick size: {})", symbol, tick_size);
    info!("Post-exit windows: 30m, 60m, and RTH close");

    let mut scid_files = match symbol.as_str() {
        "NQ" => find_nq_scid_files(&args.data_dir)?,
        "MNQ" => find_mnq_scid_files(&args.data_dir)?,
        _ => unreachable!(),
    };

    if scid_files.is_empty() {
        anyhow::bail!(
            "No {} SCID files found in {}",
            symbol,
            args.data_dir.display()
        );
    }

    if args.skip_files > 0 {
        if args.skip_files >= scid_files.len() {
            anyhow::bail!("Skip count exceeds number of files");
        }
        scid_files = scid_files[args.skip_files..].to_vec();
    }

    if let Some(limit) = args.limit_files {
        scid_files.truncate(limit);
    }

    info!("Processing {} SCID files", scid_files.len());

    let scenario_configs: Vec<(Timeframe, usize)> = vec![
        (Timeframe::M1, 50),
        (Timeframe::M5, 7),
        (Timeframe::M5, 20),
        (Timeframe::M15, 10),
        (Timeframe::H1, 7),
    ];

    let mut scenarios: Vec<Scenario> = scenario_configs
        .iter()
        .map(|(tf, pw)| Scenario::new(&symbol, *tf, *pw))
        .collect();

    let timeframes = vec![
        Timeframe::S15,
        Timeframe::M1,
        Timeframe::M5,
        Timeframe::M15,
        Timeframe::M30,
        Timeframe::H1,
    ];

    let mut bar_builder = MultiBarBuilder::new(&timeframes, 500);
    let mut cluster_manager = ClusterManager::new(ClusterConfig::default());
    let mut feature_extractor = FeatureExtractor::new(tick_size);
    let mut writer = create_csv_writer(&args.output, args.append)?;

    // Active zone interaction + post-exit trackers
    let mut current_interaction: Option<ZoneInteraction> = None;
    let mut post_exit_interactions: Vec<PostExitInteraction> = Vec::new();
    let mut next_id: u64 = 0;

    let mut current_s15_bar: Option<Bar> = None;
    let mut current_m1_bar: Option<Bar> = None;
    let mut current_m5_bar: Option<Bar> = None;

    let mut total_ticks: u64 = 0;
    let mut total_nodes: u64 = 0;
    let mut total_zone_entries: u64 = 0;
    let mut completed_interactions: u64 = 0;
    let mut anchor_touch_30m: u64 = 0;
    let mut anchor_touch_60m: u64 = 0;
    let mut anchor_touch_session: u64 = 0;
    let mut sum_time_in_zone_ms: i64 = 0;

    for (file_idx, scid_path) in scid_files.iter().enumerate() {
        info!(
            "[{}/{}] Processing: {}",
            file_idx + 1,
            scid_files.len(),
            scid_path
        );

        let mut reader = ScidReader::open(scid_path)?;
        let record_count = reader.record_count();

        let pb = ProgressBar::new(record_count);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )?
                .progress_chars("#>-"),
        );

        let mut file_ticks: u64 = 0;

        while let Some(record) = reader.next_record()? {
            if !record.is_valid_tick() {
                continue;
            }

            let enhanced_tick = record.to_enhanced_tick();
            let tick = enhanced_tick.to_basic();
            total_ticks += 1;
            file_ticks += 1;

            let price = tick.price;
            let time = tick.timestamp_ms;

            // Update feature extractor with every tick
            feature_extractor.on_tick(&enhanced_tick);

            // ========================================
            // STEP 1: Update bars and scenarios
            // ========================================
            let completed_bars = bar_builder.on_tick(&tick);

            for (timeframe, bar) in &completed_bars {
                match timeframe {
                    Timeframe::S15 => current_s15_bar = Some(bar.clone()),
                    Timeframe::M1 => current_m1_bar = Some(bar.clone()),
                    Timeframe::M5 => current_m5_bar = Some(bar.clone()),
                    _ => {}
                }

                // Update feature extractor with bar close
                feature_extractor.on_bar_close(bar, *timeframe);

                for scenario in &mut scenarios {
                    if scenario.timeframe != *timeframe {
                        continue;
                    }

                    let candle = Candle {
                        timestamp_ms: bar.timestamp_ms,
                        open: bar.open,
                        high: bar.high,
                        low: bar.low,
                        close: bar.close,
                        volume: bar.volume as f64,
                    };

                    if let Some(_dm) = scenario.engine.on_candle(&candle) {
                        if let Some(anchor) = scenario.engine.current_anchor() {
                            cluster_manager.set_scenario_anchor(
                                &symbol,
                                scenario.timeframe.as_str(),
                                scenario.phase_window,
                                anchor,
                            );
                            cluster_manager.update_scenario_with_bar(
                                &symbol,
                                scenario.timeframe.as_str(),
                                scenario.phase_window,
                                anchor,
                                bar.high,
                                bar.low,
                                bar.timestamp_ms,
                            );

                            // Update feature extractor with phase state
                            let is_bullish =
                                matches!(scenario.engine.current_phase(), Some(Phase::Bullish));
                            feature_extractor.on_phase_update(
                                scenario.timeframe,
                                scenario.phase_window,
                                is_bullish,
                                anchor,
                                bar.timestamp_ms,
                            );
                        }

                        if let Some(flip) = scenario.engine.take_last_phase_flip() {
                            let side = match flip.from {
                                Phase::Bullish => ProjectionSide::Bullish,
                                Phase::Bearish => ProjectionSide::Bearish,
                            };
                            let node = TrackedNode::new(
                                side,
                                scenario.timeframe.as_str().to_string(),
                                scenario.phase_window,
                                flip.node_move_pct,
                                flip.old_anchor,
                                bar.timestamp_ms,
                            );
                            cluster_manager.add_node(&symbol, node);
                            total_nodes += 1;
                        }
                    }
                }
            }

            // ========================================
            // STEP 2: Update post-exit trackers
            // ========================================
            if !post_exit_interactions.is_empty() {
                let mut idx = 0;
                while idx < post_exit_interactions.len() {
                    post_exit_interactions[idx].update(price, time);
                    if post_exit_interactions[idx].is_complete(time) {
                        let record = post_exit_interactions[idx].to_record(tick_size);
                        if record.anchor_touch_30m {
                            anchor_touch_30m += 1;
                        }
                        if record.anchor_touch_60m {
                            anchor_touch_60m += 1;
                        }
                        if record.anchor_touch_session {
                            anchor_touch_session += 1;
                        }
                        sum_time_in_zone_ms += record.time_in_zone_ms;
                        completed_interactions += 1;
                        writer.serialize(record)?;
                        post_exit_interactions.swap_remove(idx);
                    } else {
                        idx += 1;
                    }
                }
            }

            // ========================================
            // STEP 3: Update current in-zone interaction
            // ========================================
            if let Some(mut interaction) = current_interaction.take() {
                if interaction.price_in_zone(price) {
                    interaction.update_in_zone(price);
                    current_interaction = Some(interaction);
                } else {
                    let exit_features = feature_extractor.extract_features(price, time);
                    let m1_exit = current_m1_bar.as_ref().map(BarSnapshot::from_bar);
                    let m5_exit = current_m5_bar.as_ref().map(BarSnapshot::from_bar);
                    let session_end = feature_extractor
                        .rth_close_ms()
                        .unwrap_or(time.saturating_add(WINDOW_60M_MS));

                    let post_exit = interaction.to_post_exit(
                        price,
                        time,
                        exit_features,
                        m1_exit,
                        m5_exit,
                        session_end,
                    );

                    if post_exit.is_complete(time) {
                        let record = post_exit.to_record(tick_size);
                        if record.anchor_touch_30m {
                            anchor_touch_30m += 1;
                        }
                        if record.anchor_touch_60m {
                            anchor_touch_60m += 1;
                        }
                        if record.anchor_touch_session {
                            anchor_touch_session += 1;
                        }
                        sum_time_in_zone_ms += record.time_in_zone_ms;
                        completed_interactions += 1;
                        writer.serialize(record)?;
                    } else {
                        post_exit_interactions.push(post_exit);
                    }
                }
            }

            // ========================================
            // STEP 4: Check for new zone entry
            // ========================================
            if current_interaction.is_none() {
                if let Some(clusters_data) = cluster_manager.get_clusters(&symbol) {
                    if let Some(anchor) = clusters_data.anchor {
                        for cluster in &clusters_data.clusters {
                            if price >= cluster.low && price <= cluster.high {
                                let direction = cluster_direction(cluster, anchor);
                                let valid = match direction {
                                    TradeDirection::Long => anchor > price,
                                    TradeDirection::Short => anchor < price,
                                };

                                if valid {
                                    let mut interaction = ZoneInteraction::new(
                                        next_id, direction, cluster, anchor, price, time,
                                    );
                                    next_id += 1;

                                    // Capture bar context at ZONE ENTRY (for reference only)
                                    if let Some(bar) = &current_s15_bar {
                                        interaction.s15_bar_at_zone_entry =
                                            Some(BarSnapshot::from_bar(bar));
                                    }
                                    // NOTE: exit_features will be captured at ZONE EXIT time

                                    current_interaction = Some(interaction);
                                    total_zone_entries += 1;
                                    break; // Only one zone per tick
                                }
                            }
                        }
                    }
                }
            }
            // NOTE: Removed redundant get_clusters call that was only counting skipped trades
            // The skipped_already_in_trade counter is not useful enough to justify the overhead

            if file_ticks % 100_000 == 0 {
                pb.set_position(reader.position());
            }
        }

        pb.finish_with_message(format!("{} ticks processed", file_ticks));
        writer.flush()?;
    }


    info!("=== Results ===");
    info!("Total ticks: {}", total_ticks);
    info!("Total nodes: {}", total_nodes);
    info!("Zone entries: {}", total_zone_entries);
    info!("Completed interactions: {}", completed_interactions);

    if completed_interactions > 0 {
        let completed = completed_interactions as f64;
        info!(
            "Anchor touch rates: 30m {:.1}%, 60m {:.1}%, session {:.1}%",
            anchor_touch_30m as f64 / completed * 100.0,
            anchor_touch_60m as f64 / completed * 100.0,
            anchor_touch_session as f64 / completed * 100.0
        );
        let avg_time_in_zone = sum_time_in_zone_ms as f64 / completed;
        info!("Average time in zone: {:.1} ms", avg_time_in_zone);
    }

    writer.flush()?;
    if completed_interactions > 0 {
        info!(
            "Wrote {} records to {}",
            completed_interactions,
            args.output.display()
        );
    }

    info!("Done!");
    Ok(())
}

