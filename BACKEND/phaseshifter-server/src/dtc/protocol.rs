//! DTC Protocol message definitions and binary parsing
//!
//! Implements the binary variable-length encoding used by Sierra Chart's DTC server.

use bytes::{Buf, BufMut, BytesMut};
use std::io::Cursor;

/// DTC Protocol version
/// Sierra Chart uses the latest version (currently around 52+)
/// The protocol is backward compatible, so we request the latest known version
pub const DTC_VERSION: i32 = 8;
pub const DTC_CURRENT_VERSION: i32 = 52;

/// Heartbeat interval in seconds
pub const HEARTBEAT_INTERVAL_SECONDS: i32 = 10;

/// Encoding types
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Binary = 0,
    BinaryVLen = 1, // Variable length binary (Sierra default)
    BinaryWithTimestamps = 2,
    Json = 3,
    JsonCompact = 4,
    ProtocolBuffers = 5,
}

/// DTC Message Types
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    // Session messages
    LogonRequest = 1,
    LogonResponse = 2,
    Heartbeat = 3,
    Logoff = 5,
    EncodingRequest = 6,
    EncodingResponse = 7,

    // Market data messages
    MarketDataRequest = 101,
    MarketDataReject = 103,
    MarketDataSnapshot = 104,
    MarketDataUpdateTrade = 107,
    MarketDataUpdateTradeCompact = 112,
    MarketDataUpdateBidAsk = 108,
    MarketDataUpdateBidAskCompact = 117,
    MarketDataUpdateBidAskNoTimestamp = 143,
    MarketDataUpdateSessionOpen = 120,
    MarketDataUpdateSessionHigh = 114,
    MarketDataUpdateSessionLow = 115,
    MarketDataUpdateSessionVolume = 113,
    MarketDataUpdateOpenInterest = 124,
    MarketDataUpdateLastTradeSnapshot = 134,

    // Security definition
    SecurityDefinitionForSymbolRequest = 506,
    SecurityDefinitionResponse = 507,

    // Historical price data (correct message type values per DTC protocol)
    HistoricalPriceDataRequest = 800,
    HistoricalPriceDataResponseHeader = 801,
    HistoricalPriceDataReject = 802,
    HistoricalPriceDataRecordResponse = 803,
    HistoricalPriceDataTickRecordResponse = 804,

    // Unknown
    Unknown = 0xFFFF,
}

impl From<u16> for MessageType {
    fn from(value: u16) -> Self {
        match value {
            1 => MessageType::LogonRequest,
            2 => MessageType::LogonResponse,
            3 => MessageType::Heartbeat,
            5 => MessageType::Logoff,
            6 => MessageType::EncodingRequest,
            7 => MessageType::EncodingResponse,
            101 => MessageType::MarketDataRequest,
            103 => MessageType::MarketDataReject,
            104 => MessageType::MarketDataSnapshot,
            107 => MessageType::MarketDataUpdateTrade,
            112 => MessageType::MarketDataUpdateTradeCompact,
            108 => MessageType::MarketDataUpdateBidAsk,
            117 => MessageType::MarketDataUpdateBidAskCompact,
            143 => MessageType::MarketDataUpdateBidAskNoTimestamp,
            120 => MessageType::MarketDataUpdateSessionOpen,
            114 => MessageType::MarketDataUpdateSessionHigh,
            115 => MessageType::MarketDataUpdateSessionLow,
            113 => MessageType::MarketDataUpdateSessionVolume,
            124 => MessageType::MarketDataUpdateOpenInterest,
            134 => MessageType::MarketDataUpdateLastTradeSnapshot,
            506 => MessageType::SecurityDefinitionForSymbolRequest,
            507 => MessageType::SecurityDefinitionResponse,
            800 => MessageType::HistoricalPriceDataRequest,
            801 => MessageType::HistoricalPriceDataResponseHeader,
            802 => MessageType::HistoricalPriceDataReject,
            803 => MessageType::HistoricalPriceDataRecordResponse,
            804 => MessageType::HistoricalPriceDataTickRecordResponse,
            _ => MessageType::Unknown,
        }
    }
}

