//! PhaseShifter Real-Time Streaming Server
//!
//! Connects to Sierra Chart via DTC protocol and streams real-time market data
//! to frontend clients via WebSocket.
//!
//! Also accepts direct TCP connections from ACSIL studies for live CME data
//! (bypasses DTC protocol restrictions).
//!
//! Usage:
//!     phaseshifter-server --dtc-host 127.0.0.1 --dtc-port 11099 --ws-port 8000

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod bar_builder;
mod clusters;
mod contracts;
mod dtc;
mod python_pipeline;
mod scid;
mod server;
mod sierra_tcp;
mod yfinance;

use server::Server;

#[derive(Debug, Parser)]
#[command(
    name = "phaseshifter-server",
    version,
    about = "Real-time streaming server for PhaseShifter"
)]
struct Args {
    /// DTC server host (Sierra Chart)
    #[arg(long, default_value = "127.0.0.1")]
    dtc_host: String,

    /// DTC server port
    #[arg(long, default_value = "11099")]
    dtc_port: u16,

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
    #[arg(long, default_value = "D:\\Trading\\Sierra\\Data")]
    sierra_data_folder: String,

    /// Sierra TCP port for ACSIL study connections (live tick data)
    #[arg(long, default_value = "9000")]
    sierra_tcp_port: u16,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// yfinance poll interval in seconds (for non-Sierra symbols)
    #[arg(long, default_value = "60")]
    yfinance_interval: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
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
        "║  DTC Server:   {}:{}                          ║",
        args.dtc_host, args.dtc_port
    );
    info!(
        "║  Sierra TCP:   127.0.0.1:{}  (ACSIL live data)            ║",
        args.sierra_tcp_port
    );
    info!(
        "║  WebSocket:    ws://{}:{}                          ║",
        args.ws_host, args.ws_port
    );
    info!("║  Symbols:      {:41}  ║", args.symbols);
    info!(
        "║  yfinance:     {}s poll interval                            ║",
        args.yfinance_interval
    );
    info!("╚════════════════════════════════════════════════════════════╝");

    // Parse symbols
    let symbols: Vec<String> = args
        .symbols
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();

    // Create and run server
    let server = Server::new(
        &args.dtc_host,
        args.dtc_port,
        &args.ws_host,
        args.ws_port,
        symbols,
        &args.sierra_data_folder,
        args.sierra_tcp_port,
        args.yfinance_interval,
    );

    server.run().await
}
