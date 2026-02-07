//! PhaseShifter Real-Time Streaming Server
//!
//! Streams real-time market data to frontend clients via WebSocket.
//!
//! Accepts direct TCP connections from ACSIL studies for live CME data.
//!
//! Usage:
//!     phaseshifter-server --ws-port 8000

use anyhow::Result;
use clap::Parser;
use std::time::Instant;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod bar_builder;
mod clusters;
mod contracts;
mod scid;
mod server;
mod sierra_tcp;
mod tick;

use server::Server;

#[derive(Debug, Parser)]
#[command(
    name = "phaseshifter-server",
    version,
    about = "Real-time streaming server for PhaseShifter"
)]
struct Args {
    /// WebSocket server port for frontend connections
    #[arg(long, default_value = "8000")]
    ws_port: u16,

    /// WebSocket server host
    #[arg(long, default_value = "127.0.0.1")]
    ws_host: String,

    /// Symbols to subscribe to (comma-separated, e.g., "NQ,ES")
    #[arg(long, default_value = "NQ,ES")]
    symbols: String,

    /// Sierra Chart data folder (for loading historical SCID files)
    #[arg(
        long,
        default_value = "/Users/urban/.wine-sierra/drive_c/SierraChart/Data"
    )]
    sierra_data_folder: String,

    /// Sierra TCP port for ACSIL study connections (live tick data)
    #[arg(long, default_value = "9000")]
    sierra_tcp_port: u16,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let start = Instant::now();
    let args = Args::parse();

    // Initialize logging
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    info!("╔════════════════════════════════════════════════════════════╗");
    info!("║          PhaseShifter Real-Time Streaming Server           ║");
    info!("╠════════════════════════════════════════════════════════════╣");
    info!(
        "║  Sierra TCP:   127.0.0.1:{}  (ACSIL live data)            ║",
        args.sierra_tcp_port
    );
    info!(
        "║  WebSocket:    ws://{}:{}                          ║",
        args.ws_host, args.ws_port
    );
    info!("║  Symbols:      {:41}  ║", args.symbols);
    info!("╚════════════════════════════════════════════════════════════╝");

    // Parse symbols
    let symbols: Vec<String> = args
        .symbols
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();

    let elapsed = start.elapsed();
    info!(
        "⏱ Startup init: {}.{:09}s ({} ns)",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        elapsed.as_nanos()
    );

    // Create and run server
    let server = Server::new(
        &args.ws_host,
        args.ws_port,
        symbols,
        &args.sierra_data_folder,
        args.sierra_tcp_port,
    );

    server.run().await
}