/// Request action types for market data
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestAction {
    Subscribe = 1,
    Unsubscribe = 2,
    Snapshot = 3,
}

/// Logon status
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogonStatus {
    Success = 1,
    Error = 2,
    ErrorNoReconnect = 3,
    ReconnectNewAddress = 4,
}

impl From<i32> for LogonStatus {
    fn from(value: i32) -> Self {
        match value {
            1 => LogonStatus::Success,
            2 => LogonStatus::Error,
            3 => LogonStatus::ErrorNoReconnect,
            4 => LogonStatus::ReconnectNewAddress,
            _ => LogonStatus::Error,
        }
    }
}

/// Message header (4 bytes)
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub size: u16,
    pub msg_type: u16,
}

impl MessageHeader {
    pub const SIZE: usize = 4;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        let mut cursor = Cursor::new(data);
        let size = cursor.get_u16_le();
        let msg_type = cursor.get_u16_le();
        Some(Self { size, msg_type })
    }

    pub fn write(&self, buf: &mut BytesMut) {
        buf.put_u16_le(self.size);
        buf.put_u16_le(self.msg_type);
    }
}

/// Encoding request message
#[derive(Debug)]
pub struct EncodingRequest {
    pub protocol_version: i32,
    pub encoding: i32,
    pub protocol_type: [u8; 4],
}

impl Default for EncodingRequest {
    fn default() -> Self {
        Self {
            protocol_version: DTC_VERSION,   // Use version 8
            encoding: Encoding::Json as i32, // Use JSON encoding - simpler and proven to work
            protocol_type: *b"DTC\0",
        }
    }
}

impl EncodingRequest {
    pub fn encode(&self) -> BytesMut {
        let body_size = 4 + 4 + 4; // version + encoding + protocol_type
        let total_size = MessageHeader::SIZE + body_size;

        let mut buf = BytesMut::with_capacity(total_size);

        // Header
        buf.put_u16_le(total_size as u16);
        buf.put_u16_le(MessageType::EncodingRequest as u16);

        // Body
        buf.put_i32_le(self.protocol_version);
        buf.put_i32_le(self.encoding);
        buf.put_slice(&self.protocol_type);

        eprintln!(
            "EncodingRequest: size={}, version={}, encoding={}, bytes: {:?}",
            total_size,
            self.protocol_version,
            self.encoding,
            &buf[..]
        );

        buf
    }
}

/// Encoding response message
#[derive(Debug)]
pub struct EncodingResponse {
    pub protocol_version: i32,
    pub encoding: i32,
    pub protocol_type: String,
}

impl EncodingResponse {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 8 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let protocol_version = cursor.get_i32_le();
        let encoding = cursor.get_i32_le();

        let protocol_type = if body.len() >= 12 {
            let bytes = &body[8..12];
            String::from_utf8_lossy(bytes)
                .trim_end_matches('\0')
                .to_string()
        } else {
            String::new()
        };

        Some(Self {
            protocol_version,
            encoding,
            protocol_type,
        })
    }
}

/// Logon request message (DTC Protocol message type 1)
/// Field order per Sierra Chart specification
#[derive(Debug)]
pub struct LogonRequest {
    pub protocol_version: i32,       // Field 1: int32
    pub username: String,            // Field 2: vls (variable-length string)
    pub password: String,            // Field 3: vls
    pub general_text_data: String,   // Field 4: vls
    pub integer_1: i32,              // Field 5: int32
    pub integer_2: i32,              // Field 6: int32
    pub heartbeat_interval: i32,     // Field 7: int32
    pub trade_account: String,       // Field 8: vls
    pub hardware_identifier: String, // Field 9: vls
    pub client_name: String,         // Field 10: vls
    pub trade_mode: i32,             // Field 11: int32 (ADDED LATER - must be at end!)
}

