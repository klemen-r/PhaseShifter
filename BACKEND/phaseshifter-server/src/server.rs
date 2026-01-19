//! WebSocket Server
//!
//! High-performance WebSocket server that broadcasts real-time market data
//! to connected frontend clients. Integrates DTC client, bar builder, and PhaseEngine.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, trace, warn};

use phaseshifter_core::{Candle, EngineConfig, Phase, PhaseEngine, PhaseSide};

use crate::bar_builder::{Bar, BarBuilder, BarEvent, Timeframe};
use crate::clusters::{ClusterConfig, ClusterManager, ClustersData, ProjectionSide, TrackedNode};
use crate::contracts::{to_sierra_symbol, to_sierra_symbol_with_exchange};
use crate::dtc::{DtcClient, DtcClientConfig, Tick};
use crate::scid::ScidReader;
use crate::sierra_tcp::{SierraTcpConfig, SierraTcpServer, SymbolRegistry};

/// Client ID type
type ClientId = u64;

/// WebSocket message types sent to clients
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsOutMessage {
    /// Connection confirmed
    Connected {
        client_id: ClientId,
        symbols: Vec<String>,
    },
    /// Real-time tick data
    Tick {
        symbol: String,
        price: f64,
        volume: f64,
        timestamp: i64,
        bid: f64,
        ask: f64,
    },
    /// Bar update (in-progress bar changed)
    BarUpdate {
        symbol: String,
        timeframe: String,
        #[serde(rename = "time")]
        timestamp_ms: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    },
    /// Bar closed (new bar started)
    BarClosed {
        symbol: String,
        timeframe: String,
        #[serde(rename = "time")]
        timestamp_ms: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    },
    /// Phase engine update
    PhaseUpdate {
        symbol: String,
        timeframe: String,
        #[serde(rename = "time")]
        timestamp_ms: i64,
        phase: String,
        anchor: f64,
        dm: f64,
    },
    /// Node created
    NodeCreated {
        symbol: String,
        timeframe: String,
        direction: String,
        distance_pct: f64,
        anchor: f64,
        extreme: f64,
        created_at: i64,
    },
    /// Node closed (target hit)
    NodeClosed {
        symbol: String,
        timeframe: String,
        direction: String,
        distance_pct: f64,
        target: f64,
        closed_at: i64,
    },
    /// Current open nodes snapshot
    OpenNodes {
        symbol: String,
        timeframe: String,
        nodes: Vec<OpenNodeInfo>,
    },
    /// Historical bars for initialization
    History {
        symbol: String,
        timeframe: String,
        bars: Vec<HistoryBar>,
    },
    /// Cluster data response
    Clusters { ticker: String, data: ClustersData },
    /// Error message
    Error { message: String },
    /// Pong response
    Pong { timestamp: i64 },
}

/// Open node info for snapshots
#[derive(Debug, Clone, Serialize)]
pub struct OpenNodeInfo {
    pub direction: String,
    pub distance_pct: f64,
    pub anchor: f64,
    pub extreme: f64,
    pub created_at: i64,
    pub projected_target: f64,
}

/// Historical bar for initialization
#[derive(Debug, Clone, Serialize)]
pub struct HistoryBar {
    #[serde(rename = "time")]
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Incoming WebSocket messages from clients
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsInMessage {
    /// Subscribe to a symbol
    Subscribe {
        symbol: String,
        timeframes: Option<Vec<String>>,
    },
    /// Unsubscribe from a symbol
    Unsubscribe { symbol: String },
    /// Request historical bars
    GetHistory {
        symbol: String,
        timeframe: String,
        limit: Option<usize>,
    },
    /// Request open nodes
    GetNodes { symbol: String, timeframe: String },
    /// Request cluster data
    GetClusters { ticker: String },
    /// Ping for keepalive
    Ping { timestamp: i64 },
}

/// Connected client state
struct ClientState {
    sender: mpsc::Sender<WsOutMessage>,
    subscribed_symbols: Vec<String>,
    subscribed_timeframes: Vec<Timeframe>,
}

/// Phase engine state per symbol+timeframe
struct EngineState {
    engine: PhaseEngine,
    timeframe: Timeframe,
}

/// Main server struct
pub struct Server {
    dtc_host: String,
    dtc_port: u16,
    ws_host: String,
    ws_port: u16,
    symbols: Vec<String>,
    sierra_data_folder: String,
    sierra_tcp_port: u16,
}

impl Server {
    pub fn new(
        dtc_host: &str,
        dtc_port: u16,
        ws_host: &str,
        ws_port: u16,
        symbols: Vec<String>,
        sierra_data_folder: &str,
        sierra_tcp_port: u16,
    ) -> Self {
        Self {
            dtc_host: dtc_host.to_string(),
            dtc_port,
            ws_host: ws_host.to_string(),
            ws_port,
            symbols,
            sierra_data_folder: sierra_data_folder.to_string(),
            sierra_tcp_port,
        }
    }

