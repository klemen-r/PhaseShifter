//! yfinance data poller for non-Sierra symbols
//!
//! Polls Yahoo Finance API to get OHLCV data for symbols
//! that aren't streamed via Sierra Chart (e.g., BTC-USD, AAPL, SPY).
//!
//! Uses intelligent fallback: tries 1m first, falls back to 5m if data
//! appears "flat" (≥90% candles have high == low).
//!
//! The fetched bars are converted to the same format as Sierra bars
//! and fed into the same BarBuilder → PhaseEngine pipeline.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::bar_builder::Bar;

/// Primary interval to try first
const PRIMARY_INTERVAL: &str = "1m";
/// Fallback interval if primary data looks flat
const FALLBACK_INTERVAL: &str = "5m";
/// If this ratio of candles have high == low, data is considered flat
const FLAT_RATIO_THRESHOLD: f64 = 0.9;
/// Minimum samples needed to determine if data is flat
const FLAT_MIN_SAMPLES: usize = 30;

/// Configuration for the yfinance poller
#[derive(Debug, Clone)]
pub struct YfinanceConfig {
    /// How often to poll (in seconds), default 60 (1 minute for 1m candles)
    pub poll_interval_secs: u64,
    /// How many bars to fetch per request (default 5)
    pub count: u32,
}

impl Default for YfinanceConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 60, // 1 minute polling for live updates
            count: 5,
        }
    }
}

/// Yahoo Finance chart response structures
#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooResult>>,
    error: Option<YahooError>,
}

