//! Shared tick type used by the bar builder and streaming paths.

/// Normalized tick data for bar building.
#[derive(Debug, Clone)]
pub struct Tick {
    pub symbol: String,
    pub symbol_id: u16,
    pub price: f64,
    pub volume: f64,
    /// Unix timestamp in seconds.
    pub timestamp: f64,
    pub at_bid_or_ask: u8,
    pub bid_price: f64,
    pub ask_price: f64,
}
