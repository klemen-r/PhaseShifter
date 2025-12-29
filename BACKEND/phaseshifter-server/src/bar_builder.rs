//! Multi-timeframe bar builder
//!
//! Constructs OHLCV bars from tick data for multiple timeframes simultaneously.
//! Uses lock-free data structures for high-performance tick processing.
//! Supports async SCID verification of closed bars.

use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

use crate::dtc::Tick;

/// Supported timeframes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    M1,  // 1 minute
    M5,  // 5 minutes
    M15, // 15 minutes
    M30, // 30 minutes
    H1,  // 1 hour
    H4,  // 4 hours
    D1,  // 1 day
}

impl Timeframe {
    /// Duration in milliseconds
    pub fn millis(&self) -> i64 {
        match self {
            Timeframe::M1 => 60_000,
            Timeframe::M5 => 300_000,
            Timeframe::M15 => 900_000,
            Timeframe::M30 => 1_800_000,
            Timeframe::H1 => 3_600_000,
            Timeframe::H4 => 14_400_000,
            Timeframe::D1 => 86_400_000,
        }
    }

    /// Duration in seconds
    pub fn seconds(&self) -> i64 {
        self.millis() / 1000
    }

    /// String representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::M1 => "1m",
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::M30 => "30m",
            Timeframe::H1 => "1h",
            Timeframe::H4 => "4h",
            Timeframe::D1 => "1d",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "1m" => Some(Timeframe::M1),
            "5m" => Some(Timeframe::M5),
            "15m" => Some(Timeframe::M15),
            "30m" => Some(Timeframe::M30),
            "1h" => Some(Timeframe::H1),
            "4h" => Some(Timeframe::H4),
            "1d" => Some(Timeframe::D1),
            _ => None,
        }
    }

    /// Default timeframes for analysis
    pub fn default_set() -> Vec<Self> {
        vec![Timeframe::M1, Timeframe::M5, Timeframe::M15]
    }
}

/// OHLCV Bar
#[derive(Debug, Clone, Serialize)]
pub struct Bar {
    pub symbol: String,
    pub timeframe: String,
    #[serde(rename = "time")]
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    #[serde(skip)]
    pub tick_count: u32,
    #[serde(skip)]
    pub is_closed: bool,
}

impl Bar {
    /// Create a new bar from a tick
    fn new(symbol: &str, timeframe: Timeframe, timestamp_ms: i64, price: f64, volume: f64) -> Self {
        Self {
            symbol: symbol.to_string(),
            timeframe: timeframe.as_str().to_string(),
            timestamp_ms,
            open: price,
            high: price,
            low: price,
            close: price,
            volume,
            tick_count: 1,
            is_closed: false,
        }
    }

    /// Update bar with a new tick
    fn update(&mut self, price: f64, volume: f64) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
        self.volume += volume;
        self.tick_count += 1;
    }

    /// Mark bar as closed
    fn close(&mut self) {
        self.is_closed = true;
    }
}

/// Bar event (for broadcasting)
#[derive(Debug, Clone)]
pub enum BarEvent {
    /// Bar was updated (tick received)
    Updated(Bar),
    /// Bar closed (new bar started)
    Closed(Bar),
}

/// Request to verify a closed bar against SCID data
#[derive(Debug, Clone)]
pub struct BarVerifyRequest {
    pub bar: Bar,
    pub sierra_symbol: String, // e.g., "NQH26-CME"
}

/// Result of SCID verification
#[derive(Debug, Clone)]
pub struct BarVerifyResult {
    pub bar: Bar,
    pub scid_bar: Option<Bar>,
    pub matched: bool,
    pub diff: Option<BarDiff>,
}

/// Difference between live bar and SCID bar
#[derive(Debug, Clone, Serialize)]
pub struct BarDiff {
    pub open_diff: f64,
    pub high_diff: f64,
    pub low_diff: f64,
    pub close_diff: f64,
    pub volume_diff: f64,
}

/// State for a single symbol-timeframe combination
struct BarState {
    current_bar: Option<Bar>,
    history: Vec<Bar>,
    max_history: usize,
}

impl BarState {
    fn new(max_history: usize) -> Self {
        Self {
            current_bar: None,
            history: Vec::with_capacity(max_history),
            max_history,
        }
    }