#[derive(Debug, Deserialize)]
struct YahooError {
    code: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct YahooResult {
    meta: YahooMeta,
    timestamp: Option<Vec<i64>>,
    indicators: YahooIndicators,
}

#[derive(Debug, Deserialize)]
struct YahooMeta {
    symbol: String,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    open: Vec<Option<f64>>,
    high: Vec<Option<f64>>,
    low: Vec<Option<f64>>,
    close: Vec<Option<f64>>,
    volume: Vec<Option<i64>>,
}

/// A single OHLCV bar from yfinance
#[derive(Debug, Clone)]
pub struct YfinanceBar {
    pub symbol: String,
    pub timeframe: String,
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl YfinanceBar {
    /// Convert to the standard Bar format used by BarBuilder
    pub fn to_bar(&self) -> Bar {
        Bar {
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.clone(),
            timestamp_ms: self.timestamp_ms,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
            tick_count: 1,
            is_closed: true,
        }
    }
}

/// Tracks state for each subscribed symbol
#[derive(Debug)]
struct SymbolState {
    /// Last bar timestamp we received (to avoid duplicates)
    last_bar_time: Option<i64>,
    /// Whether initial history has been sent
    history_sent: bool,
    /// Preferred interval for this symbol (primary or fallback)
    preferred_interval: String,
}

impl Default for SymbolState {
    fn default() -> Self {
        Self {
            last_bar_time: None,
            history_sent: false,
            preferred_interval: PRIMARY_INTERVAL.to_string(),
        }
    }
}

/// yfinance data poller
pub struct YfinancePoller {
    config: YfinanceConfig,
    /// Symbols currently subscribed
    subscribed: Arc<RwLock<HashSet<String>>>,
    /// Per-symbol state
    states: Arc<RwLock<HashMap<String, SymbolState>>>,
    /// Channel to send bars
    bar_sender: mpsc::Sender<YfinanceBar>,
    /// HTTP client
    client: reqwest::Client,
}

impl YfinancePoller {
    pub fn new(config: YfinanceConfig, bar_sender: mpsc::Sender<YfinanceBar>) -> Self {
        Self {
            config,
            subscribed: Arc::new(RwLock::new(HashSet::new())),
            states: Arc::new(RwLock::new(HashMap::new())),
            bar_sender,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Subscribe to a symbol
    pub async fn subscribe(&self, symbol: &str) {
        let symbol = symbol.to_uppercase();
        let mut subscribed = self.subscribed.write().await;
        if subscribed.insert(symbol.clone()) {
            info!("[yfinance] Subscribed to {}", symbol);
            // Initialize state
            let mut states = self.states.write().await;
            states.entry(symbol.clone()).or_default();

            // Fetch initial history immediately
            drop(subscribed);
            drop(states);
            if let Err(e) = self.fetch_and_send_bars(&symbol, true).await {
                warn!(
                    "[yfinance] Failed to fetch initial data for {}: {}",
                    symbol, e
                );
            }
        }
    }

    /// Unsubscribe from a symbol
    pub async fn unsubscribe(&self, symbol: &str) {
        let symbol = symbol.to_uppercase();
        let mut subscribed = self.subscribed.write().await;
        if subscribed.remove(&symbol) {
            info!("[yfinance] Unsubscribed from {}", symbol);
            let mut states = self.states.write().await;
            states.remove(&symbol);
        }
    }

    /// Get list of subscribed symbols
    pub async fn get_subscribed(&self) -> Vec<String> {
        self.subscribed.read().await.iter().cloned().collect()
    }

    /// Check if a symbol is subscribed
    pub async fn is_subscribed(&self, symbol: &str) -> bool {
        self.subscribed
            .read()
            .await
            .contains(&symbol.to_uppercase())
    }

    /// Get the preferred interval for a symbol
    pub async fn get_preferred_interval(&self, symbol: &str) -> String {
        let states = self.states.read().await;
        states
            .get(&symbol.to_uppercase())
            .map(|s| s.preferred_interval.clone())
            .unwrap_or_else(|| PRIMARY_INTERVAL.to_string())
    }

    /// Main polling loop - runs forever, polling every N seconds
    pub async fn run(&self) {
        let mut ticker = interval(Duration::from_secs(self.config.poll_interval_secs));

        info!(
            "[yfinance] Starting poller with {}s interval (primary: {}, fallback: {})",
            self.config.poll_interval_secs, PRIMARY_INTERVAL, FALLBACK_INTERVAL
        );

        loop {
            ticker.tick().await;

            let symbols: Vec<String> = self.subscribed.read().await.iter().cloned().collect();

            if symbols.is_empty() {
                continue;
            }

            debug!("[yfinance] Polling {} symbols", symbols.len());

            for symbol in symbols {
                if let Err(e) = self.fetch_and_send_bars(&symbol, false).await {
                    warn!("[yfinance] Failed to fetch {}: {}", symbol, e);
                }
            }
        }
    }

    /// Fetch bars from yfinance and send new ones
    async fn fetch_and_send_bars(&self, symbol: &str, is_initial: bool) -> Result<()> {
        // Get current preferred interval
        let interval = {
            let states = self.states.read().await;
            states
                .get(symbol)
                .map(|s| s.preferred_interval.clone())
                .unwrap_or_else(|| PRIMARY_INTERVAL.to_string())
        };

        // Fetch bars with preferred interval
        let (bars, used_interval) = self.fetch_bars_with_fallback(symbol, &interval).await?;

        if bars.is_empty() {
            debug!("[yfinance] No bars returned for {}", symbol);
            return Ok(());
        }

        // Update preferred interval if fallback was used
        if used_interval != interval {
            let mut states = self.states.write().await;
            if let Some(state) = states.get_mut(symbol) {
                info!(
                    "[yfinance] {} switching from {} to {} candles",
                    symbol, interval, used_interval
                );
                state.preferred_interval = used_interval.clone();
            }
        }

        let mut states = self.states.write().await;
        let state = states.entry(symbol.to_string()).or_default();

        let mut new_bars = 0;
        for bar in bars {
            // Skip bars we've already seen
            if let Some(last_time) = state.last_bar_time {
                if bar.timestamp_ms <= last_time && !is_initial {
                    continue;
                }
            }

            // Send the bar
            if self.bar_sender.send(bar.clone()).await.is_err() {
                error!("[yfinance] Bar channel closed");
                return Err(anyhow!("Bar channel closed"));
            }

            state.last_bar_time = Some(bar.timestamp_ms);
            new_bars += 1;
        }

        if new_bars > 0 {
            debug!("[yfinance] Sent {} new bars for {}", new_bars, symbol);
        }

        if is_initial {
            state.history_sent = true;
        }

        Ok(())
    }

    /// Fetch OHLCV bars from Yahoo Finance API with automatic fallback
    async fn fetch_bars_with_fallback(
        &self,
        symbol: &str,
        interval: &str,
    ) -> Result<(Vec<YfinanceBar>, String)> {
        // Try the preferred interval first
        let bars = self.fetch_bars(symbol, interval).await?;

        // Check if we should fallback (only for primary interval)
        if interval == PRIMARY_INTERVAL && self.should_fallback(&bars) {
            info!(
                "[yfinance] {} {} data looks flat; switching to {} candles",
                symbol, PRIMARY_INTERVAL, FALLBACK_INTERVAL
            );
            let fallback_bars = self.fetch_bars(symbol, FALLBACK_INTERVAL).await?;
            return Ok((fallback_bars, FALLBACK_INTERVAL.to_string()));
        }

        Ok((bars, interval.to_string()))
    }

    /// Check if data should fallback to 5m (high == low for ≥90% of candles)
    fn should_fallback(&self, bars: &[YfinanceBar]) -> bool {
        if bars.len() < FLAT_MIN_SAMPLES {
            return false;
        }

        let flat_count = bars
            .iter()
            .filter(|b| (b.high - b.low).abs() < f64::EPSILON)
            .count();
        let flat_ratio = flat_count as f64 / bars.len() as f64;

        flat_ratio >= FLAT_RATIO_THRESHOLD
    }

    /// Fetch OHLCV bars from Yahoo Finance API
    async fn fetch_bars(&self, symbol: &str, yf_interval: &str) -> Result<Vec<YfinanceBar>> {
        // Yahoo Finance API URL
        // range=1d gets last day of data
        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={}&range=1d",
            symbol, yf_interval
        );

        debug!("[yfinance] Fetching: {}", url);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Yahoo Finance API error: {} for {}",
                response.status(),
                symbol
            ));
        }