    pub async fn run(self) -> Result<()> {
        // Shared state
        let clients: Arc<DashMap<ClientId, ClientState>> = Arc::new(DashMap::new());
        let next_client_id = Arc::new(std::sync::atomic::AtomicU64::new(1));

        // Live mode flag - false during historical loading, true after
        // This prevents flooding clients with millions of historical bar events
        let live_mode = Arc::new(AtomicBool::new(false));

        // Broadcast channel for all events
        let (broadcast_tx, _) = broadcast::channel::<WsOutMessage>(10000);

        // Bar event channel
        let (bar_event_tx, bar_event_rx) = mpsc::channel::<BarEvent>(10000);

        // Bar verification channel (for SCID validation)
        let (verify_tx, verify_rx) = mpsc::channel::<crate::bar_builder::BarVerifyRequest>(100);

        // Create bar builder with default timeframes
        let timeframes = vec![Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::H1];
        let mut builder = BarBuilder::new(timeframes.clone(), bar_event_tx);
        builder.set_verify_sender(verify_tx);
        let bar_builder = Arc::new(RwLock::new(builder));

        // Phase engines per symbol+timeframe+phase_window (multiple scenarios)
        // Key: (symbol, timeframe, phase_window)
        let engines: Arc<DashMap<(String, Timeframe, usize), RwLock<EngineState>>> =
            Arc::new(DashMap::new());

        // Define scenarios: (timeframe, phase_window) combinations for clustering
        let scenarios: Vec<(Timeframe, usize)> = vec![
            (Timeframe::M1, 50),  // 1-minute with 50-period window
            (Timeframe::M5, 7),   // 5-minute with 7-period window
            (Timeframe::M5, 20),  // 5-minute with 20-period window
            (Timeframe::M15, 10), // 15-minute with 10-period window
            (Timeframe::H1, 7),   // 1-hour with 7-period window
        ];

        // Initialize engines for each symbol and scenario
        for base_symbol in &self.symbols {
            for &(tf, phase_window) in &scenarios {
                let config = EngineConfig {
                    side: PhaseSide::Long,
                    timeframe: tf.as_str().to_string(),
                    warmup_candles: 0,
                    phase_window,
                    depth_days: 50,
                };
                let engine = PhaseEngine::new(base_symbol.clone(), config);
                engines.insert(
                    (base_symbol.clone(), tf, phase_window),
                    RwLock::new(EngineState {
                        engine,
                        timeframe: tf,
                    }),
                );
            }
        }

        // Cluster manager for computing clusters from multiple scenarios
        let cluster_manager: Arc<RwLock<ClusterManager>> =
            Arc::new(RwLock::new(ClusterManager::new(ClusterConfig::default())));

        // Start DTC client
        let dtc_config = DtcClientConfig {
            host: self.dtc_host.clone(),
            port: self.dtc_port,
            client_name: "PhaseShifter".to_string(),
            ..Default::default()
        };
        let mut dtc_client = DtcClient::new(dtc_config);

        // Convert symbols to Sierra format (without exchange suffix for internal use)
        let sierra_symbols: Vec<String> =
            self.symbols.iter().map(|s| to_sierra_symbol(s)).collect();

        // Convert symbols to full Sierra format with exchange (for DTC subscription)
        // Note: CME data via DTC is blocked by Sierra Chart policy, but we format correctly anyway
        let dtc_symbols: Vec<String> = self
            .symbols
            .iter()
            .map(|s| to_sierra_symbol_with_exchange(s, "CME"))
            .collect();

        // Take tick receiver before connecting
        let tick_rx = dtc_client
            .take_tick_receiver()
            .expect("tick receiver should be available");

        // Take historical bar receiver before connecting
        let hist_rx = dtc_client
            .take_historical_bar_receiver()
            .expect("historical bar receiver should be available");

        // Register symbol subscriptions BEFORE connecting (they will be sent on logon)
        // Using full symbol format with exchange suffix (e.g., NQH26-CME)
        for symbol in &dtc_symbols {
            dtc_client.subscribe(symbol).await?;
            info!("Registered DTC symbol subscription: {}", symbol);
        }

        // IMPORTANT: Spawn bar event processing task FIRST
        // This must be running before we load historical data so that
        // bar events from SCID loading are processed correctly
        let engines_bar = engines.clone();
        let cluster_manager_bar = cluster_manager.clone();
        let scenarios_bar = scenarios.clone();
        let broadcast_tx_bar = broadcast_tx.clone();
        let live_mode_bar = live_mode.clone();
        tokio::spawn(async move {
            process_bar_events(
                bar_event_rx,
                engines_bar,
                cluster_manager_bar,
                scenarios_bar,
                broadcast_tx_bar,
                live_mode_bar,
            )
            .await;
        });

        // Small delay to ensure bar event processor is ready
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Load historical data from SCID files (bypasses CME DTC restrictions)
        info!(
            "Loading historical data from SCID files in: {}",
            self.sierra_data_folder
        );
        let sierra_data_folder = self.sierra_data_folder.clone();
        let sierra_symbols_for_hist = sierra_symbols.clone();
        let base_symbols_for_hist = self.symbols.clone();
        let bar_builder_for_hist = bar_builder.clone();
        let live_mode_hist = live_mode.clone();
        tokio::spawn(async move {
            load_historical_data_scid(
                &sierra_data_folder,
                sierra_symbols_for_hist,
                base_symbols_for_hist,
                bar_builder_for_hist,
                live_mode_hist,
            )
            .await;
        });

        // Connect to DTC server (subscriptions and historical requests will be sent on logon)
        info!("Connecting to DTC server...");
        dtc_client
            .connect()
            .await
            .context("Failed to connect to DTC server")?;
        info!("DTC connected successfully");

        // Set up Sierra TCP server for ACSIL live tick data
        // This bypasses DTC restrictions for CME data
        let symbol_registry = Arc::new(RwLock::new(SymbolRegistry::new()));

        // Register all symbols in the registry so we can map symbol_id -> symbol name
        // Also build a symbol map for Sierra -> base symbol conversion
        let mut sierra_to_base_map: HashMap<String, String> = HashMap::new();
        {
            let mut reg = symbol_registry.write().await;
            let mut builder = bar_builder.write().await;
            for (sierra_sym, base_sym) in sierra_symbols.iter().zip(self.symbols.iter()) {
                // Register both the Sierra format (NQH26-CME) and base format (NQ)
                let sierra_with_exchange = to_sierra_symbol_with_exchange(sierra_sym, "CME");
                let id = reg.register(&sierra_with_exchange);
                info!(
                    "Registered symbol {} -> {} (id: {})",
                    sierra_with_exchange, base_sym, id
                );
                // Add to symbol map for tick processing
                sierra_to_base_map.insert(sierra_with_exchange.clone(), base_sym.clone());
                // Register mapping in bar builder for SCID verification
                builder.register_symbol_mapping(base_sym, &sierra_with_exchange);
            }
        }

        // Create channel for Sierra TCP ticks
        let (sierra_tick_tx, sierra_tick_rx) = mpsc::channel::<crate::sierra_tcp::Tick>(10000);

        // Start Sierra TCP server
        let sierra_tcp_config = SierraTcpConfig {
            host: "127.0.0.1".to_string(),
            port: self.sierra_tcp_port,
        };
        let sierra_server =
            SierraTcpServer::new(sierra_tcp_config, symbol_registry.clone(), sierra_tick_tx);

        tokio::spawn(async move {
            if let Err(e) = sierra_server.run().await {
                error!("Sierra TCP server error: {}", e);
            }
        });

        // Spawn Sierra tick processing task
        let bar_builder_sierra = bar_builder.clone();
        let broadcast_tx_sierra = broadcast_tx.clone();
        tokio::spawn(async move {
            process_sierra_ticks(
                sierra_tick_rx,
                bar_builder_sierra,
                broadcast_tx_sierra,
                sierra_to_base_map,
            )
            .await;
        });

        // Spawn bar verification task (validates closed bars against SCID)
        let sierra_data_folder_verify = self.sierra_data_folder.clone();
        let broadcast_tx_verify = broadcast_tx.clone();
        tokio::spawn(async move {
            process_bar_verifications(verify_rx, sierra_data_folder_verify, broadcast_tx_verify)
                .await;
        });

        // Start WebSocket listener
        let addr: SocketAddr = format!("{}:{}", self.ws_host, self.ws_port).parse()?;
        let listener = TcpListener::bind(&addr).await?;
        info!("WebSocket server listening on ws://{}", addr);

        // Spawn historical bar processing task (from DTC, if any)
        let bar_builder_hist = bar_builder.clone();
        let sierra_symbols_hist = sierra_symbols.clone();
        let symbols_for_hist = self.symbols.clone();
        tokio::spawn(async move {
            process_historical_bars(
                hist_rx,
                bar_builder_hist,
                sierra_symbols_hist,
                symbols_for_hist,
            )
            .await;
        });

        // Spawn tick processing task
        let bar_builder_tick = bar_builder.clone();
        let broadcast_tx_tick = broadcast_tx.clone();
        let symbols_for_tick = self.symbols.clone();
        tokio::spawn(async move {
            process_ticks(
                tick_rx,
                bar_builder_tick,
                broadcast_tx_tick,
                sierra_symbols,
                symbols_for_tick,
            )
            .await;
        });

        // Accept WebSocket connections
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let client_id =
                        next_client_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let clients = clients.clone();
                    let broadcast_rx = broadcast_tx.subscribe();
                    let bar_builder = bar_builder.clone();
                    let engines = engines.clone();
                    let cluster_manager = cluster_manager.clone();
                    let symbols = self.symbols.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(
                            stream,
                            peer_addr,
                            client_id,
                            clients,
                            broadcast_rx,
                            bar_builder,
                            engines,
                            cluster_manager,
                            symbols,
                        )
                        .await
                        {
                            debug!("Client {} error: {}", client_id, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

/// Process incoming ticks from DTC
async fn process_ticks(
    mut tick_rx: mpsc::Receiver<Tick>,
    bar_builder: Arc<RwLock<BarBuilder>>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    sierra_symbols: Vec<String>,
    base_symbols: Vec<String>,
) {
    // Map Sierra symbols back to base symbols
    let symbol_map: HashMap<String, String> = sierra_symbols
        .into_iter()
        .zip(base_symbols.into_iter())
        .collect();

    while let Some(tick) = tick_rx.recv().await {
        // Map Sierra symbol to base symbol
        let base_symbol = symbol_map
            .get(&tick.symbol)
            .cloned()
            .unwrap_or_else(|| tick.symbol.clone());

        // Broadcast tick to WebSocket clients
        let tick_msg = WsOutMessage::Tick {
            symbol: base_symbol.clone(),
            price: tick.price,
            volume: tick.volume,
            timestamp: (tick.timestamp * 1000.0) as i64,
            bid: tick.bid_price,
            ask: tick.ask_price,
        };
        let _ = broadcast_tx.send(tick_msg);

        // Update bar builder with remapped symbol
        let mut remapped_tick = tick.clone();
        remapped_tick.symbol = base_symbol;

        let mut builder = bar_builder.write().await;
        builder.on_tick(&remapped_tick).await;
    }

    warn!("Tick receiver closed");
}

/// Process incoming ticks from Sierra TCP (ACSIL study)
async fn process_sierra_ticks(
    mut tick_rx: mpsc::Receiver<crate::sierra_tcp::Tick>,
    bar_builder: Arc<RwLock<BarBuilder>>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    symbol_map: HashMap<String, String>, // Sierra symbol -> base symbol
) {
    let mut tick_count: u64 = 0;
    let mut last_log_count: u64 = 0;
    let mut waiting_for_live_mode = true;

    while let Some(tick) = tick_rx.recv().await {
        // Wait for live mode before processing ticks
        // This prevents live ticks from interfering with historical data loading
        if waiting_for_live_mode {
            let builder = bar_builder.read().await;
            if !builder.is_live_mode() {
                // Skip this tick - historical loading still in progress
                continue;
            }
            waiting_for_live_mode = false;
            info!("Sierra TCP processing started - live mode active");
        }

        tick_count += 1;

        // Map Sierra symbol to base symbol (e.g., NQH26-CME -> NQ)
        let base_symbol = symbol_map
            .get(&tick.symbol)
            .cloned()
            .unwrap_or_else(|| tick.symbol.clone());

        // Debug: log first 10 ticks with symbol mapping
        if tick_count <= 10 {
            info!(
                "process_sierra_ticks #{}: raw={} -> base={} @ {:.2}",
                tick_count, tick.symbol, base_symbol, tick.price
            );
        }

        // Broadcast tick to WebSocket clients with base symbol
        let tick_msg = WsOutMessage::Tick {
            symbol: base_symbol.clone(),
            price: tick.price,
            volume: tick.volume,
            timestamp: tick.timestamp_ms,
            bid: 0.0, // Sierra TCP doesn't provide bid/ask
            ask: 0.0,
        };
        let _ = broadcast_tx.send(tick_msg);

        // Convert to DTC Tick format for bar builder (use base symbol)
        let dtc_tick = Tick {
            symbol: base_symbol.clone(),
            symbol_id: 0,
            price: tick.price,
            volume: tick.volume,
            timestamp: tick.timestamp_ms as f64 / 1000.0, // Convert ms to seconds
            at_bid_or_ask: 0,
            bid_price: 0.0,
            ask_price: 0.0,
        };

        // Update bar builder
        let mut builder = bar_builder.write().await;
        builder.on_tick(&dtc_tick).await;

        // Log first few ticks with timestamp info
        if tick_count <= 3 {
            debug!(
                "Sierra tick #{}: {} @ {:.2}, ts_ms={}, ts_sec={:.3}",
                tick_count, base_symbol, tick.price, tick.timestamp_ms, dtc_tick.timestamp
            );
        }

        // Periodic logging
        if tick_count - last_log_count >= 10000 {
            last_log_count = tick_count;
            info!(
                "Sierra TCP processed {} live ticks, last: {} @ {:.2}",
                tick_count, base_symbol, dtc_tick.price
            );
        }
    }

    warn!("Sierra tick receiver closed");
}

/// Process bar verification requests against SCID data
/// When a mismatch is found, broadcasts a corrected bar_closed message with SCID values
async fn process_bar_verifications(
    mut verify_rx: mpsc::Receiver<crate::bar_builder::BarVerifyRequest>,
    sierra_data_folder: String,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
) {
    use crate::scid::build_m1_bar_from_scid;
    use std::path::Path;

    info!("Bar verification task started");

    while let Some(req) = verify_rx.recv().await {
        // Build SCID file path
        let scid_filename = format!("{}.scid", req.sierra_symbol);
        let scid_path = Path::new(&sierra_data_folder).join(&scid_filename);

        if !scid_path.exists() {
            debug!("SCID file not found for verification: {:?}", scid_path);
            continue;
        }

        // Build bar from SCID for the same timestamp
        match build_m1_bar_from_scid(&scid_path, req.bar.timestamp_ms) {
            Ok(Some(scid_bar)) => {
                // Compare bars
                let open_diff = (req.bar.open - scid_bar.open).abs();
                let high_diff = (req.bar.high - scid_bar.high).abs();
                let low_diff = (req.bar.low - scid_bar.low).abs();
                let close_diff = (req.bar.close - scid_bar.close).abs();

                // Allow small tolerance (0.25 for NQ tick size)
                let tolerance = 0.25;
                let matched = open_diff <= tolerance
                    && high_diff <= tolerance
                    && low_diff <= tolerance
                    && close_diff <= tolerance;

                if !matched {
                    info!(
                        "Bar correction for {} at {}: live O:{:.2} H:{:.2} L:{:.2} C:{:.2} -> SCID O:{:.2} H:{:.2} L:{:.2} C:{:.2}",
                        req.bar.symbol, req.bar.timestamp_ms,
                        req.bar.open, req.bar.high, req.bar.low, req.bar.close,
                        scid_bar.open, scid_bar.high, scid_bar.low, scid_bar.close
                    );

                    // Broadcast corrected bar with SCID values
                    let corrected_msg = WsOutMessage::BarClosed {
                        symbol: req.bar.symbol.clone(),
                        timeframe: req.bar.timeframe.clone(),
                        timestamp_ms: req.bar.timestamp_ms,
                        open: scid_bar.open,
                        high: scid_bar.high,
                        low: scid_bar.low,
                        close: scid_bar.close,
                        volume: scid_bar.volume,
                    };
                    let _ = broadcast_tx.send(corrected_msg);
                } else {
                    trace!(
                        "Bar verified for {} at {}: O:{:.2} H:{:.2} L:{:.2} C:{:.2} ({} ticks)",
                        req.bar.symbol,
                        req.bar.timestamp_ms,
                        req.bar.open,
                        req.bar.high,
                        req.bar.low,
                        req.bar.close,
                        scid_bar.tick_count
                    );
                }
            }
            Ok(None) => {
                debug!(
                    "No SCID ticks found for {} at {} - bar may be from live data only",
                    req.bar.symbol, req.bar.timestamp_ms
                );
            }
            Err(e) => {
                warn!("Failed to verify bar from SCID: {}", e);
            }
        }
    }

    warn!("Bar verification receiver closed");
}

/// Load historical data from SCID files on disk
async fn load_historical_data_scid(
    sierra_data_folder: &str,
    sierra_symbols: Vec<String>,
    base_symbols: Vec<String>,
    bar_builder: Arc<RwLock<BarBuilder>>,
    live_mode: Arc<AtomicBool>,
) {
    use std::path::Path;

    // Map Sierra symbols to base symbols
    let symbol_map: HashMap<String, String> = sierra_symbols
        .iter()
        .cloned()
        .zip(base_symbols.iter().cloned())
        .collect();

    // Load data for each symbol
    for sierra_symbol in &sierra_symbols {
        let base_symbol = symbol_map
            .get(sierra_symbol)
            .cloned()
            .unwrap_or_else(|| sierra_symbol.clone());

        // SCID files use the format: NQH26-CME.scid
        let scid_filename = format!("{}-CME.scid", sierra_symbol);
        let scid_path = Path::new(sierra_data_folder).join(&scid_filename);

        info!("Loading historical data from: {:?}", scid_path);

        if !scid_path.exists() {
            warn!("SCID file not found: {:?}", scid_path);
            continue;
        }

        // Open and read SCID file
        match ScidReader::open(&scid_path) {
            Ok(mut reader) => {
                let record_count = reader.record_count();
                info!(
                    "SCID file has {} records for {}",
                    record_count, sierra_symbol
                );

                // Calculate how many records we want
                // For tick data: ~50k-100k ticks per day for NQ
                // For bar data: ~1440 one-minute bars per day
                // Load enough for ~50 days of data to match Python pipeline depth_days
                // 50 days * 100k ticks/day = 5 million records
                let max_records = 5_000_000u64;
                let records = match reader.read_last_n_records(max_records) {
                    Ok(recs) => recs,
                    Err(e) => {
                        error!("Failed to read SCID records for {}: {}", sierra_symbol, e);
                        continue;
                    }
                };

                info!(
                    "Loaded {} historical records for {}",
                    records.len(),
                    base_symbol
                );

                // Convert SCID records to ticks and feed to bar builder
                // SCID files can contain either tick data or bar data
                let mut builder = bar_builder.write().await;
                let mut tick_count = 0u32;
                let mut bar_count = 0u32;
                let mut skipped = 0u32;
                let mut skipped_bid_ask_only = 0u32;
                let mut skipped_invalid_tick = 0u32;
                let mut skipped_invalid_bar = 0u32;

                for record in &records {
                    let unix_ts = record.to_unix_timestamp();

                    if record.is_valid_tick() {
                        // This is tick data - use close as trade price
                        let tick = Tick {
                            symbol: base_symbol.clone(),
                            symbol_id: 0,
                            price: record.tick_price() as f64,
                            volume: record.total_volume as f64,
                            timestamp: unix_ts,
                            at_bid_or_ask: 0,
                            bid_price: record.tick_bid() as f64,
                            ask_price: record.tick_ask() as f64,
                        };
                        builder.on_tick(&tick).await;
                        tick_count += 1;
                    } else if record.is_valid_bar() {
                        // This is bar data - create synthetic OHLC ticks
                        let ticks = vec![
                            Tick {
                                symbol: base_symbol.clone(),
                                symbol_id: 0,
                                price: record.open as f64,
                                volume: record.total_volume as f64 / 4.0,
                                timestamp: unix_ts,
                                at_bid_or_ask: 0,
                                bid_price: record.open as f64,
                                ask_price: record.open as f64,
                            },
                            Tick {
                                symbol: base_symbol.clone(),
                                symbol_id: 0,
                                price: record.high as f64,
                                volume: record.total_volume as f64 / 4.0,
                                timestamp: unix_ts + 15.0,
                                at_bid_or_ask: 0,
                                bid_price: record.high as f64,
                                ask_price: record.high as f64,
                            },
                            Tick {
                                symbol: base_symbol.clone(),
                                symbol_id: 0,
                                price: record.low as f64,
                                volume: record.total_volume as f64 / 4.0,
                                timestamp: unix_ts + 30.0,
                                at_bid_or_ask: 0,
                                bid_price: record.low as f64,
                                ask_price: record.low as f64,
                            },
                            Tick {
                                symbol: base_symbol.clone(),
                                symbol_id: 0,
                                price: record.close as f64,
                                volume: record.total_volume as f64 / 4.0,
                                timestamp: unix_ts + 45.0,
                                at_bid_or_ask: 0,
                                bid_price: record.close as f64,
                                ask_price: record.close as f64,
                            },
                        ];

                        for tick in ticks {
                            builder.on_tick(&tick).await;
                        }
                        bar_count += 1;
                    } else {
                        skipped += 1;
                        // Track why it was skipped
                        if record.is_bid_ask_only() {
                            // Bid/ask quote without trade - expected, no need to log
                            skipped_bid_ask_only += 1;
                        } else if record.is_single_trade() {
                            // It's a tick but failed validation - bad price?
                            skipped_invalid_tick += 1;
                            if skipped_invalid_tick <= 3 {
                                debug!(
                                    "Invalid tick: O={} H={} L={} C={} vol={} ts={}",
                                    record.open,
                                    record.high,
                                    record.low,
                                    record.close,
                                    record.total_volume,
                                    unix_ts
                                );
                            }
                        } else {
                            // It's a bar but failed validation
                            skipped_invalid_bar += 1;
                            if skipped_invalid_bar <= 3 {
                                debug!(
                                    "Invalid bar: O={} H={} L={} C={} vol={} ts={}",
                                    record.open,
                                    record.high,
                                    record.low,
                                    record.close,
                                    record.total_volume,
                                    unix_ts
                                );
                            }
                        }
                    }
                }
                drop(builder);

                info!(
                    "Loaded historical data for {}: {} ticks, {} bars ({} skipped: {} bid/ask only, {} invalid ticks, {} invalid bars)",
                    base_symbol, tick_count, bar_count, skipped, skipped_bid_ask_only, skipped_invalid_tick, skipped_invalid_bar
                );

                // Log bar count after loading and check for anomalies
                {
                    let builder = bar_builder.read().await;
                    let m1_bars = builder.get_bars(&base_symbol, Timeframe::M1);
                    let m5_bars = builder.get_bars(&base_symbol, Timeframe::M5);
                    info!(
                        "Built bars for {}: M1={}, M5={}",
                        base_symbol,
                        m1_bars.len(),
                        m5_bars.len()
                    );

                    // Check for abnormally large bars (potential bug indicator)
                    for (i, bar) in m1_bars.iter().enumerate() {
                        let range = bar.high - bar.low;
                        // ES typically has 1-5 point range per minute, NQ has 10-50
                        // Flag anything over 100 points as suspicious
                        if range > 100.0 || bar.tick_count > 10000 {
                            warn!(
                                "Large M1 bar detected for {} at index {}: time={} range={:.2} ticks={} O={:.2} H={:.2} L={:.2} C={:.2}",
                                base_symbol, i, bar.timestamp_ms, range, bar.tick_count,
                                bar.open, bar.high, bar.low, bar.close
                            );
                        }
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to open SCID file {:?} for {}: {}",
                    scid_path, sierra_symbol, e
                );
            }
        }
    }

    info!("Historical data loading complete");

    // Enable live mode now that historical loading is done
    // This enables SCID verification for closed bars AND broadcasting to clients
    {
        let mut builder = bar_builder.write().await;
        builder.set_live_mode(true);
    }

    // Set the shared live_mode flag so process_bar_events starts broadcasting
    live_mode.store(true, Ordering::Relaxed);
    info!("Live mode enabled - bar events will now be broadcast to clients");
}

/// Process historical bars from DTC and inject as synthetic ticks
async fn process_historical_bars(
    mut hist_rx: mpsc::Receiver<crate::dtc::HistoricalBar>,
    bar_builder: Arc<RwLock<BarBuilder>>,
    sierra_symbols: Vec<String>,
    base_symbols: Vec<String>,
) {
    // Map Sierra symbols back to base symbols
    let symbol_map: HashMap<String, String> = sierra_symbols
        .into_iter()
        .zip(base_symbols.into_iter())
        .collect();

    let mut bar_count = 0;
    let mut completed_requests = std::collections::HashSet::new();

    while let Some(hist_bar) = hist_rx.recv().await {
        // Map Sierra symbol to base symbol
        let base_symbol = symbol_map
            .get(&hist_bar.symbol)
            .cloned()
            .unwrap_or_else(|| hist_bar.symbol.clone());

        // Convert historical bar to synthetic ticks
        // We'll create 4 ticks per bar: open, high, low, close
        let timestamp = hist_bar.start_date_time;

        let ticks = vec![
            Tick {
                symbol: base_symbol.clone(),
                symbol_id: 0,
                price: hist_bar.open,
                volume: hist_bar.volume / 4.0,
                timestamp,
                at_bid_or_ask: 0,
                bid_price: hist_bar.open,
                ask_price: hist_bar.open,
            },
            Tick {
                symbol: base_symbol.clone(),
                symbol_id: 0,
                price: hist_bar.high,
                volume: hist_bar.volume / 4.0,
                timestamp: timestamp + 15.0,
                at_bid_or_ask: 0,
                bid_price: hist_bar.high,
                ask_price: hist_bar.high,
            },
            Tick {
                symbol: base_symbol.clone(),
                symbol_id: 0,
                price: hist_bar.low,
                volume: hist_bar.volume / 4.0,
                timestamp: timestamp + 30.0,
                at_bid_or_ask: 0,
                bid_price: hist_bar.low,
                ask_price: hist_bar.low,
            },
            Tick {
                symbol: base_symbol.clone(),
                symbol_id: 0,
                price: hist_bar.close,
                volume: hist_bar.volume / 4.0,
                timestamp: timestamp + 45.0,
                at_bid_or_ask: 0,
                bid_price: hist_bar.close,
                ask_price: hist_bar.close,
            },
        ];

        // Feed ticks to bar builder
        let mut builder = bar_builder.write().await;
        for tick in ticks {
            builder.on_tick(&tick).await;
        }
        drop(builder);

        bar_count += 1;

        if hist_bar.is_final && !completed_requests.contains(&hist_bar.request_id) {
            completed_requests.insert(hist_bar.request_id);
            info!(
                "Completed loading {} historical bars for symbol={}, request_id={}",
                bar_count, base_symbol, hist_bar.request_id
            );
        }
    }

    warn!("Historical bar receiver closed");
}

/// Process bar events from the bar builder
async fn process_bar_events(
    mut bar_event_rx: mpsc::Receiver<BarEvent>,
    engines: Arc<DashMap<(String, Timeframe, usize), RwLock<EngineState>>>,
    cluster_manager: Arc<RwLock<ClusterManager>>,
    scenarios: Vec<(Timeframe, usize)>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    live_mode: Arc<AtomicBool>,
) {
    while let Some(event) = bar_event_rx.recv().await {
        // Only broadcast to clients when in live mode (after historical loading)
        let is_live = live_mode.load(Ordering::Relaxed);

        match event {
            BarEvent::Updated(bar) => {
                // Only broadcast bar updates in live mode
                if is_live {
                    // Log 1m bar updates at trace level (very frequent - every tick)
                    if bar.timeframe == "1m" {
                        trace!(
                            "[BAR_UPDATE] {} {} time={} close={:.2}",
                            bar.symbol,
                            bar.timeframe,
                            bar.timestamp_ms,
                            bar.close
                        );
                    }
                    // Broadcast bar update
                    let msg = WsOutMessage::BarUpdate {
                        symbol: bar.symbol.clone(),
                        timeframe: bar.timeframe.clone(),
                        timestamp_ms: bar.timestamp_ms,
                        open: bar.open,
                        high: bar.high,
                        low: bar.low,
                        close: bar.close,
                        volume: bar.volume,
                    };
                    let _ = broadcast_tx.send(msg);
                }
            }
            BarEvent::Closed(bar) => {
                // Only broadcast and log bar closes in live mode
                if is_live {
                    info!(
                        "[BAR_CLOSED] {} tf={} time={}",
                        bar.symbol, bar.timeframe, bar.timestamp_ms
                    );
                    // Broadcast bar closed
                    let msg = WsOutMessage::BarClosed {
                        symbol: bar.symbol.clone(),
                        timeframe: bar.timeframe.clone(),
                        timestamp_ms: bar.timestamp_ms,
                        open: bar.open,
                        high: bar.high,
                        low: bar.low,
                        close: bar.close,
                        volume: bar.volume,
                    };
                    let _ = broadcast_tx.send(msg);
                }

                // Feed closed bar to all matching PhaseEngines (different phase_windows)
                if let Some(tf) = Timeframe::from_str(&bar.timeframe) {
                    let candle = Candle {
                        timestamp_ms: bar.timestamp_ms,
                        open: bar.open,
                        high: bar.high,
                        low: bar.low,
                        close: bar.close,
                        volume: bar.volume,
                    };

                    // Process all scenarios for this timeframe
                    for &(scenario_tf, phase_window) in &scenarios {
                        if scenario_tf != tf {
                            continue;
                        }

                        let key = (bar.symbol.clone(), tf, phase_window);
                        if let Some(engine_state) = engines.get(&key) {
                            let mut state = engine_state.write().await;
                            if let Some(_dm) = state.engine.on_candle(&candle) {
                                // Check for phase updates (only broadcast for primary scenario)
                                if phase_window == 7 || (tf == Timeframe::M1 && phase_window == 50)
                                {
                                    if let Some(phase) = state.engine.current_phase() {
                                        if let Some(anchor) = state.engine.current_anchor() {
                                            let phase_msg = WsOutMessage::PhaseUpdate {
                                                symbol: bar.symbol.clone(),
                                                timeframe: bar.timeframe.clone(),
                                                timestamp_ms: bar.timestamp_ms,
                                                phase: format!("{:?}", phase).to_lowercase(),
                                                anchor,
                                                dm: _dm,
                                            };
                                            let _ = broadcast_tx.send(phase_msg);
                                        }
                                    }
                                }

                                // Check for phase flip and node creation
                                if let Some(flip) = state.engine.take_last_phase_flip() {
                                    // Node direction matches the phase we're coming FROM
                                    let direction = match flip.from {
                                        Phase::Bullish => "bullish",
                                        Phase::Bearish => "bearish",
                                    };

                                    // Calculate extreme and projected value
                                    let (extreme, projected_value, side) = if flip.from
                                        == Phase::Bullish
                                    {
                                        let extreme = flip.old_anchor * (1.0 + flip.node_move_pct);
                                        let projected =
                                            flip.new_anchor * (1.0 + flip.node_move_pct);
                                        (extreme, projected, ProjectionSide::Bullish)
                                    } else {
                                        let extreme = flip.old_anchor * (1.0 - flip.node_move_pct);
                                        let projected =
                                            flip.new_anchor * (1.0 - flip.node_move_pct);
                                        (extreme, projected, ProjectionSide::Bearish)
                                    };

                                    // Add node to cluster manager
                                    {
                                        let mut cm = cluster_manager.write().await;
                                        cm.add_node(
                                            &bar.symbol,
                                            TrackedNode::new(
                                                side,
                                                tf.as_str().to_string(),
                                                phase_window,
                                                flip.node_move_pct,
                                                flip.old_anchor,
                                                bar.timestamp_ms,
                                            ),
                                        );
                                    }

                                    // Broadcast node created
                                    let node_msg = WsOutMessage::NodeCreated {
                                        symbol: bar.symbol.clone(),
                                        timeframe: bar.timeframe.clone(),
                                        direction: direction.to_string(),
                                        distance_pct: flip.node_move_pct,
                                        anchor: flip.old_anchor,
                                        extreme,
                                        created_at: bar.timestamp_ms,
                                    };
                                    let _ = broadcast_tx.send(node_msg);

                                    info!(
                                        "Node created: {} {} {:.4}% pw={} (extreme: {:.2}) at {}",
                                        bar.symbol,
                                        direction,
                                        flip.node_move_pct * 100.0,
                                        phase_window,
                                        extreme,
                                        bar.timestamp_ms
                                    );

                                    // Recalculate and broadcast clusters on phase flip
                                    {
                                        let mut cm = cluster_manager.write().await;
                                        if let Some(data) = cm.get_clusters(&bar.symbol) {
                                            let clusters_msg = WsOutMessage::Clusters {
                                                ticker: bar.symbol.clone(),
                                                data,
                                            };
                                            let _ = broadcast_tx.send(clusters_msg);
                                            info!(
                                                "Broadcast clusters for {} after phase flip",
                                                bar.symbol
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // After processing all scenarios for this timeframe,
                    // update cluster manager with bar data for each scenario.
                    // Each scenario uses its own anchor to check node closures.
                    //
                    // First, collect anchors from engines (read locks), then update
                    // cluster manager once (single write lock) to avoid lock thrashing.
                    let mut scenario_anchors: Vec<(Timeframe, usize, f64)> = Vec::new();
                    for &(scenario_tf, phase_window) in &scenarios {
                        if scenario_tf != tf {
                            continue;
                        }
                        let key = (bar.symbol.clone(), tf, phase_window);
                        if let Some(engine_state) = engines.get(&key) {
                            let state = engine_state.read().await;
                            if let Some(anchor) = state.engine.current_anchor() {
                                scenario_anchors.push((scenario_tf, phase_window, anchor));
                            }
                        }
                    }

                    // Single write lock for all cluster manager updates
                    if !scenario_anchors.is_empty() {
                        let mut cm = cluster_manager.write().await;
                        let mut any_nodes_closed = false;

                        for (scenario_tf, phase_window, anchor) in scenario_anchors {
                            let closed_count = cm.update_scenario_with_bar(
                                &bar.symbol,
                                scenario_tf.as_str(),
                                phase_window,
                                anchor,
                                bar.high,
                                bar.low,
                                bar.timestamp_ms,
                            );
                            if closed_count > 0 {
                                any_nodes_closed = true;
                            }
                        }

                        // Broadcast updated clusters after every bar close
                        // This ensures frontend always has fresh cluster positions as anchors move
                        if let Some(data) = cm.get_clusters(&bar.symbol) {
                            let clusters_msg = WsOutMessage::Clusters {
                                ticker: bar.symbol.clone(),
                                data,
                            };
                            let _ = broadcast_tx.send(clusters_msg);
                            if any_nodes_closed {
                                info!("Broadcast clusters for {} after node(s) closed", bar.symbol);
                            }
                        }
                    }
                }
            }
        }
    }

    warn!("Bar event receiver closed");
}

/// Handle a single WebSocket client connection
async fn handle_client(
    stream: TcpStream,
    peer_addr: SocketAddr,
    client_id: ClientId,
    clients: Arc<DashMap<ClientId, ClientState>>,
    mut broadcast_rx: broadcast::Receiver<WsOutMessage>,
    bar_builder: Arc<RwLock<BarBuilder>>,
    engines: Arc<DashMap<(String, Timeframe, usize), RwLock<EngineState>>>,
    cluster_manager: Arc<RwLock<ClusterManager>>,
    symbols: Vec<String>,
) -> Result<()> {
    info!("Client {} connected from {}", client_id, peer_addr);

    let ws_stream = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Channel for sending messages to this client
    let (client_tx, mut client_rx) = mpsc::channel::<WsOutMessage>(1000);

    // Register client
    clients.insert(
        client_id,
        ClientState {
            sender: client_tx.clone(),
            subscribed_symbols: symbols.clone(),
            subscribed_timeframes: vec![Timeframe::M1, Timeframe::M5, Timeframe::M15],
        },
    );

    // Send connected message
    let connected_msg = WsOutMessage::Connected {
        client_id,
        symbols: symbols.clone(),
    };
    let msg_json = serde_json::to_string(&connected_msg)?;
    ws_sender.send(Message::Text(msg_json.into())).await?;

    // Main client loop
    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsInMessage>(&text) {
                            Ok(in_msg) => {
                                handle_client_message(
                                    client_id,
                                    in_msg,
                                    &clients,
                                    &bar_builder,
                                    &engines,
                                    &cluster_manager,
                                    &client_tx,
                                ).await?;
                            }
                            Err(e) => {
                                debug!("Invalid message from client {}: {}", client_id, e);
                                let err_msg = WsOutMessage::Error {
                                    message: format!("Invalid message: {}", e),
                                };
                                let _ = client_tx.send(err_msg).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        ws_sender.send(Message::Pong(data)).await?;
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("Client {} closed connection", client_id);
                        break;
                    }
                    Some(Err(e)) => {
                        debug!("WebSocket error for client {}: {}", client_id, e);
                        break;
                    }
                    None => {
                        debug!("WebSocket stream ended for client {}", client_id);
                        break;
                    }
                    _ => {}
                }
            }

            // Handle broadcast messages
            result = broadcast_rx.recv() => {
                match result {
                    Ok(msg) => {
                        // Filter based on client subscriptions
                        if should_send_to_client(client_id, &msg, &clients) {
                            if let Ok(json) = serde_json::to_string(&msg) {
                                if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Client {} lagged {} messages", client_id, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // Handle direct messages to this client
            Some(msg) = client_rx.recv() => {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if ws_sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    // Cleanup
    clients.remove(&client_id);
    info!("Client {} disconnected", client_id);

    Ok(())
}

/// Handle incoming client message
async fn handle_client_message(
    client_id: ClientId,
    msg: WsInMessage,
    clients: &Arc<DashMap<ClientId, ClientState>>,
    bar_builder: &Arc<RwLock<BarBuilder>>,
    _engines: &Arc<DashMap<(String, Timeframe, usize), RwLock<EngineState>>>,
    cluster_manager: &Arc<RwLock<ClusterManager>>,
    client_tx: &mpsc::Sender<WsOutMessage>,
) -> Result<()> {
    match msg {
        WsInMessage::Subscribe { symbol, timeframes } => {
            let symbol_upper = symbol.to_uppercase();

            if let Some(mut client) = clients.get_mut(&client_id) {
                if !client.subscribed_symbols.contains(&symbol_upper) {
                    client.subscribed_symbols.push(symbol_upper.clone());
                }
                if let Some(tfs) = timeframes {
                    client.subscribed_timeframes =
                        tfs.iter().filter_map(|s| Timeframe::from_str(s)).collect();
                }
            }

            debug!(
                "Client {} subscribed to {} (Sierra)",
                client_id, symbol_upper
            );
        }

        WsInMessage::Unsubscribe { symbol } => {
            let symbol_upper = symbol.to_uppercase();

            if let Some(mut client) = clients.get_mut(&client_id) {
                client.subscribed_symbols.retain(|s| s != &symbol_upper);
            }

            debug!("Client {} unsubscribed from {}", client_id, symbol_upper);
        }

        WsInMessage::GetHistory {
            symbol,
            timeframe,
            limit,
        } => {
            let symbol_upper = symbol.to_uppercase();
            let tf = Timeframe::from_str(&timeframe).unwrap_or(Timeframe::M1);
            let builder = bar_builder.read().await;
            let bars = builder.get_bars(&symbol_upper, tf);

            let limit = limit.unwrap_or(500);
            let history_bars: Vec<HistoryBar> = bars
                .into_iter()
                .rev()
                .take(limit)
                .rev()
                .map(|b| HistoryBar {
                    timestamp_ms: b.timestamp_ms,
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    volume: b.volume,
                })
                .collect();

            // Log first and last bar for debugging
            if !history_bars.is_empty() {
                let first = &history_bars[0];
                let last = &history_bars[history_bars.len() - 1];
                debug!(
                    "Sending {} history bars for {} {}: first=[time={}, O={:.2} H={:.2} L={:.2} C={:.2}] last=[time={}, O={:.2} H={:.2} L={:.2} C={:.2}]",
                    history_bars.len(), symbol_upper, timeframe,
                    first.timestamp_ms, first.open, first.high, first.low, first.close,
                    last.timestamp_ms, last.open, last.high, last.low, last.close
                );
            }

            let msg = WsOutMessage::History {
                symbol: symbol_upper,
                timeframe,
                bars: history_bars,
            };
            let _ = client_tx.send(msg).await;
        }

        WsInMessage::GetNodes { symbol, timeframe } => {
            let symbol_upper = symbol.to_uppercase();
            // For now, send empty nodes - full node tracking requires more state
            let msg = WsOutMessage::OpenNodes {
                symbol: symbol_upper,
                timeframe,
                nodes: vec![],
            };
            let _ = client_tx.send(msg).await;
        }

        WsInMessage::GetClusters { ticker } => {
            let ticker_upper = ticker.to_uppercase();
            info!(
                "GetClusters request for {} from client {}",
                ticker_upper, client_id
            );

            // Use Rust ClusterManager for all symbols
            let mut cm = cluster_manager.write().await;
            if let Some(data) = cm.get_clusters(&ticker_upper) {
                let msg = WsOutMessage::Clusters {
                    ticker: ticker_upper.clone(),
                    data,
                };
                let _ = client_tx.send(msg).await;
                info!(
                    "Sent clusters for {} to client {} (via Rust ClusterManager)",
                    ticker_upper, client_id
                );
            } else {
                // No clusters yet - might be warming up
                let msg = WsOutMessage::Clusters {
                    ticker: ticker_upper.clone(),
                    data: ClustersData {
                        ticker: ticker_upper.clone(),
                        anchor: None,
                        clusters: vec![],
                        nodes: vec![],
                        generated_at: chrono::Utc::now().to_rfc3339(),
                    },
                };
                let _ = client_tx.send(msg).await;
                info!(
                    "No clusters available yet for {} (warming up)",
                    ticker_upper
                );
            }
        }

        WsInMessage::Ping { timestamp } => {
            let msg = WsOutMessage::Pong { timestamp };
            let _ = client_tx.send(msg).await;
        }
    }

    Ok(())
}

/// Check if a broadcast message should be sent to a specific client
fn should_send_to_client(
    client_id: ClientId,
    msg: &WsOutMessage,
    clients: &Arc<DashMap<ClientId, ClientState>>,
) -> bool {
    let Some(client) = clients.get(&client_id) else {
        return false;
    };

    match msg {
        WsOutMessage::Tick { symbol, .. }
        | WsOutMessage::BarUpdate { symbol, .. }
        | WsOutMessage::BarClosed { symbol, .. }
        | WsOutMessage::PhaseUpdate { symbol, .. }
        | WsOutMessage::NodeCreated { symbol, .. }
        | WsOutMessage::NodeClosed { symbol, .. } => client.subscribed_symbols.contains(symbol),
        // Always send these
        WsOutMessage::Connected { .. }
        | WsOutMessage::Error { .. }
        | WsOutMessage::Pong { .. }
        | WsOutMessage::OpenNodes { .. }
        | WsOutMessage::History { .. }
        | WsOutMessage::Clusters { .. } => true,
    }
}