impl Default for LogonRequest {
    fn default() -> Self {
        Self {
            protocol_version: DTC_CURRENT_VERSION, // Use latest version (52)
            username: String::new(),
            password: String::new(),
            general_text_data: String::new(),
            integer_1: 0,
            integer_2: 0,
            heartbeat_interval: HEARTBEAT_INTERVAL_SECONDS,
            trade_mode: 0,
            trade_account: String::new(),
            hardware_identifier: String::new(),
            client_name: "PhaseShifter".to_string(),
        }
    }
}

impl LogonRequest {
    pub fn encode(&self) -> BytesMut {
        // Variable-length binary (VLS) encoding per DTC Protocol specification
        // Strings use offset+length pairs (vls_t), with actual string data appended at end
        // Reference: https://www.sierrachart.com/index.php?page=doc/DTCProtocol.php

        // LOGON_REQUEST structure layout:
        // Header: Size(u16) + Type(u16) = 4 bytes
        // ProtocolVersion: i32 = 4 bytes
        // Username: vls_t (u16 offset + u16 length) = 4 bytes
        // Password: vls_t = 4 bytes
        // GeneralTextData: vls_t = 4 bytes
        // Integer_1: i32 = 4 bytes
        // Integer_2: i32 = 4 bytes
        // HeartbeatIntervalInSeconds: i32 = 4 bytes
        // TradeAccount: vls_t = 4 bytes
        // HardwareIdentifier: vls_t = 4 bytes
        // ClientName: vls_t = 4 bytes
        // TradeMode: i32 = 4 bytes
        // Total fixed size = 48 bytes

        let base_size: u16 = 48;

        // Collect strings in order
        let strings: [&str; 6] = [
            &self.username,
            &self.password,
            &self.general_text_data,
            &self.trade_account,
            &self.hardware_identifier,
            &self.client_name,
        ];

        // Calculate string offsets (from start of message)
        let mut current_offset = base_size;
        let mut string_info: Vec<(u16, u16)> = Vec::new();

        for s in &strings {
            if s.is_empty() {
                string_info.push((0, 0)); // Empty string: offset=0, length=0
            } else {
                let len = (s.len() + 1) as u16; // +1 for null terminator
                string_info.push((current_offset, len));
                current_offset += len;
            }
        }

        let mut buf = BytesMut::with_capacity(current_offset as usize);

        // Header
        buf.put_u16_le(0); // size - will update at end
        buf.put_u16_le(MessageType::LogonRequest as u16);

        // ProtocolVersion (i32)
        buf.put_i32_le(self.protocol_version);

        // Username vls_t
        buf.put_u16_le(string_info[0].0);
        buf.put_u16_le(string_info[0].1);

        // Password vls_t
        buf.put_u16_le(string_info[1].0);
        buf.put_u16_le(string_info[1].1);

        // GeneralTextData vls_t
        buf.put_u16_le(string_info[2].0);
        buf.put_u16_le(string_info[2].1);

        // Integer_1
        buf.put_i32_le(self.integer_1);

        // Integer_2
        buf.put_i32_le(self.integer_2);

        // HeartbeatIntervalInSeconds
        buf.put_i32_le(self.heartbeat_interval);

        // TradeAccount vls_t
        buf.put_u16_le(string_info[3].0);
        buf.put_u16_le(string_info[3].1);

        // HardwareIdentifier vls_t
        buf.put_u16_le(string_info[4].0);
        buf.put_u16_le(string_info[4].1);

        // ClientName vls_t
        buf.put_u16_le(string_info[5].0);
        buf.put_u16_le(string_info[5].1);

        // TradeMode
        buf.put_i32_le(self.trade_mode);

        // Append string data at the end (in same order as vls_t fields)
        for s in &strings {
            if !s.is_empty() {
                buf.put_slice(s.as_bytes());
                buf.put_u8(0); // null terminator
            }
        }

        // Update size field
        let size = buf.len() as u16;
        buf[0..2].copy_from_slice(&size.to_le_bytes());

        // Debug output
        eprintln!(
            "LogonRequest VLS: size={}, protocol_version={}, base={}, bytes: {:?}",
            size,
            self.protocol_version,
            base_size,
            &buf[..]
        );

        buf
    }
}

