//! DTC Protocol Client
//!
//! Async client for connecting to Sierra Chart's DTC server.
//! Uses JSON encoding for simplicity and compatibility.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, trace, warn};

use super::protocol::*;

/// Tick data from a trade update
#[derive(Debug, Clone)]
pub struct Tick {
    pub symbol: String,
    pub symbol_id: u16,
    pub price: f64,
    pub volume: f64,
    pub timestamp: f64,
    pub at_bid_or_ask: u8,
    pub bid_price: f64,
    pub ask_price: f64,
}

/// Historical bar data
#[derive(Debug, Clone)]
pub struct HistoricalBar {
    pub symbol: String,
    pub request_id: i32,
    pub start_date_time: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub is_final: bool,
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Encoding,
    LoggingIn,
    Connected,
    Reconnecting,
}

/// Symbol subscription state
#[derive(Debug, Clone)]
pub struct SymbolSubscription {
    pub symbol: String,
    pub symbol_id: u16,
    pub subscribed: bool,
    pub last_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub session_high: f64,
    pub session_low: f64,
    pub session_volume: f64,
}

/// DTC Client configuration
#[derive(Debug, Clone)]
pub struct DtcClientConfig {
    pub host: String,
    pub port: u16,
    pub client_name: String,
    pub heartbeat_interval: Duration,
    pub reconnect_delay: Duration,
    pub max_reconnect_attempts: u32,
}

impl Default for DtcClientConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 11099,
            client_name: "PhaseShifter".to_string(),
            heartbeat_interval: Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS as u64),
            reconnect_delay: Duration::from_secs(5),
            max_reconnect_attempts: 10,
        }
    }
}

/// Historical data request tracking
#[derive(Debug, Clone)]
struct HistoricalRequest {
    request_id: i32,
    symbol: String,
    exchange: String,
    interval: HistoricalDataInterval,
    max_days: u32,
    bars_received: u32,
}

/// Internal state shared between tasks
struct ClientState {
    state: ConnectionState,
    subscriptions: HashMap<String, SymbolSubscription>,
    symbol_id_map: HashMap<u16, String>,
    next_symbol_id: u16,
    reconnect_attempts: u32,
    next_request_id: i32,
    historical_requests: HashMap<i32, HistoricalRequest>,
}

impl ClientState {
    fn new() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            subscriptions: HashMap::new(),
            symbol_id_map: HashMap::new(),
            next_symbol_id: 1,
            reconnect_attempts: 0,
            next_request_id: 1,
            historical_requests: HashMap::new(),
        }
    }
}

/// DTC Protocol Client
pub struct DtcClient {
    config: DtcClientConfig,
    state: Arc<RwLock<ClientState>>,
    tick_sender: mpsc::Sender<Tick>,
    tick_receiver: Option<mpsc::Receiver<Tick>>,
    historical_bar_sender: mpsc::Sender<HistoricalBar>,
    historical_bar_receiver: Option<mpsc::Receiver<HistoricalBar>>,
    shutdown_sender: Option<mpsc::Sender<()>>,
}

impl DtcClient {
    /// Create a new DTC client
    pub fn new(config: DtcClientConfig) -> Self {
        let (tick_tx, tick_rx) = mpsc::channel(10000); // Large buffer for tick data
        let (hist_tx, hist_rx) = mpsc::channel(10000); // Large buffer for historical bars

        Self {
            config,
            state: Arc::new(RwLock::new(ClientState::new())),
            tick_sender: tick_tx,
            tick_receiver: Some(tick_rx),
            historical_bar_sender: hist_tx,
            historical_bar_receiver: Some(hist_rx),
            shutdown_sender: None,
        }
    }

    /// Take the tick receiver (can only be called once)
    pub fn take_tick_receiver(&mut self) -> Option<mpsc::Receiver<Tick>> {
        self.tick_receiver.take()
    }

    /// Take the historical bar receiver (can only be called once)
    pub fn take_historical_bar_receiver(&mut self) -> Option<mpsc::Receiver<HistoricalBar>> {
        self.historical_bar_receiver.take()
    }