        let data: YahooResponse = response.json().await?;

        // Check for API errors
        if let Some(error) = data.chart.error {
            return Err(anyhow!(
                "Yahoo Finance error for {}: {} - {}",
                symbol,
                error.code,
                error.description
            ));
        }

        // Extract result
        let result = data
            .chart
            .result
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| anyhow!("No data returned for {}", symbol))?;

        let timestamps = result
            .timestamp
            .ok_or_else(|| anyhow!("No timestamps for {}", symbol))?;

        let quote = result
            .indicators
            .quote
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No quotes for {}", symbol))?;

        // Build bars from the response
        let mut bars = Vec::new();
        for (i, &ts) in timestamps.iter().enumerate() {
            // Skip if any OHLC value is missing
            let open = match quote.open.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let high = match quote.high.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let low = match quote.low.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let close = match quote.close.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let volume = quote.volume.get(i).and_then(|v| *v).unwrap_or(0) as f64;

            bars.push(YfinanceBar {
                symbol: symbol.to_string(),
                timeframe: yf_interval.to_string(),
                timestamp_ms: ts * 1000, // Convert seconds to milliseconds
                open,
                high,
                low,
                close,
                volume,
            });
        }

        // Sort by timestamp and take only the latest N bars
        bars.sort_by_key(|b| b.timestamp_ms);

        // For initial fetch, keep more history; for updates, just recent bars
        let keep_count = if bars.len() > 100 { 100 } else { bars.len() };
        let bars: Vec<_> = bars.into_iter().rev().take(keep_count).collect();
        let bars: Vec<_> = bars.into_iter().rev().collect(); // Reverse back to chronological