/// Logon response message
#[derive(Debug)]
pub struct LogonResponse {
    pub protocol_version: i32,
    pub result: LogonStatus,
    pub result_text: String,
    pub server_name: String,
    pub market_data_supported: bool,
    pub historical_price_data_supported: bool,
}

impl LogonResponse {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 8 {
            return None;
        }

        // Debug: print full raw message
        eprintln!(
            "LogonResponse FULL raw data ({} bytes): {:?}",
            data.len(),
            data
        );

        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let protocol_version = cursor.get_i32_le();
        let result_code = cursor.get_i32_le();
        let result = LogonStatus::from(result_code);

        // Parse VLS strings - they use offset+length pairs
        // For now, just try to find the error text in the raw data
        let remaining = &body[8..];

        // Debug: print raw bytes
        eprintln!(
            "LogonResponse - protocol_version: {}, result_code: {}, body bytes: {:?}",
            protocol_version, result_code, body
        );

        // Try to find readable strings in the message
        let strings: Vec<&str> = data
            .split(|&b| b == 0)
            .map(|s| std::str::from_utf8(s).unwrap_or(""))
            .filter(|s| s.len() > 2 && s.chars().all(|c| c.is_ascii_graphic() || c == ' '))
            .collect();

        eprintln!("Readable strings in response: {:?}", strings);

        Some(Self {
            protocol_version,
            result,
            result_text: strings.get(0).unwrap_or(&"No error text").to_string(),
            server_name: strings.get(1).unwrap_or(&"Unknown").to_string(),
            market_data_supported: true,
            historical_price_data_supported: true,
        })
    }
}

/// Heartbeat message
#[derive(Debug)]
pub struct Heartbeat {
    pub num_dropped_messages: u32,
    pub current_date_time: i64,
}

impl Default for Heartbeat {
    fn default() -> Self {
        Self {
            num_dropped_messages: 0,
            current_date_time: chrono::Utc::now().timestamp(),
        }
    }
}

impl Heartbeat {
    pub fn encode(&self) -> BytesMut {
        let body_size = 4 + 8; // u32 + i64
        let total_size = MessageHeader::SIZE + body_size;

        let mut buf = BytesMut::with_capacity(total_size);
        buf.put_u16_le(total_size as u16);
        buf.put_u16_le(MessageType::Heartbeat as u16);
        buf.put_u32_le(self.num_dropped_messages);
        buf.put_i64_le(self.current_date_time);

        buf
    }

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 12 {
            return Some(Self::default());
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let num_dropped_messages = cursor.get_u32_le();
        let current_date_time = cursor.get_i64_le();

        Some(Self {
            num_dropped_messages,
            current_date_time,
        })
    }
}

/// Market data request
#[derive(Debug)]
pub struct MarketDataRequest {
    pub request_action: RequestAction,
    pub symbol_id: u16,
    pub symbol: String,
    pub exchange: String,
}

impl MarketDataRequest {
    pub fn subscribe(symbol_id: u16, symbol: &str) -> Self {
        Self {
            request_action: RequestAction::Subscribe,
            symbol_id,
            symbol: symbol.to_string(),
            exchange: String::new(),
        }
    }

    pub fn unsubscribe(symbol_id: u16, symbol: &str) -> Self {
        Self {
            request_action: RequestAction::Unsubscribe,
            symbol_id,
            symbol: symbol.to_string(),
            exchange: String::new(),
        }
    }

    pub fn encode(&self) -> BytesMut {
        // Fixed: action(2) + symbol_id(2) = 4 bytes
        // Variable: symbol + exchange (null-terminated)
        let mut body = BytesMut::new();
        body.put_u16_le(self.request_action as u16);
        body.put_u16_le(self.symbol_id);
        body.put_slice(self.symbol.as_bytes());
        body.put_u8(0);
        body.put_slice(self.exchange.as_bytes());
        body.put_u8(0);

        let total_size = MessageHeader::SIZE + body.len();
        let mut buf = BytesMut::with_capacity(total_size);
        buf.put_u16_le(total_size as u16);
        buf.put_u16_le(MessageType::MarketDataRequest as u16);
        buf.put_slice(&body);

        buf
    }
}