    /// Process a tick, returns any events generated
    fn on_tick(
        &mut self,
        symbol: &str,
        timeframe: Timeframe,
        price: f64,
        volume: f64,
        timestamp_ms: i64,
    ) -> Vec<BarEvent> {
        let mut events = Vec::new();
        let bar_open_time = get_bar_open_time(timestamp_ms, timeframe);

        // Check if we need to close current bar and start new one
        if let Some(ref current) = self.current_bar {
            if current.timestamp_ms < bar_open_time {
                // Close current bar
                info!(
                    "[BAR_BUILDER] Closing {} bar for {}: old_time={} -> new_time={}",
                    timeframe.as_str(),
                    symbol,
                    current.timestamp_ms,
                    bar_open_time
                );
                let mut closed_bar = current.clone();
                closed_bar.close();

                // Add to history before pushing event to avoid extra clone
                self.history.push(closed_bar.clone());
                if self.history.len() > self.max_history {
                    // Use VecDeque's efficient pop_front or drain to avoid O(n) removal
                    self.history.drain(0..1);
                }

                events.push(BarEvent::Closed(closed_bar));
                self.current_bar = None;
            }
        }

        // Update or create current bar
        match &mut self.current_bar {
            Some(bar) => {
                bar.update(price, volume);
                events.push(BarEvent::Updated(bar.clone()));
            }
            None => {
                let bar = Bar::new(symbol, timeframe, bar_open_time, price, volume);
                events.push(BarEvent::Updated(bar.clone()));
                self.current_bar = Some(bar);
            }
        }

        events
    }

    /// Get all bars (history + current)
    fn get_all_bars(&self) -> Vec<Bar> {
        let mut bars = self.history.clone();
        if let Some(ref current) = self.current_bar {
            bars.push(current.clone());
        }
        bars
    }
}

/// Calculate bar open time aligned to timeframe boundaries
fn get_bar_open_time(timestamp_ms: i64, timeframe: Timeframe) -> i64 {
    let tf_ms = timeframe.millis();
    (timestamp_ms / tf_ms) * tf_ms
}

/// Multi-timeframe bar builder
pub struct BarBuilder {
    timeframes: Vec<Timeframe>,
    /// State: symbol -> timeframe -> BarState
    states: HashMap<String, HashMap<Timeframe, BarState>>,
    max_history: usize,
    /// Channel for bar events
    event_sender: mpsc::Sender<BarEvent>,
    /// Optional channel for SCID verification requests
    verify_sender: Option<mpsc::Sender<BarVerifyRequest>>,
    /// Symbol mapping: base symbol -> Sierra symbol (e.g., "NQ" -> "NQH26-CME")
    symbol_to_sierra: HashMap<String, String>,
    /// Live mode flag - when true, closed bars are sent for SCID verification
    /// Set to false during historical data loading, true after loading completes
    live_mode: bool,
    /// Statistics
    tick_count: u64,
    bars_closed: u64,
}

impl BarBuilder {
    /// Create a new bar builder
    pub fn new(timeframes: Vec<Timeframe>, event_sender: mpsc::Sender<BarEvent>) -> Self {
        Self {
            timeframes,
            states: HashMap::new(),
            max_history: 500,
            event_sender,
            verify_sender: None,
            symbol_to_sierra: HashMap::new(),
            live_mode: false, // Start in historical mode
            tick_count: 0,
            bars_closed: 0,
        }
    }

    /// Set the verification channel for SCID validation
    pub fn set_verify_sender(&mut self, sender: mpsc::Sender<BarVerifyRequest>) {
        self.verify_sender = Some(sender);
    }

    /// Enable live mode - bars will be sent for SCID verification
    /// Call this after historical data loading is complete
    pub fn set_live_mode(&mut self, enabled: bool) {
        self.live_mode = enabled;
        if enabled {
            info!("BarBuilder entering live mode - SCID verification enabled");
        } else {
            info!("BarBuilder entering historical mode - SCID verification disabled");
        }
    }

    /// Check if in live mode
    pub fn is_live_mode(&self) -> bool {
        self.live_mode
    }

    /// Register a symbol mapping (base -> Sierra)
    pub fn register_symbol_mapping(&mut self, base: &str, sierra: &str) {
        self.symbol_to_sierra
            .insert(base.to_string(), sierra.to_string());
        debug!("Registered symbol mapping: {} -> {}", base, sierra);
    }