    /// Get current connection state
    pub async fn state(&self) -> ConnectionState {
        self.state.read().await.state
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.state.read().await.state == ConnectionState::Connected
    }

    /// Connect to the DTC server
    pub async fn connect(&mut self) -> Result<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_sender = Some(shutdown_tx);

        let config = self.config.clone();
        let state = self.state.clone();
        let tick_sender = self.tick_sender.clone();
        let historical_bar_sender = self.historical_bar_sender.clone();

        tokio::spawn(async move {
            if let Err(e) = run_client(
                config,
                state,
                tick_sender,
                historical_bar_sender,
                shutdown_rx,
            )
            .await
            {
                error!("DTC client error: {}", e);
            }
        });

        // Wait for connection to establish
        for _ in 0..50 {
            if self.is_connected().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        bail!("Timeout waiting for DTC connection")
    }

    /// Disconnect from the DTC server
    pub async fn disconnect(&mut self) {
        if let Some(tx) = self.shutdown_sender.take() {
            let _ = tx.send(()).await;
        }
    }

    /// Subscribe to market data for a symbol
    pub async fn subscribe(&self, symbol: &str) -> Result<u16> {
        let mut state = self.state.write().await;

        // Check if already subscribed
        if let Some(sub) = state.subscriptions.get(symbol) {
            return Ok(sub.symbol_id);
        }

        let symbol_id = state.next_symbol_id;
        state.next_symbol_id += 1;

        state.subscriptions.insert(
            symbol.to_string(),
            SymbolSubscription {
                symbol: symbol.to_string(),
                symbol_id,
                subscribed: false,
                last_price: 0.0,
                bid_price: 0.0,
                ask_price: 0.0,
                session_high: 0.0,
                session_low: 0.0,
                session_volume: 0.0,
            },
        );
        state.symbol_id_map.insert(symbol_id, symbol.to_string());

        Ok(symbol_id)
    }

    /// Get subscription info for a symbol
    pub async fn get_subscription(&self, symbol: &str) -> Option<SymbolSubscription> {
        self.state.read().await.subscriptions.get(symbol).cloned()
    }

    /// Get all subscribed symbols
    pub async fn get_subscribed_symbols(&self) -> Vec<String> {
        self.state
            .read()
            .await
            .subscriptions
            .keys()
            .cloned()
            .collect()
    }

    /// Request historical bar data for a symbol
    /// Returns the request ID that can be used to track responses
    pub async fn request_historical_data(
        &self,
        symbol: &str,
        exchange: &str,
        interval: HistoricalDataInterval,
        max_days: u32,
    ) -> Result<i32> {
        let mut state = self.state.write().await;

        let request_id = state.next_request_id;
        state.next_request_id += 1;

        // Track the request
        state.historical_requests.insert(
            request_id,
            HistoricalRequest {
                request_id,
                symbol: symbol.to_string(),
                exchange: exchange.to_string(),
                interval,
                max_days,
                bars_received: 0,
            },
        );

        info!(
            "Requesting historical data: symbol={}, interval={:?}, max_days={}, request_id={}",
            symbol, interval, max_days, request_id
        );

        Ok(request_id)
    }
}