/// Market data reject
#[derive(Debug)]
pub struct MarketDataReject {
    pub symbol_id: u16,
    pub reject_text: String,
}

impl MarketDataReject {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 2 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let symbol_id = u16::from_le_bytes([body[0], body[1]]);
        let reject_text = String::from_utf8_lossy(&body[2..])
            .trim_end_matches('\0')
            .to_string();

        Some(Self {
            symbol_id,
            reject_text,
        })
    }
}

/// Market data snapshot (initial data for a symbol)
#[derive(Debug, Clone)]
pub struct MarketDataSnapshot {
    pub symbol_id: u16,
    pub session_settlement_price: f64,
    pub session_open_price: f64,
    pub session_high_price: f64,
    pub session_low_price: f64,
    pub session_volume: f64,
    pub session_num_trades: u32,
    pub open_interest: u32,
    pub bid_price: f64,
    pub ask_price: f64,
    pub ask_quantity: f64,
    pub bid_quantity: f64,
    pub last_trade_price: f64,
    pub last_trade_volume: f64,
    pub last_trade_date_time: f64,
    pub bid_ask_date_time: f64,
}

impl MarketDataSnapshot {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 100 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        // Parse fields - Sierra uses doubles (f64) for most price/volume fields
        let symbol_id = cursor.get_u16_le();

        // Skip padding if any
        let _ = cursor.get_u16_le(); // padding

        let session_settlement_price = cursor.get_f64_le();
        let session_open_price = cursor.get_f64_le();
        let session_high_price = cursor.get_f64_le();
        let session_low_price = cursor.get_f64_le();
        let session_volume = cursor.get_f64_le();
        let session_num_trades = cursor.get_u32_le();
        let open_interest = cursor.get_u32_le();
        let bid_price = cursor.get_f64_le();
        let ask_price = cursor.get_f64_le();
        let ask_quantity = cursor.get_f64_le();
        let bid_quantity = cursor.get_f64_le();
        let last_trade_price = cursor.get_f64_le();
        let last_trade_volume = cursor.get_f64_le();
        let last_trade_date_time = cursor.get_f64_le();
        let bid_ask_date_time = cursor.get_f64_le();

        Some(Self {
            symbol_id,
            session_settlement_price,
            session_open_price,
            session_high_price,
            session_low_price,
            session_volume,
            session_num_trades,
            open_interest,
            bid_price,
            ask_price,
            ask_quantity,
            bid_quantity,
            last_trade_price,
            last_trade_volume,
            last_trade_date_time,
            bid_ask_date_time,
        })
    }
}

/// Trade update (real-time tick)
#[derive(Debug, Clone)]
pub struct TradeUpdate {
    pub symbol_id: u16,
    pub at_bid_or_ask: u8, // 0=unknown, 1=at_bid, 2=at_ask
    pub price: f64,
    pub volume: f64,
    pub date_time: f64, // Unix timestamp as double
}

impl TradeUpdate {
    /// Parse full trade update (with doubles)
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 26 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let symbol_id = cursor.get_u16_le();
        let at_bid_or_ask = cursor.get_u8();
        let _ = cursor.get_u8(); // padding
        let price = cursor.get_f64_le();
        let volume = cursor.get_f64_le();
        let date_time = cursor.get_f64_le();

        Some(Self {
            symbol_id,
            at_bid_or_ask,
            price,
            volume,
            date_time,
        })
    }

    /// Parse compact trade update (with floats)
    pub fn parse_compact(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 14 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let symbol_id = cursor.get_u16_le();
        let at_bid_or_ask = cursor.get_u8();
        let _ = cursor.get_u8(); // padding
        let price = cursor.get_f32_le() as f64;
        let volume = cursor.get_f32_le() as f64;
        let date_time = cursor.get_u32_le() as f64;

        Some(Self {
            symbol_id,
            at_bid_or_ask,
            price,
            volume,
            date_time,
        })
    }
}