    /// Process a tick
    pub async fn on_tick(&mut self, tick: &Tick) {
        self.tick_count += 1;
        let timestamp_ms = (tick.timestamp * 1000.0) as i64;

        // Ensure symbol states exist
        let tf_states = self.states.entry(tick.symbol.clone()).or_insert_with(|| {
            let mut states = HashMap::new();
            for &tf in &self.timeframes {
                states.insert(tf, BarState::new(self.max_history));
            }
            states
        });

        // Update each timeframe
        for &tf in &self.timeframes {
            if let Some(state) = tf_states.get_mut(&tf) {
                let events = state.on_tick(&tick.symbol, tf, tick.price, tick.volume, timestamp_ms);

                for event in events {
                    // Send verification request for closed M1 bars (only in live mode)
                    if let BarEvent::Closed(ref bar) = event {
                        self.bars_closed += 1;

                        // Only verify M1 bars in live mode to avoid verifying historical data
                        if tf == Timeframe::M1 && self.live_mode {
                            if let Some(ref verify_tx) = self.verify_sender {
                                let sierra_symbol = self
                                    .symbol_to_sierra
                                    .get(&tick.symbol)
                                    .cloned()
                                    .unwrap_or_else(|| tick.symbol.clone());

                                let req = BarVerifyRequest {
                                    bar: bar.clone(),
                                    sierra_symbol,
                                };
                                // Non-blocking send - if channel full, skip verification
                                if verify_tx.try_send(req).is_err() {
                                    warn!(
                                        "Verify channel full, skipping SCID verification for {}",
                                        bar.symbol
                                    );
                                }
                            }
                        }
                    }

                    // Send event - use blocking send to ensure no events are dropped
                    if let Err(e) = self.event_sender.send(event).await {
                        warn!("Failed to send bar event: {}", e);
                    }
                }
            }
        }

        trace!(
            "Tick processed: {} @ {:.2} (ticks={}, bars_closed={})",
            tick.symbol,
            tick.price,
            self.tick_count,
            self.bars_closed
        );
    }

    /// Get all bars for a symbol and timeframe
    pub fn get_bars(&self, symbol: &str, timeframe: Timeframe) -> Vec<Bar> {
        self.states
            .get(symbol)
            .and_then(|tf_states| tf_states.get(&timeframe))
            .map(|state| state.get_all_bars())
            .unwrap_or_default()
    }

    /// Get current bar for a symbol and timeframe
    pub fn get_current_bar(&self, symbol: &str, timeframe: Timeframe) -> Option<Bar> {
        self.states
            .get(symbol)
            .and_then(|tf_states| tf_states.get(&timeframe))
            .and_then(|state| state.current_bar.clone())
    }

    /// Get all tracked symbols
    pub fn get_symbols(&self) -> Vec<String> {
        self.states.keys().cloned().collect()
    }

    /// Get statistics
    pub fn stats(&self) -> BarBuilderStats {
        BarBuilderStats {
            tick_count: self.tick_count,
            bars_closed: self.bars_closed,
            symbols: self.states.keys().cloned().collect(),
            timeframes: self
                .timeframes
                .iter()
                .map(|tf| tf.as_str().to_string())
                .collect(),
        }
    }

    /// Add a bar directly (for external data sources)
    /// This stores the bar in history without going through tick processing
    pub fn add_bar_directly(&mut self, symbol: &str, timeframe: Timeframe, bar: Bar) {
        let tf_states = self
            .states
            .entry(symbol.to_string())
            .or_insert_with(HashMap::new);

        let state = tf_states
            .entry(timeframe)
            .or_insert_with(|| BarState::new(self.max_history));

        // Add to history, maintaining max_history limit
        state.history.push(bar);
        if state.history.len() > self.max_history {
            state.history.remove(0);
        }
    }

    /// Clear data for a symbol
    pub fn clear_symbol(&mut self, symbol: &str) {
        self.states.remove(symbol);
    }

    /// Clear all data
    pub fn clear_all(&mut self) {
        self.states.clear();
        self.tick_count = 0;
        self.bars_closed = 0;
    }
}

/// Bar builder statistics
#[derive(Debug, Clone, Serialize)]
pub struct BarBuilderStats {
    pub tick_count: u64,
    pub bars_closed: u64,
    pub symbols: Vec<String>,
    pub timeframes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_open_time() {
        // Test 1-minute bars
        let ts = 1704067200000i64; // 2024-01-01 00:00:00 UTC
        assert_eq!(get_bar_open_time(ts, Timeframe::M1), ts);
        assert_eq!(get_bar_open_time(ts + 30_000, Timeframe::M1), ts);
        assert_eq!(get_bar_open_time(ts + 60_000, Timeframe::M1), ts + 60_000);

        // Test 5-minute bars
        assert_eq!(get_bar_open_time(ts, Timeframe::M5), ts);
        assert_eq!(get_bar_open_time(ts + 180_000, Timeframe::M5), ts);
        assert_eq!(get_bar_open_time(ts + 300_000, Timeframe::M5), ts + 300_000);
    }

    #[test]
    fn test_bar_update() {
        let mut bar = Bar::new("NQ", Timeframe::M1, 1704067200000, 100.0, 10.0);

        bar.update(105.0, 5.0);
        assert_eq!(bar.high, 105.0);
        assert_eq!(bar.low, 100.0);
        assert_eq!(bar.close, 105.0);
        assert_eq!(bar.volume, 15.0);
        assert_eq!(bar.tick_count, 2);

        bar.update(98.0, 3.0);
        assert_eq!(bar.high, 105.0);
        assert_eq!(bar.low, 98.0);
        assert_eq!(bar.close, 98.0);
        assert_eq!(bar.volume, 18.0);
        assert_eq!(bar.tick_count, 3);
    }
}
