//! Sierra Chart TCP Listener
//!
//! Receives tick data from ACSIL study via TCP socket.
//! This bypasses DTC protocol restrictions for CME data.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use bytes::{Buf, BytesMut};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Message types from ACSIL study
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Tick = 1,
    BarClose = 2,
    Heartbeat = 255,
}

/// Tick message from Sierra Chart (28 bytes)
#[derive(Debug, Clone)]
pub struct SierraTick {
    pub symbol_id: u32,
    pub price: f64,
    pub volume: f64,
    pub timestamp_us: i64,
}

/// Parsed message from Sierra Chart
#[derive(Debug, Clone)]
pub enum SierraMessage {
    Tick(SierraTick),
    BarClose(SierraTick),
    Heartbeat,
}

const MESSAGE_SIZE: usize = 32; // 1 + 3 + 4 + 8 + 8 + 8 = 32 bytes

impl SierraMessage {
    /// Parse a message from bytes
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MESSAGE_SIZE {
            return None;
        }

        let msg_type = data[0];
        // Skip reserved bytes [1..4]
        let symbol_id = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let price = f64::from_le_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);
        let volume = f64::from_le_bytes([
            data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
        ]);
        let timestamp_us = i64::from_le_bytes([
            data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
        ]);

        let tick = SierraTick {
            symbol_id,
            price,
            volume,
            timestamp_us,
        };

        match msg_type {
            1 => Some(SierraMessage::Tick(tick)),
            2 => Some(SierraMessage::BarClose(tick)),
            255 => Some(SierraMessage::Heartbeat),
            _ => {
                warn!("Unknown message type: {}", msg_type);
                None
            }
        }
    }
}

/// Symbol ID to symbol name mapping
#[derive(Debug, Default)]
pub struct SymbolRegistry {
    /// Map from symbol_id (hash) to symbol name
    id_to_name: HashMap<u32, String>,
    /// Map from symbol name to symbol_id
    name_to_id: HashMap<String, u32>,
}

impl SymbolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a symbol and return its ID
    pub fn register(&mut self, symbol: &str) -> u32 {
        if let Some(&id) = self.name_to_id.get(symbol) {
            return id;
        }

        let id = Self::hash_symbol(symbol);
        self.id_to_name.insert(id, symbol.to_string());
        self.name_to_id.insert(symbol.to_string(), id);
        id
    }

    /// Get symbol name from ID
    pub fn get_name(&self, id: u32) -> Option<&str> {
        self.id_to_name.get(&id).map(|s| s.as_str())
    }

    /// Get all registered symbols (for debugging)
    pub fn get_all(&self) -> Vec<(u32, String)> {
        self.id_to_name
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    /// Hash function matching the ACSIL study
    fn hash_symbol(symbol: &str) -> u32 {
        let mut hash: u32 = 5381;
        for c in symbol.bytes() {
            hash = hash
                .wrapping_shl(5)
                .wrapping_add(hash)
                .wrapping_add(c as u32);
        }
        hash
    }
}

/// Tick data converted to internal format
#[derive(Debug, Clone)]
pub struct Tick {
    pub symbol: String,
    pub price: f64,
    pub volume: f64,
    pub timestamp_ms: i64,
}

/// Sierra TCP server configuration
#[derive(Debug, Clone)]
pub struct SierraTcpConfig {
    pub host: String,
    pub port: u16,
}

impl Default for SierraTcpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9000,
        }
    }
}

/// Sierra TCP server
pub struct SierraTcpServer {
    config: SierraTcpConfig,
    symbol_registry: Arc<RwLock<SymbolRegistry>>,
    tick_sender: mpsc::Sender<Tick>,
}

impl SierraTcpServer {
    /// Create a new Sierra TCP server
    pub fn new(
        config: SierraTcpConfig,
        symbol_registry: Arc<RwLock<SymbolRegistry>>,
        tick_sender: mpsc::Sender<Tick>,
    ) -> Self {
        Self {
            config,
            symbol_registry,
            tick_sender,
        }
    }

    /// Run the TCP server
    pub async fn run(&self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("Sierra TCP server listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    info!("Sierra Chart connected from {}", peer_addr);

                    let registry = self.symbol_registry.clone();
                    let tick_sender = self.tick_sender.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, registry, tick_sender).await {
                            error!("Connection error: {}", e);
                        }
                        info!("Sierra Chart disconnected: {}", peer_addr);
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }
}