/// Bid/Ask update
#[derive(Debug, Clone)]
pub struct BidAskUpdate {
    pub symbol_id: u16,
    pub bid_price: f64,
    pub bid_quantity: f64,
    pub ask_price: f64,
    pub ask_quantity: f64,
    pub date_time: f64,
}

impl BidAskUpdate {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 34 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let symbol_id = cursor.get_u16_le();
        let _ = cursor.get_u16_le(); // padding
        let bid_price = cursor.get_f64_le();
        let bid_quantity = cursor.get_f64_le();
        let ask_price = cursor.get_f64_le();
        let ask_quantity = cursor.get_f64_le();
        let date_time = cursor.get_f64_le();

        Some(Self {
            symbol_id,
            bid_price,
            bid_quantity,
            ask_price,
            ask_quantity,
            date_time,
        })
    }

    pub fn parse_compact(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 18 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let symbol_id = cursor.get_u16_le();
        let bid_price = cursor.get_f32_le() as f64;
        let bid_quantity = cursor.get_f32_le() as f64;
        let ask_price = cursor.get_f32_le() as f64;
        let ask_quantity = cursor.get_f32_le() as f64;
        let date_time = cursor.get_u32_le() as f64;

        Some(Self {
            symbol_id,
            bid_price,
            bid_quantity,
            ask_price,
            ask_quantity,
            date_time,
        })
    }
}

/// Session high update
#[derive(Debug, Clone)]
pub struct SessionHighUpdate {
    pub symbol_id: u16,
    pub price: f64,
}

impl SessionHighUpdate {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 10 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let symbol_id = cursor.get_u16_le();
        let _ = cursor.get_u16_le(); // padding
        let price = cursor.get_f64_le();

        Some(Self { symbol_id, price })
    }
}

/// Session low update
#[derive(Debug, Clone)]
pub struct SessionLowUpdate {
    pub symbol_id: u16,
    pub price: f64,
}

impl SessionLowUpdate {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 10 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let symbol_id = cursor.get_u16_le();
        let _ = cursor.get_u16_le(); // padding
        let price = cursor.get_f64_le();

        Some(Self { symbol_id, price })
    }
}

/// Session volume update
#[derive(Debug, Clone)]
pub struct SessionVolumeUpdate {
    pub symbol_id: u16,
    pub volume: f64,
}

impl SessionVolumeUpdate {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 10 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let symbol_id = cursor.get_u16_le();
        let _ = cursor.get_u16_le(); // padding
        let volume = cursor.get_f64_le();

        Some(Self { symbol_id, volume })
    }
}

/// Historical data interval enum
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoricalDataInterval {
    Tick = 0,
    OneSecond = 1,
    TwoSeconds = 2,
    FourSeconds = 4,
    FiveSeconds = 5,
    TenSeconds = 10,
    FifteenSeconds = 15,
    ThirtySeconds = 30,
    OneMinute = 60,
    TwoMinutes = 120,
    ThreeMinutes = 180,
    FourMinutes = 240,
    FiveMinutes = 300,
    TenMinutes = 600,
    FifteenMinutes = 900,
    ThirtyMinutes = 1800,
    OneHour = 3600,
    TwoHours = 7200,
    FourHours = 14400,
    OneDay = 86400,
    OneWeek = 604800,
}

impl HistoricalDataInterval {
    /// Convert from seconds value
    pub fn from_seconds(seconds: i32) -> Option<Self> {
        match seconds {
            0 => Some(Self::Tick),
            1 => Some(Self::OneSecond),
            2 => Some(Self::TwoSeconds),
            4 => Some(Self::FourSeconds),
            5 => Some(Self::FiveSeconds),
            10 => Some(Self::TenSeconds),
            15 => Some(Self::FifteenSeconds),
            30 => Some(Self::ThirtySeconds),
            60 => Some(Self::OneMinute),
            120 => Some(Self::TwoMinutes),
            180 => Some(Self::ThreeMinutes),
            240 => Some(Self::FourMinutes),
            300 => Some(Self::FiveMinutes),
            600 => Some(Self::TenMinutes),
            900 => Some(Self::FifteenMinutes),
            1800 => Some(Self::ThirtyMinutes),
            3600 => Some(Self::OneHour),
            7200 => Some(Self::TwoHours),
            14400 => Some(Self::FourHours),
            86400 => Some(Self::OneDay),
            604800 => Some(Self::OneWeek),
            _ => None,
        }
    }
}

