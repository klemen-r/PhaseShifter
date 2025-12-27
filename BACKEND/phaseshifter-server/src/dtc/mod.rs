//! DTC Protocol implementation for Sierra Chart
//!
//! Reference: https://dtcprotocol.org/
//! Sierra Chart uses binary variable-length encoding.

mod client;
mod protocol;

pub use client::{DtcClient, DtcClientConfig, HistoricalBar, Tick};
pub use protocol::{HistoricalDataInterval, MessageType};