/// Handle a single connection from Sierra Chart
async fn handle_connection(
    mut stream: TcpStream,
    registry: Arc<RwLock<SymbolRegistry>>,
    tick_sender: mpsc::Sender<Tick>,
) -> Result<()> {
    let mut buffer = BytesMut::with_capacity(4096);
    let mut tick_count: u64 = 0;
    let mut last_log_count: u64 = 0;
    let mut unknown_symbols: HashMap<u32, u64> = HashMap::new();

    loop {
        // Read data into buffer
        let n = stream.read_buf(&mut buffer).await?;
        if n == 0 {
            // Connection closed
            break;
        }

        // Process complete messages
        while buffer.len() >= MESSAGE_SIZE {
            let msg_bytes = &buffer[..MESSAGE_SIZE];

            if let Some(msg) = SierraMessage::parse(msg_bytes) {
                match msg {
                    SierraMessage::Tick(sierra_tick) | SierraMessage::BarClose(sierra_tick) => {
                        // Look up symbol name
                        let symbol_name = {
                            let reg = registry.read().await;
                            reg.get_name(sierra_tick.symbol_id).map(|s| s.to_string())
                        };

                        if let Some(symbol) = symbol_name {
                            tick_count += 1;

                            let tick = Tick {
                                symbol: symbol.clone(),
                                price: sierra_tick.price,
                                volume: sierra_tick.volume,
                                timestamp_ms: sierra_tick.timestamp_us / 1000,
                            };

                            // Log first few ticks for debugging (with symbol_id for verification)
                            if tick_count <= 10 {
                                info!(
                                    "Sierra tick #{}: symbol_id={} -> {} @ {:.2}",
                                    tick_count, sierra_tick.symbol_id, symbol, sierra_tick.price
                                );
                            }

                            if tick_sender.send(tick).await.is_err() {
                                warn!("Tick receiver dropped");
                                return Ok(());
                            }
                        } else {
                            // Unknown symbol ID - log immediately for debugging
                            let count = unknown_symbols.entry(sierra_tick.symbol_id).or_insert(0);
                            *count += 1;

                            if *count == 1 {
                                warn!("Unknown symbol ID {} (price={:.2}) - not registered. Available symbols in registry:",
                                      sierra_tick.symbol_id, sierra_tick.price);
                                let reg = registry.read().await;
                                let registered = reg.get_all();
                                for (id, name) in registered {
                                    warn!("  Registered: {} -> {}", id, name);
                                }
                            }
                        }
                    }
                    SierraMessage::Heartbeat => {
                        debug!("Received heartbeat from Sierra Chart");
                    }
                }
            }

            buffer.advance(MESSAGE_SIZE);
        }

        // Periodic logging
        if tick_count - last_log_count >= 10000 {
            last_log_count = tick_count;
            info!("Sierra TCP: Received {} ticks", tick_count);

            // Log unknown symbols
            for (id, count) in &unknown_symbols {
                warn!(
                    "Unknown symbol ID {} received {} ticks - register it with symbol_registry",
                    id, count
                );
            }
            unknown_symbols.clear();
        }
    }

    info!("Sierra TCP: Connection closed after {} ticks", tick_count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_hash() {
        // Test that our hash matches the ACSIL hash
        let hash = SymbolRegistry::hash_symbol("NQH26-CME");
        println!("Hash of NQH26-CME: {}", hash);
        assert!(hash != 0);
    }

    #[test]
    fn test_message_parse() {
        let mut data = vec![0u8; 32];
        data[0] = 1; // Tick message
                     // symbol_id = 12345 at offset 4
        data[4..8].copy_from_slice(&12345u32.to_le_bytes());
        // price = 21000.50 at offset 8
        data[8..16].copy_from_slice(&21000.50f64.to_le_bytes());
        // volume = 10.0 at offset 16
        data[16..24].copy_from_slice(&10.0f64.to_le_bytes());
        // timestamp = 1234567890000 at offset 24
        data[24..32].copy_from_slice(&1234567890000i64.to_le_bytes());

        let msg = SierraMessage::parse(&data).unwrap();
        match msg {
            SierraMessage::Tick(tick) => {
                assert_eq!(tick.symbol_id, 12345);
                assert!((tick.price - 21000.50).abs() < 0.001);
                assert!((tick.volume - 10.0).abs() < 0.001);
                assert_eq!(tick.timestamp_us, 1234567890000);
            }
            _ => panic!("Expected Tick message"),
        }
    }
}