/// Historical price data request
#[derive(Debug, Clone)]
pub struct HistoricalPriceDataRequest {
    pub request_id: i32,
    pub symbol: String,
    pub exchange: String,
    pub record_interval: HistoricalDataInterval,
    pub start_date_time: f64, // 0 = earliest available
    pub end_date_time: f64,   // 0 = latest available
    pub max_days_to_return: u32,
    pub use_zlib_compression: u8,
    pub request_dividend_adjusted_stock_data: u8,
    pub integer_1: u16,
}

impl HistoricalPriceDataRequest {
    /// Create a new historical data request
    pub fn new(
        request_id: i32,
        symbol: &str,
        exchange: &str,
        interval: HistoricalDataInterval,
        max_days: u32,
    ) -> Self {
        Self {
            request_id,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            record_interval: interval,
            start_date_time: 0.0, // Earliest available
            end_date_time: 0.0,   // Latest available
            max_days_to_return: max_days,
            use_zlib_compression: 0,
            request_dividend_adjusted_stock_data: 0,
            integer_1: 0,
        }
    }
}

/// Historical price data response header
#[derive(Debug, Clone)]
pub struct HistoricalPriceDataResponseHeader {
    pub request_id: i32,
    pub record_interval: HistoricalDataInterval,
    pub use_zlib_compression: u8,
    pub no_records_to_return: u8,
    pub int_to_float_price_divisor: f32,
}

impl HistoricalPriceDataResponseHeader {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 14 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let request_id = cursor.get_i32_le();
        let interval_value = cursor.get_i32_le();
        let record_interval = HistoricalDataInterval::from_seconds(interval_value)?;
        let use_zlib_compression = cursor.get_u8();
        let no_records_to_return = cursor.get_u8();
        let int_to_float_price_divisor = cursor.get_f32_le();

        Some(Self {
            request_id,
            record_interval,
            use_zlib_compression,
            no_records_to_return,
            int_to_float_price_divisor,
        })
    }
}

/// Historical price data record response (OHLCV bar)
#[derive(Debug, Clone)]
pub struct HistoricalPriceDataRecordResponse {
    pub request_id: i32,
    pub start_date_time: f64,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub last_price: f64, // Close price
    pub volume: f64,
    pub num_trades: u32,
    pub bid_volume: f64,
    pub ask_volume: f64,
    pub is_final_record: u8,
}

impl HistoricalPriceDataRecordResponse {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 57 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let request_id = cursor.get_i32_le();
        let start_date_time = cursor.get_f64_le();
        let open_price = cursor.get_f64_le();
        let high_price = cursor.get_f64_le();
        let low_price = cursor.get_f64_le();
        let last_price = cursor.get_f64_le();
        let volume = cursor.get_f64_le();
        let num_trades = cursor.get_u32_le();
        let bid_volume = cursor.get_f64_le();
        let ask_volume = cursor.get_f64_le();
        let is_final_record = cursor.get_u8();

        Some(Self {
            request_id,
            start_date_time,
            open_price,
            high_price,
            low_price,
            last_price,
            volume,
            num_trades,
            bid_volume,
            ask_volume,
            is_final_record,
        })
    }
}

/// Historical price data reject
#[derive(Debug, Clone)]
pub struct HistoricalPriceDataReject {
    pub request_id: i32,
    pub reject_text: String,
    pub reject_reason_code: u16,
}

impl HistoricalPriceDataReject {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < MessageHeader::SIZE + 6 {
            return None;
        }
        let body = &data[MessageHeader::SIZE..];
        let mut cursor = Cursor::new(body);

        let request_id = cursor.get_i32_le();
        let reject_reason_code = cursor.get_u16_le();

        let reject_text = if body.len() > 6 {
            String::from_utf8_lossy(&body[6..])
                .trim_end_matches('\0')
                .to_string()
        } else {
            String::new()
        };