        Ok(bars)
    }

    /// Fetch historical data for multiple timeframes (for pipeline/clustering)
    /// Returns bars for: 1m (8 days), 5m (50 days), 15m (60 days)
    pub async fn fetch_pipeline_data(
        &self,
        symbol: &str,
    ) -> Result<HashMap<String, Vec<YfinanceBar>>> {
        let mut result = HashMap::new();

        // Pipeline scenarios from Python implementation:
        // {"interval": "1m", "days": 8, ...},
        // {"interval": "5m", "days": 50, ...},
        // {"interval": "5m", "days": 50, ...},  // Same data, different phase_window
        // {"interval": "15m", "days": 60, ...}

        // Fetch 1m data (8 days in Python, but yfinance limits to 7d for 1m interval)
        // Python: {"interval": "1m", "days": 8, "phase_window": 50, "depth_days": 8}
        match self.fetch_bars_range(symbol, "1m", "7d").await {
            Ok(bars) => {
                info!(
                    "[yfinance] Fetched {} 1m bars for {} pipeline",
                    bars.len(),
                    symbol
                );
                result.insert("1m".to_string(), bars);
            }
            Err(e) => {
                warn!(
                    "[yfinance] Failed to fetch 1m pipeline data for {}: {}",
                    symbol, e
                );
            }
        }

        // Fetch 5m data (50 days - matches Python: {"interval": "5m", "days": 50, ...})
        match self.fetch_bars_range(symbol, "5m", "50d").await {
            Ok(bars) => {
                info!(
                    "[yfinance] Fetched {} 5m bars for {} pipeline",
                    bars.len(),
                    symbol
                );
                result.insert("5m".to_string(), bars);
            }
            Err(e) => {
                warn!(
                    "[yfinance] Failed to fetch 5m pipeline data for {}: {}",
                    symbol, e
                );
            }
        }

        // Fetch 15m data (60 days - matches Python: {"interval": "15m", "days": 60, ...})
        match self.fetch_bars_range(symbol, "15m", "60d").await {
            Ok(bars) => {
                info!(
                    "[yfinance] Fetched {} 15m bars for {} pipeline",
                    bars.len(),
                    symbol
                );
                result.insert("15m".to_string(), bars);
            }
            Err(e) => {
                warn!(
                    "[yfinance] Failed to fetch 15m pipeline data for {}: {}",
                    symbol, e
                );
            }
        }

        Ok(result)
    }

    /// Fetch bars for a specific range
    async fn fetch_bars_range(
        &self,
        symbol: &str,
        yf_interval: &str,
        range: &str,
    ) -> Result<Vec<YfinanceBar>> {
        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={}&range={}",
            symbol, yf_interval, range
        );

        debug!("[yfinance] Fetching pipeline data: {}", url);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Yahoo Finance API error: {} for {}",
                response.status(),
                symbol
            ));
        }

        let data: YahooResponse = response.json().await?;

        // Check for API errors
        if let Some(error) = data.chart.error {
            return Err(anyhow!(
                "Yahoo Finance error for {}: {} - {}",
                symbol,
                error.code,
                error.description
            ));
        }

        // Extract result
        let result = data
            .chart
            .result
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| anyhow!("No data returned for {}", symbol))?;

        let timestamps = result
            .timestamp
            .ok_or_else(|| anyhow!("No timestamps for {}", symbol))?;

        let quote = result
            .indicators
            .quote
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No quotes for {}", symbol))?;

        // Build bars from the response
        let mut bars = Vec::new();
        for (i, &ts) in timestamps.iter().enumerate() {
            // Skip if any OHLC value is missing
            let open = match quote.open.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let high = match quote.high.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let low = match quote.low.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let close = match quote.close.get(i).and_then(|v| *v) {
                Some(v) => v,
                None => continue,
            };
            let volume = quote.volume.get(i).and_then(|v| *v).unwrap_or(0) as f64;

            bars.push(YfinanceBar {
                symbol: symbol.to_string(),
                timeframe: yf_interval.to_string(),
                timestamp_ms: ts * 1000, // Convert seconds to milliseconds
                open,
                high,
                low,
                close,
                volume,
            });
        }

        // Sort by timestamp
        bars.sort_by_key(|b| b.timestamp_ms);

        Ok(bars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_yfinance_fetch() {
        let (tx, _rx) = mpsc::channel(100);
        let poller = YfinancePoller::new(YfinanceConfig::default(), tx);

        // Test fetching a common symbol
        let result = poller.fetch_bars("AAPL", "1m").await;

        // This might fail if network is unavailable, so just check the structure
        match result {
            Ok(bars) => {
                println!("Fetched {} bars for AAPL", bars.len());
                if let Some(bar) = bars.last() {
                    println!("Latest bar: {:?}", bar);
                    assert!(bar.open > 0.0);
                    assert!(bar.high >= bar.low);
                    assert!(bar.close > 0.0);
                }
            }
            Err(e) => {
                println!("Network error (expected in CI): {}", e);
            }
        }
    }

    #[test]
    fn test_should_fallback() {
        let (tx, _rx) = mpsc::channel(100);
        let poller = YfinancePoller::new(YfinanceConfig::default(), tx);

        // Create flat data (high == low for all bars)
        let flat_bars: Vec<YfinanceBar> = (0..50)
            .map(|i| YfinanceBar {
                symbol: "TEST".to_string(),
                timeframe: "1m".to_string(),
                timestamp_ms: i * 60000,
                open: 100.0,
                high: 100.0, // Same as low
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
            })
            .collect();

        assert!(poller.should_fallback(&flat_bars));

        // Create normal data with price movement
        let normal_bars: Vec<YfinanceBar> = (0..50)
            .map(|i| YfinanceBar {
                symbol: "TEST".to_string(),
                timeframe: "1m".to_string(),
                timestamp_ms: i * 60000,
                open: 100.0,
                high: 101.0, // Different from low
                low: 99.0,
                close: 100.5,
                volume: 1000.0,
            })
            .collect();

        assert!(!poller.should_fallback(&normal_bars));

        // Too few samples
        let small_bars: Vec<YfinanceBar> = (0..10)
            .map(|i| YfinanceBar {
                symbol: "TEST".to_string(),
                timeframe: "1m".to_string(),
                timestamp_ms: i * 60000,
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
            })
            .collect();

        assert!(!poller.should_fallback(&small_bars)); // Not enough samples
    }
}
