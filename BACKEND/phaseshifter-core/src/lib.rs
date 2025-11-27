use std::{collections::VecDeque, fmt, mem};

use serde::Serialize;

pub mod config;
pub mod csv_source;
mod nodes;

use anyhow::{Context, Result};
use clap::Parser;
use config::{CliArgs, RuntimeConfig, build_runtime_config};
use csv_source::CsvCandleSource;
use nodes::{Bar, NodeCreation, NodeDirection, NodeEvent, NodeState, build_nodes_from_history};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use tracing::{debug, info, trace, warn};
use tracing_subscriber::EnvFilter;

/// Core data structures and a stub phase engine for the PhaseShifter backend.
#[derive(Debug, Clone)]
pub struct Candle {
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum PhaseSide {
    Long,
    Short,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Bullish,
    Bearish,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseUpdatePhase {
    Bullish,
    Bearish,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Phase::Bullish => write!(f, "bullish"),
            Phase::Bearish => write!(f, "bearish"),
        }
    }
}

impl From<Phase> for PhaseUpdatePhase {
    fn from(value: Phase) -> Self {
        match value {
            Phase::Bullish => PhaseUpdatePhase::Bullish,
            Phase::Bearish => PhaseUpdatePhase::Bearish,
        }
    }
}

impl From<NodeDirection> for PhaseUpdatePhase {
    fn from(value: NodeDirection) -> Self {
        match value {
            NodeDirection::Bullish => PhaseUpdatePhase::Bullish,
            NodeDirection::Bearish => PhaseUpdatePhase::Bearish,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub side: PhaseSide,
    pub timeframe: String,
    pub warmup_candles: usize,
    /// Period length used for Donchian midpoint (phase window).
    pub phase_window: usize,
    /// Depth in days for future aggregation / lookback use.
    pub depth_days: i64,
}

#[derive(Debug)]
pub struct PhaseEngine {
    symbol: String,
    config: EngineConfig,
    closes: VecDeque<f64>,                     // rolling close buffer for DM
    current_dm: Option<f64>,                   // Donchian midpoint for the current candle
    previous_dm: Option<f64>,                  // prior DM so we can tell slope direction
    current_phase: Option<Phase>,              // which side we are on right now
    current_anchor: Option<f64>,               // anchor price locked in at phase start
    phase_high: Option<f64>,                   // highest high seen during a bullish leg
    phase_low: Option<f64>,                    // lowest low seen during a bearish leg
    last_phase_flip: Option<PhaseFlipInfo>,    // stash last flip so callers can read it once
    pending_node_creations: Vec<NodeCreation>, // node events emitted since last candle
}

#[derive(Debug, Clone, Serialize)]
pub struct PhaseUpdate {
    #[serde(rename = "type")]
    pub update_type: &'static str,
    pub symbol: String,
    pub time: i64,
    pub phase: PhaseUpdatePhase,
    pub anchor: f64,
    pub dm: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub depth: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct PhaseFlipInfo {
    pub from: Phase,
    pub to: Phase,
    pub old_anchor: f64,
    pub new_anchor: f64,
    pub node_move_pct: f64,
}

/// Generic candle source that can yield a stream of candles from any backend.
pub trait CandleSource {
    type Error;
    type IntoIter: Iterator<Item = Result<Candle, Self::Error>>;

    fn into_iter(self) -> Self::IntoIter;
}

impl PhaseEngine {
    pub fn new(symbol: impl Into<String>, config: EngineConfig) -> Self {
        assert!(
            config.phase_window > 0,
            "phase_window must be greater than zero"
        );
        let capacity = config.phase_window;
        Self {
            symbol: symbol.into(),
            config,
            closes: VecDeque::with_capacity(capacity),
            current_dm: None,
            previous_dm: None,
            current_phase: None,
            current_anchor: None,
            phase_high: None,
            phase_low: None,
            last_phase_flip: None,
            pending_node_creations: Vec::new(),
        }
    }

    /// Handle a new candle and, when enough data exists, compute the Donchian midpoint.
    /// Returns `Some(dm)` once the buffer has reached the configured window size, otherwise `None`.
    pub fn on_candle(&mut self, candle: &Candle) -> Option<f64> {
        self.closes.push_back(candle.close);
        self.last_phase_flip = None;
        self.pending_node_creations.clear();

        if self.closes.len() > self.config.phase_window {
            self.closes.pop_front();
        }

        if self.closes.len() < self.config.phase_window {
            return None;
        }

        // find high/low closes over the window to build DM
        let mut iter = self.closes.iter().copied();
        let first = iter.next().expect("buffer should be non-empty here");
        let (lo, hi) = iter.fold((first, first), |(min, max), value| {
            (min.min(value), max.max(value))
        });

        let dm = (hi + lo) / 2.0;

        self.previous_dm = self.current_dm;
        self.current_dm = Some(dm);

        if let (Some(prev_dm), Some(curr_dm)) = (self.previous_dm, self.current_dm) {
            // DM slope drives phase changes - when the slope flips we set a new anchor and build nodes
            let new_phase = if curr_dm > prev_dm {
                Some(Phase::Bullish)
            } else if curr_dm < prev_dm {
                Some(Phase::Bearish)
            } else {
                self.current_phase
            };

            if let Some(new_phase) = new_phase {
                match self.current_phase {
                    None => {
                        // first time we have a slope, lock in phase and anchor
                        self.current_phase = Some(new_phase);
                        self.current_anchor = Some(curr_dm);

                        match new_phase {
                            Phase::Bullish => {
                                self.phase_high = Some(candle.high);
                                self.phase_low = None;
                            }
                            Phase::Bearish => {
                                self.phase_low = Some(candle.low);
                                self.phase_high = None;
                            }
                        }
                    }
                    Some(current_phase) if current_phase == new_phase => {
                        // same phase, refresh extremes only
                        match new_phase {
                            Phase::Bullish => {
                                let updated_high =
                                    self.phase_high.map_or(candle.high, |h| h.max(candle.high));
                                self.phase_high = Some(updated_high);
                                self.phase_low = None;
                            }
                            Phase::Bearish => {
                                let updated_low =
                                    self.phase_low.map_or(candle.low, |l| l.min(candle.low));
                                self.phase_low = Some(updated_low);
                                self.phase_high = None;
                            }
                        }
                    }
                    Some(old_phase) => {
                        // slope changed, flip phase and set new anchor plus node for the leg we just finished
                        match (old_phase, new_phase) {
                            (Phase::Bullish, Phase::Bearish) => {
                                if let Some(anchor_bull) = self.current_anchor {
                                    let highest_high = self.phase_high.unwrap_or(candle.high);
                                    let distance_pct = (highest_high - anchor_bull) / anchor_bull;
                                    self.last_phase_flip = Some(PhaseFlipInfo {
                                        from: old_phase,
                                        to: new_phase,
                                        old_anchor: anchor_bull,
                                        new_anchor: prev_dm,
                                        node_move_pct: distance_pct,
                                    });

                                    // turn the bullish leg extreme into a node above price
                                    self.pending_node_creations.push(NodeCreation {
                                        created_at: candle.timestamp_ms,
                                        anchor_at_creation: anchor_bull,
                                        extreme_at_creation: highest_high,
                                    });
                                }

                                self.current_anchor = Some(prev_dm);
                                self.current_phase = Some(new_phase);
                                self.phase_low = Some(candle.low);
                                self.phase_high = None;
                            }
                            (Phase::Bearish, Phase::Bullish) => {
                                if let Some(anchor_bear) = self.current_anchor {
                                    let lowest_low = self.phase_low.unwrap_or(candle.low);
                                    let distance_pct = (anchor_bear - lowest_low) / anchor_bear;
                                    self.last_phase_flip = Some(PhaseFlipInfo {
                                        from: old_phase,
                                        to: new_phase,
                                        old_anchor: anchor_bear,
                                        new_anchor: prev_dm,
                                        node_move_pct: distance_pct,
                                    });

                                    // turn the bearish leg extreme into a node below price
                                    self.pending_node_creations.push(NodeCreation {
                                        created_at: candle.timestamp_ms,
                                        anchor_at_creation: anchor_bear,
                                        extreme_at_creation: lowest_low,
                                    });
                                }

                                self.current_anchor = Some(prev_dm);
                                self.current_phase = Some(new_phase);
                                self.phase_high = Some(candle.high);
                                self.phase_low = None;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Some(dm)
    }

    pub fn current_phase(&self) -> Option<Phase> {
        self.current_phase
    }

    pub fn current_anchor(&self) -> Option<f64> {
        self.current_anchor
    }

    pub fn take_last_phase_flip(&mut self) -> Option<PhaseFlipInfo> {
        self.last_phase_flip.take()
    }

    pub(crate) fn take_node_creations(&mut self) -> Vec<NodeCreation> {
        mem::take(&mut self.pending_node_creations)
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn build_phase_update(&self, candle: &Candle) -> Option<PhaseUpdate> {
        let phase = self.current_phase?;
        let anchor = self.current_anchor?;
        let dm = self.current_dm?;

        // build the outbound payload only when state is ready
        Some(PhaseUpdate {
            update_type: "phase_update",
            symbol: self.symbol.clone(),
            time: candle.timestamp_ms,
            phase: phase.into(),
            anchor,
            dm,
            open: candle.open,
            high: candle.high,
            low: candle.low,
            close: candle.close,
            depth: self.config.depth_days,
        })
    }
}

/// Entry point used by the CLI binary.
pub fn run_cli() -> Result<()> {
    let cli = CliArgs::parse();
    let runtime = build_runtime_config(&cli)?;
    init_tracing(runtime.log_level.as_deref())?;

    info!(
        symbol = %runtime.symbol,
        csv = %runtime.csv.display(),
        phase_window = runtime.phase_window,
        depth_days = runtime.depth_days,
        timeframe = runtime.timeframe.as_deref().unwrap_or("none"),
        warmup_candles = runtime.warmup_candles,
        "Starting PhaseShifter CLI"
    );

    run_with_runtime(runtime)
}

pub fn run_with_runtime(config: RuntimeConfig) -> Result<()> {
    let engine_config = EngineConfig {
        side: PhaseSide::Long,
        timeframe: config.timeframe.clone().unwrap_or_else(|| "".to_string()),
        warmup_candles: config.warmup_candles,
        phase_window: config.phase_window,
        depth_days: config.depth_days,
    };

    let mut engine = PhaseEngine::new(config.symbol.clone(), engine_config);
    let source = CsvCandleSource::new(config.csv.clone())?;
    let mut out = prepare_output_writer(config.out_json.as_deref())?;
    let mut node_logger = NodeEventLogger::new(config.node_events_log.as_deref());

    let mut processed = 0usize;
    let mut bars: Vec<Bar> = Vec::new();
    let mut node_creations: Vec<NodeCreation> = Vec::new();
    for candle in source.into_iter() {
        let candle = candle?;
        processed += 1;

        let dm = engine.on_candle(&candle);
        log_candle(&engine, dm, &candle);

        if let Some(flip) = engine.take_last_phase_flip() {
            log_phase_flip(&flip, candle.timestamp_ms);
        }

        node_creations.extend(engine.take_node_creations());

        if let Some(anchor) = engine.current_anchor() {
            bars.push(Bar {
                timestamp: candle.timestamp_ms,
                open: candle.open,
                high: candle.high,
                low: candle.low,
                close: candle.close,
                anchor,
            });
        }

        if let Some(update) = engine.build_phase_update(&candle) {
            let line =
                serde_json::to_string(&update).context("failed to serialize phase_update")?;
            println!("{line}");

            if let Some(writer) = out.as_mut() {
                writeln!(writer, "{line}").context("failed to write phase_update to file")?;
            }
        }
    }

    let mut node_manager = build_nodes_from_history(bars, node_creations);
    for event in node_manager.take_recent_events() {
        log_node_event(&event);
        node_logger.log(config.symbol.as_str(), &event);
    }

    info!(
        processed,
        open_nodes = node_manager.open_nodes().len(),
        closed_nodes = node_manager.closed_nodes().len(),
        "Finished processing candles"
    );

    if let Some(anchor) = engine.current_anchor() {
        let projections = node_manager.current_projections(anchor);
        if !projections.is_empty() {
            debug!(
                anchor,
                projections = ?projections,
                "Current projections for open nodes"
            );
        }
    }

    if let Some(mut writer) = out {
        writer.flush().context("failed to flush output file")?;
    }
    node_logger.finish();
    Ok(())
}

fn init_tracing(log_level: Option<&str>) -> Result<()> {
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => {
            let fallback = log_level.unwrap_or("info");
            EnvFilter::try_new(fallback)?
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    Ok(())
}

fn log_candle(engine: &PhaseEngine, dm: Option<f64>, candle: &Candle) {
    if let Some(dm) = dm {
        debug!(
            symbol = engine.symbol(),
            ts = candle.timestamp_ms,
            close = candle.close,
            dm,
            phase = ?engine.current_phase(),
            anchor = ?engine.current_anchor(),
            "DM ready"
        );
    } else {
        trace!(
            symbol = engine.symbol(),
            ts = candle.timestamp_ms,
            close = candle.close,
            "Waiting for phase window to fill"
        );
    }
}

fn log_phase_flip(flip: &PhaseFlipInfo, candle_ts: i64) {
    info!(
        from = ?flip.from,
        to = ?flip.to,
        old_anchor = flip.old_anchor,
        new_anchor = flip.new_anchor,
        node_move_pct = flip.node_move_pct,
        at = %format_ts(candle_ts),
        "Phase flip"
    );
}

fn log_node_event(event: &NodeEvent) {
    match event {
        NodeEvent::Created(node) => {
            debug!(
                direction = ?node.direction,
                distance_pct = node.distance_pct,
                anchor = node.anchor_at_creation,
                extreme = node.extreme_at_creation,
                at = %format_ts(node.created_at),
                "Node created"
            );
        }
        NodeEvent::Closed {
            node,
            last_anchor_used,
            projected_price,
        } => {
            debug!(
                direction = ?node.direction,
                distance_pct = node.distance_pct,
                anchor = node.anchor_at_creation,
                extreme = node.extreme_at_creation,
                last_anchor_used,
                target = projected_price,
                opened = %format_ts(node.created_at),
                closed = %format_ts(node.closed_at.unwrap_or_default()),
                "Node closed"
            );
        }
    }
}

#[derive(Serialize)]
struct NodeEventPayload {
    #[serde(rename = "type")]
    update_type: &'static str,
    symbol: String,
    time: i64,
    event: &'static str,
    side: PhaseUpdatePhase,
    status: &'static str,
    distance_pct: f64,
    created_from_anchor: f64,
    created_from_extreme: f64,
    created_at: i64,
    closed_at: Option<i64>,
    last_anchor_used: Option<f64>,
    last_projected_target: Option<f64>,
}

impl NodeEventPayload {
    fn from_event(symbol: &str, event: &NodeEvent) -> Self {
        match event {
            NodeEvent::Created(node) => NodeEventPayload {
                update_type: "node_event",
                symbol: symbol.to_string(),
                time: node.created_at,
                event: "created",
                side: node.direction.into(),
                status: status_label(NodeState::Open),
                distance_pct: node.distance_pct,
                created_from_anchor: node.anchor_at_creation,
                created_from_extreme: node.extreme_at_creation,
                created_at: node.created_at,
                closed_at: None,
                last_anchor_used: None,
                last_projected_target: None,
            },
            NodeEvent::Closed {
                node,
                last_anchor_used,
                projected_price,
            } => NodeEventPayload {
                update_type: "node_event",
                symbol: symbol.to_string(),
                time: node.closed_at.unwrap_or(node.created_at),
                event: "closed",
                side: node.direction.into(),
                status: status_label(NodeState::Closed),
                distance_pct: node.distance_pct,
                created_from_anchor: node.anchor_at_creation,
                created_from_extreme: node.extreme_at_creation,
                created_at: node.created_at,
                closed_at: node.closed_at,
                last_anchor_used: Some(*last_anchor_used),
                last_projected_target: Some(*projected_price),
            },
        }
    }
}

struct NodeEventLogger {
    writer: Option<BufWriter<File>>,
}

impl NodeEventLogger {
    fn new(path: Option<&Path>) -> Self {
        let writer =
            path.and_then(
                |p| match OpenOptions::new().create(true).append(true).open(p) {
                    Ok(file) => Some(BufWriter::new(file)),
                    Err(err) => {
                        warn!(error = %err, path = %p.display(), "Failed to open node events log");
                        None
                    }
                },
            );

        Self { writer }
    }

    fn log(&mut self, symbol: &str, event: &NodeEvent) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };

        let payload = NodeEventPayload::from_event(symbol, event);
        match serde_json::to_string(&payload) {
            Ok(line) => {
                if let Err(err) = writeln!(writer, "{line}") {
                    warn!(error = %err, "Failed to write node event");
                    self.writer = None;
                }
            }
            Err(err) => {
                warn!(error = %err, "Failed to serialize node event");
            }
        }
    }

    fn finish(&mut self) {
        if let Some(writer) = self.writer.as_mut() {
            if let Err(err) = writer.flush() {
                warn!(error = %err, "Failed to flush node events log");
            }
        }
    }
}

fn status_label(status: NodeState) -> &'static str {
    match status {
        NodeState::Open => "open",
        NodeState::Closed => "closed",
    }
}

fn format_ts(ts_ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ts_ms)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| ts_ms.to_string())
}

fn prepare_output_writer(path: Option<&Path>) -> Result<Option<BufWriter<File>>> {
    let Some(path) = path else {
        return Ok(None);
    };

    let file = File::create(path)
        .with_context(|| format!("failed to create output file at {}", path.display()))?;
    Ok(Some(BufWriter::new(file)))
}