        Some(Self {
            request_id,
            reject_text,
            reject_reason_code,
        })
    }
}

/// Parsed DTC message
#[derive(Debug)]
pub enum DtcMessage {
    EncodingResponse(EncodingResponse),
    LogonResponse(LogonResponse),
    Heartbeat(Heartbeat),
    MarketDataReject(MarketDataReject),
    MarketDataSnapshot(MarketDataSnapshot),
    TradeUpdate(TradeUpdate),
    BidAskUpdate(BidAskUpdate),
    SessionHighUpdate(SessionHighUpdate),
    SessionLowUpdate(SessionLowUpdate),
    SessionVolumeUpdate(SessionVolumeUpdate),
    HistoricalPriceDataResponseHeader(HistoricalPriceDataResponseHeader),
    HistoricalPriceDataRecordResponse(HistoricalPriceDataRecordResponse),
    HistoricalPriceDataReject(HistoricalPriceDataReject),
    Unknown { msg_type: u16, size: u16 },
}

/// Parse a complete DTC message from a buffer
pub fn parse_message(data: &[u8]) -> Option<(DtcMessage, usize)> {
    let header = MessageHeader::parse(data)?;

    if data.len() < header.size as usize {
        return None; // Incomplete message
    }

    let msg_data = &data[..header.size as usize];
    let msg_type = MessageType::from(header.msg_type);

    let message = match msg_type {
        MessageType::EncodingResponse => {
            EncodingResponse::parse(msg_data).map(DtcMessage::EncodingResponse)
        }
        MessageType::LogonResponse => LogonResponse::parse(msg_data).map(DtcMessage::LogonResponse),
        MessageType::Heartbeat => Heartbeat::parse(msg_data).map(DtcMessage::Heartbeat),
        MessageType::MarketDataReject => {
            MarketDataReject::parse(msg_data).map(DtcMessage::MarketDataReject)
        }
        MessageType::MarketDataSnapshot => {
            MarketDataSnapshot::parse(msg_data).map(DtcMessage::MarketDataSnapshot)
        }
        MessageType::MarketDataUpdateTrade => {
            TradeUpdate::parse(msg_data).map(DtcMessage::TradeUpdate)
        }
        MessageType::MarketDataUpdateTradeCompact => {
            TradeUpdate::parse_compact(msg_data).map(DtcMessage::TradeUpdate)
        }
        MessageType::MarketDataUpdateBidAsk => {
            BidAskUpdate::parse(msg_data).map(DtcMessage::BidAskUpdate)
        }
        MessageType::MarketDataUpdateBidAskCompact
        | MessageType::MarketDataUpdateBidAskNoTimestamp => {
            BidAskUpdate::parse_compact(msg_data).map(DtcMessage::BidAskUpdate)
        }
        MessageType::MarketDataUpdateSessionHigh => {
            SessionHighUpdate::parse(msg_data).map(DtcMessage::SessionHighUpdate)
        }
        MessageType::MarketDataUpdateSessionLow => {
            SessionLowUpdate::parse(msg_data).map(DtcMessage::SessionLowUpdate)
        }
        MessageType::MarketDataUpdateSessionVolume => {
            SessionVolumeUpdate::parse(msg_data).map(DtcMessage::SessionVolumeUpdate)
        }
        MessageType::HistoricalPriceDataResponseHeader => {
            HistoricalPriceDataResponseHeader::parse(msg_data)
                .map(DtcMessage::HistoricalPriceDataResponseHeader)
        }
        MessageType::HistoricalPriceDataRecordResponse => {
            HistoricalPriceDataRecordResponse::parse(msg_data)
                .map(DtcMessage::HistoricalPriceDataRecordResponse)
        }
        MessageType::HistoricalPriceDataReject => {
            HistoricalPriceDataReject::parse(msg_data).map(DtcMessage::HistoricalPriceDataReject)
        }
        _ => Some(DtcMessage::Unknown {
            msg_type: header.msg_type,
            size: header.size,
        }),
    };

    message.map(|m| (m, header.size as usize))
}