/// Main client loop
async fn run_client(
    config: DtcClientConfig,
    state: Arc<RwLock<ClientState>>,
    tick_sender: mpsc::Sender<Tick>,
    historical_bar_sender: mpsc::Sender<HistoricalBar>,
    mut shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let mut reconnect_attempts = 0u32;

    loop {
        // Check for shutdown
        if shutdown_rx.try_recv().is_ok() {
            info!("DTC client shutting down");
            state.write().await.state = ConnectionState::Disconnected;
            break;
        }

        // Attempt connection
        state.write().await.state = ConnectionState::Connecting;

        match connect_and_run(
            &config,
            state.clone(),
            tick_sender.clone(),
            historical_bar_sender.clone(),
            &mut shutdown_rx,
        )
        .await
        {
            Ok(()) => {
                info!("DTC connection closed gracefully");
                break;
            }
            Err(e) => {
                error!("DTC connection error: {}", e);
                reconnect_attempts += 1;

                if reconnect_attempts >= config.max_reconnect_attempts {
                    error!(
                        "Max reconnect attempts ({}) exceeded",
                        config.max_reconnect_attempts
                    );
                    state.write().await.state = ConnectionState::Disconnected;
                    break;
                }

                state.write().await.state = ConnectionState::Reconnecting;
                let delay = config.reconnect_delay * reconnect_attempts;
                info!(
                    "Reconnecting in {:?} (attempt {}/{})",
                    delay, reconnect_attempts, config.max_reconnect_attempts
                );

                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = shutdown_rx.recv() => {
                        info!("Shutdown during reconnect wait");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Send a JSON message (null-terminated)
async fn send_json(writer: &mut tokio::net::tcp::OwnedWriteHalf, msg: &Value) -> Result<()> {
    let json_str = serde_json::to_string(msg)?;
    debug!("Sending JSON: {}", json_str);
    writer.write_all(json_str.as_bytes()).await?;
    writer.write_all(b"\0").await?;
    writer.flush().await?;
    Ok(())
}

/// Connect to server and run the message loop
async fn connect_and_run(
    config: &DtcClientConfig,
    state: Arc<RwLock<ClientState>>,
    tick_sender: mpsc::Sender<Tick>,
    historical_bar_sender: mpsc::Sender<HistoricalBar>,
    shutdown_rx: &mut mpsc::Receiver<()>,
) -> Result<()> {
    info!(
        "Connecting to DTC server at {}:{}",
        config.host, config.port
    );

    // Connect with timeout
    let stream = timeout(
        Duration::from_secs(10),
        TcpStream::connect(format!("{}:{}", config.host, config.port)),
    )
    .await
    .context("Connection timeout")?
    .context("Failed to connect")?;

    stream.set_nodelay(true)?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    info!("TCP connection established");

    // Step 1: Send binary encoding request to switch to JSON
    state.write().await.state = ConnectionState::Encoding;
    let encoding_req = EncodingRequest::default();
    let encoded = encoding_req.encode();
    debug!("Sending encoding request: {:?}", &encoded[..]);
    writer.write_all(&encoded).await?;
    writer.flush().await?;

    // Read encoding response (binary format, 16 bytes)
    let mut enc_resp = [0u8; 16];
    reader.read_exact(&mut enc_resp).await?;
    debug!("Encoding response: {:?}", enc_resp);

    // Parse encoding response
    let resp_size = u16::from_le_bytes([enc_resp[0], enc_resp[1]]);
    let resp_type = u16::from_le_bytes([enc_resp[2], enc_resp[3]]);
    let resp_version = i32::from_le_bytes([enc_resp[4], enc_resp[5], enc_resp[6], enc_resp[7]]);
    let resp_encoding = i32::from_le_bytes([enc_resp[8], enc_resp[9], enc_resp[10], enc_resp[11]]);

    info!(
        "Encoding response: size={}, type={}, version={}, encoding={}",
        resp_size, resp_type, resp_version, resp_encoding
    );

    if resp_encoding != Encoding::Json as i32 {
        bail!(
            "Server did not accept JSON encoding (got encoding={})",
            resp_encoding
        );
    }

    // Step 2: Send JSON logon request
    state.write().await.state = ConnectionState::LoggingIn;
    let logon_req = serde_json::json!({
        "Type": MessageType::LogonRequest as u16,
        "ProtocolVersion": DTC_VERSION,
        "Username": "",
        "Password": "",
        "HeartbeatIntervalInSeconds": config.heartbeat_interval.as_secs() as i32,
        "ClientName": config.client_name
    });
    send_json(&mut writer, &logon_req).await?;

    // Read buffer for JSON messages (null-terminated)
    let mut heartbeat_interval = interval(config.heartbeat_interval);
    let mut json_buffer = String::new();

    loop {
        tokio::select! {
            // Check for shutdown
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received");
                return Ok(());
            }

            // Heartbeat timer
            _ = heartbeat_interval.tick() => {
                if state.read().await.state == ConnectionState::Connected {
                    let hb = serde_json::json!({
                        "Type": MessageType::Heartbeat as u16,
                        "CurrentDateTime": chrono::Utc::now().timestamp()
                    });
                    if let Err(e) = send_json(&mut writer, &hb).await {
                        error!("Failed to send heartbeat: {}", e);
                        return Err(e);
                    }
                    trace!("Sent heartbeat");
                }
            }

            // Read JSON messages (null-terminated)
            result = read_json_message(&mut reader, &mut json_buffer) => {
                match result {
                    Ok(Some(msg)) => {
                        handle_json_message(
                            msg,
                            config,
                            &state,
                            &tick_sender,
                            &historical_bar_sender,
                            &mut writer,
                        ).await?;
                    }
                    Ok(None) => {
                        warn!("Server closed connection");
                        return Err(anyhow::anyhow!("Server closed connection"));
                    }
                    Err(e) => {
                        error!("Read error: {}", e);
                        return Err(e);
                    }
                }
            }
        }
    }
}

/// Read a null-terminated JSON message
async fn read_json_message(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    _buffer: &mut String,
) -> Result<Option<Value>> {
    // Read bytes until we hit a null terminator
    let mut json_bytes = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match reader.read(&mut byte).await {
            Ok(0) => {
                // EOF
                return Ok(None);
            }
            Ok(_) => {
                if byte[0] == 0 {
                    break;
                }
                json_bytes.push(byte[0]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }
    }

    let json_str = String::from_utf8(json_bytes)?;
    if json_str.is_empty() {
        // Empty message, read another
        return Box::pin(read_json_message(reader, _buffer)).await;
    }

    debug!("Received JSON: {}", json_str);
    let msg: Value = serde_json::from_str(&json_str)?;
    Ok(Some(msg))
}

/// Handle a JSON DTC message
async fn handle_json_message(
    msg: Value,
    config: &DtcClientConfig,
    state: &Arc<RwLock<ClientState>>,
    tick_sender: &mpsc::Sender<Tick>,
    historical_bar_sender: &mpsc::Sender<HistoricalBar>,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
) -> Result<()> {
    let msg_type = msg.get("Type").and_then(|v| v.as_u64()).unwrap_or(0) as u16;

    match msg_type {
        2 => {
            // LOGON_RESPONSE
            let result = msg.get("Result").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let result_text = msg.get("ResultText").and_then(|v| v.as_str()).unwrap_or("");
            let server_name = msg.get("ServerName").and_then(|v| v.as_str()).unwrap_or("");

            if result == 1 {
                // LOGON_SUCCESS
                info!("Logged in successfully. Server: {}", server_name);
                state.write().await.state = ConnectionState::Connected;
                state.write().await.reconnect_attempts = 0;

                // Subscribe to pending symbols
                let symbols: Vec<(String, u16)> = {
                    let s = state.read().await;
                    s.subscriptions
                        .iter()
                        .filter(|(_, sub)| !sub.subscribed)
                        .map(|(sym, sub)| (sym.clone(), sub.symbol_id))
                        .collect()
                };

                for (symbol, symbol_id) in symbols {
                    let req = serde_json::json!({
                        "Type": MessageType::MarketDataRequest as u16,
                        "RequestAction": 1, // SUBSCRIBE
                        "SymbolID": symbol_id,
                        "Symbol": symbol,
                        "Exchange": ""
                    });
                    send_json(writer, &req).await?;
                    info!(
                        "Sent subscription request for {} (id={})",
                        symbol, symbol_id
                    );
                }

                // Send pending historical data requests
                let historical_requests: Vec<(i32, String, String, i32, u32)> = {
                    let s = state.read().await;
                    s.historical_requests
                        .values()
                        .map(|req| {
                            (
                                req.request_id,
                                req.symbol.clone(),
                                req.exchange.clone(),
                                req.interval as i32,
                                req.max_days,
                            )
                        })
                        .collect()
                };

                for (request_id, symbol, exchange, interval, max_days) in historical_requests {
                    let req = serde_json::json!({
                        "Type": MessageType::HistoricalPriceDataRequest as u16,
                        "RequestID": request_id,
                        "Symbol": symbol,
                        "Exchange": exchange,
                        "RecordInterval": interval,
                        "StartDateTime": 0.0,
                        "EndDateTime": 0.0,
                        "MaxDaysToReturn": max_days,
                        "UseZLibCompression": 0,
                        "RequestDividendAdjustedStockData": 0,
                        "Integer_1": 0
                    });
                    send_json(writer, &req).await?;
                    info!(
                        "Sent historical data request for symbol={}, interval={}, request_id={}",
                        symbol, interval, request_id
                    );
                }
            } else {
                error!("Logon failed: {} - {}", result, result_text);
                return Err(anyhow::anyhow!("Logon failed: {}", result_text));
            }
        }

        3 => {
            // HEARTBEAT
            let dropped = msg
                .get("NumDroppedMessages")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            trace!("Received heartbeat (dropped={})", dropped);
            if dropped > 0 {
                warn!("Server reports {} dropped messages", dropped);
            }
        }

        103 => {
            // MARKET_DATA_REJECT
            let symbol_id = msg.get("SymbolID").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let reject_text = msg.get("RejectText").and_then(|v| v.as_str()).unwrap_or("");
            let s = state.read().await;
            let symbol = s.symbol_id_map.get(&symbol_id).cloned();
            error!(
                "Market data rejected for {:?} (id={}): {}",
                symbol, symbol_id, reject_text
            );
        }

        104 => {
            // MARKET_DATA_SNAPSHOT
            let symbol_id = msg.get("SymbolID").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let last_price = msg
                .get("LastTradePrice")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let bid_price = msg.get("BidPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let ask_price = msg.get("AskPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let volume = msg
                .get("LastTradeVolume")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let timestamp = msg
                .get("LastTradeDateTime")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let session_high = msg
                .get("SessionHighPrice")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let session_low = msg
                .get("SessionLowPrice")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let session_volume = msg
                .get("SessionVolume")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            let mut s = state.write().await;
            if let Some(symbol) = s.symbol_id_map.get(&symbol_id).cloned() {
                if let Some(sub) = s.subscriptions.get_mut(&symbol) {
                    sub.subscribed = true;
                    sub.last_price = last_price;
                    sub.bid_price = bid_price;
                    sub.ask_price = ask_price;
                    sub.session_high = session_high;
                    sub.session_low = session_low;
                    sub.session_volume = session_volume;

                    info!(
                        "Snapshot for {}: last={:.2}, bid={:.2}, ask={:.2}",
                        symbol, last_price, bid_price, ask_price
                    );

                    // Send initial tick
                    if last_price > 0.0 {
                        let tick = Tick {
                            symbol: symbol.clone(),
                            symbol_id,
                            price: last_price,
                            volume,
                            timestamp,
                            at_bid_or_ask: 0,
                            bid_price,
                            ask_price,
                        };
                        let _ = tick_sender.send(tick).await;
                    }
                }
            }
        }

        107 | 112 => {
            // MARKET_DATA_UPDATE_TRADE or MARKET_DATA_UPDATE_TRADE_COMPACT
            let symbol_id = msg.get("SymbolID").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let price = msg.get("Price").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let volume = msg.get("Volume").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let timestamp = msg.get("DateTime").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let at_bid_or_ask = msg.get("AtBidOrAsk").and_then(|v| v.as_u64()).unwrap_or(0) as u8;

            let s = state.read().await;
            if let Some(symbol) = s.symbol_id_map.get(&symbol_id).cloned() {
                let (bid, ask) = s
                    .subscriptions
                    .get(&symbol)
                    .map(|sub| (sub.bid_price, sub.ask_price))
                    .unwrap_or((0.0, 0.0));
                drop(s);

                // Update last price
                {
                    let mut s = state.write().await;
                    if let Some(sub) = s.subscriptions.get_mut(&symbol) {
                        sub.last_price = price;
                    }
                }

                let tick = Tick {
                    symbol,
                    symbol_id,
                    price,
                    volume,
                    timestamp,
                    at_bid_or_ask,
                    bid_price: bid,
                    ask_price: ask,
                };
                let _ = tick_sender.send(tick).await;
            }
        }

        108 | 117 | 143 => {
            // MARKET_DATA_UPDATE_BID_ASK variants
            let symbol_id = msg.get("SymbolID").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let bid_price = msg.get("BidPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let ask_price = msg.get("AskPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let mut s = state.write().await;
            if let Some(symbol) = s.symbol_id_map.get(&symbol_id).cloned() {
                if let Some(sub) = s.subscriptions.get_mut(&symbol) {
                    sub.bid_price = bid_price;
                    sub.ask_price = ask_price;
                }
            }
        }

        114 => {
            // MARKET_DATA_UPDATE_SESSION_HIGH
            let symbol_id = msg.get("SymbolID").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let price = msg.get("Price").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let mut s = state.write().await;
            if let Some(symbol) = s.symbol_id_map.get(&symbol_id).cloned() {
                if let Some(sub) = s.subscriptions.get_mut(&symbol) {
                    sub.session_high = price;
                }
            }
        }

        115 => {
            // MARKET_DATA_UPDATE_SESSION_LOW
            let symbol_id = msg.get("SymbolID").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let price = msg.get("Price").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let mut s = state.write().await;
            if let Some(symbol) = s.symbol_id_map.get(&symbol_id).cloned() {
                if let Some(sub) = s.subscriptions.get_mut(&symbol) {
                    sub.session_low = price;
                }
            }
        }

        113 => {
            // MARKET_DATA_UPDATE_SESSION_VOLUME
            let symbol_id = msg.get("SymbolID").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let volume = msg.get("Volume").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let mut s = state.write().await;
            if let Some(symbol) = s.symbol_id_map.get(&symbol_id).cloned() {
                if let Some(sub) = s.subscriptions.get_mut(&symbol) {
                    sub.session_volume = volume;
                }
            }
        }

        801 => {
            // HISTORICAL_PRICE_DATA_RESPONSE_HEADER
            let request_id = msg.get("RequestID").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let no_records = msg
                .get("NoRecordsToReturn")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u8;

            if no_records == 1 {
                let s = state.read().await;
                if let Some(req) = s.historical_requests.get(&request_id) {
                    warn!(
                        "No historical data available for symbol={}, request_id={}",
                        req.symbol, request_id
                    );
                }
            } else {
                let s = state.read().await;
                if let Some(req) = s.historical_requests.get(&request_id) {
                    info!(
                        "Historical data stream starting for symbol={}, request_id={}",
                        req.symbol, request_id
                    );
                }
            }
        }

        802 => {
            // HISTORICAL_PRICE_DATA_REJECT
            let request_id = msg.get("RequestID").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let reject_text = msg.get("RejectText").and_then(|v| v.as_str()).unwrap_or("");

            let s = state.read().await;
            if let Some(req) = s.historical_requests.get(&request_id) {
                error!(
                    "Historical data rejected for symbol={}, request_id={}: {}",
                    req.symbol, request_id, reject_text
                );
            }
        }

        803 => {
            // HISTORICAL_PRICE_DATA_RECORD_RESPONSE
            let request_id = msg.get("RequestID").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let start_date_time = msg
                .get("StartDateTime")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let open_price = msg.get("OpenPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let high_price = msg.get("HighPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let low_price = msg.get("LowPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let last_price = msg.get("LastPrice").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let volume = msg.get("Volume").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let is_final = msg
                .get("IsFinalRecord")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                == 1;

            let symbol = {
                let mut s = state.write().await;
                if let Some(req) = s.historical_requests.get_mut(&request_id) {
                    req.bars_received += 1;
                    Some(req.symbol.clone())
                } else {
                    None
                }
            };

            if let Some(symbol) = symbol {
                let bar = HistoricalBar {
                    symbol,
                    request_id,
                    start_date_time,
                    open: open_price,
                    high: high_price,
                    low: low_price,
                    close: last_price,
                    volume,
                    is_final,
                };

                // Send to channel (non-blocking)
                let _ = historical_bar_sender.try_send(bar);

                if is_final {
                    let s = state.read().await;
                    if let Some(req) = s.historical_requests.get(&request_id) {
                        info!(
                            "Historical data complete for symbol={}, received {} bars, request_id={}",
                            req.symbol, req.bars_received, request_id
                        );
                    }
                }
            }
        }

        _ => {
            trace!("Unknown message type {}: {:?}", msg_type, msg);
        }
    }

    Ok(())
}
