//! SCID File Reader
//!
//! Reads Sierra Chart's Intraday Data (.scid) files directly from disk.
//! This bypasses DTC protocol restrictions for CME data.
//!
//! File Format:
//! - 56-byte header (s_IntradayHeader)
//! - Sequential 40-byte records (s_IntradayRecord)

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{bail, Context, Result};
use tracing::{debug, info};

/// SCID file header (56 bytes)
#[derive(Debug, Clone)]
pub struct ScidHeader {
    pub file_type_id: [u8; 4], // Must be "SCID"
    pub header_size: u32,
    pub record_size: u32,
    pub version: u16,
}

impl ScidHeader {
    pub const SIZE: usize = 56;

    pub fn read(reader: &mut impl Read) -> Result<Self> {
        let mut buf = [0u8; Self::SIZE];
        reader.read_exact(&mut buf)?;

        let file_type_id: [u8; 4] = buf[0..4].try_into()?;
        if &file_type_id != b"SCID" {
            bail!(
                "Invalid SCID file: expected 'SCID' header, got {:?}",
                file_type_id
            );
        }

        let header_size = u32::from_le_bytes(buf[4..8].try_into()?);
        let record_size = u32::from_le_bytes(buf[8..12].try_into()?);
        let version = u16::from_le_bytes(buf[12..14].try_into()?);

        Ok(Self {
            file_type_id,
            header_size,
            record_size,
            version,
        })
    }
}

/// SCID intraday record (40 bytes)
#[derive(Debug, Clone)]
pub struct ScidRecord {
    /// Microseconds since December 30, 1899 (UTC)
    pub date_time: i64,
    pub open: f32,
    pub high: f32,
    pub low: f32,
    pub close: f32,
    pub num_trades: u32,
    pub total_volume: u32,
    pub bid_volume: u32,
    pub ask_volume: u32,
}

impl ScidRecord {
    pub const SIZE: usize = 40;

    /// Read a single record from the buffer
    pub fn read(reader: &mut impl Read) -> Result<Self> {
        let mut buf = [0u8; Self::SIZE];
        reader.read_exact(&mut buf)?;

        let date_time = i64::from_le_bytes(buf[0..8].try_into()?);
        let open = f32::from_le_bytes(buf[8..12].try_into()?);
        let high = f32::from_le_bytes(buf[12..16].try_into()?);
        let low = f32::from_le_bytes(buf[16..20].try_into()?);
        let close = f32::from_le_bytes(buf[20..24].try_into()?);
        let num_trades = u32::from_le_bytes(buf[24..28].try_into()?);
        let total_volume = u32::from_le_bytes(buf[28..32].try_into()?);
        let bid_volume = u32::from_le_bytes(buf[32..36].try_into()?);
        let ask_volume = u32::from_le_bytes(buf[36..40].try_into()?);

        Ok(Self {
            date_time,
            open,
            high,
            low,
            close,
            num_trades,
            total_volume,
            bid_volume,
            ask_volume,
        })
    }

    /// Convert SCID timestamp (microseconds since 1899-12-30) to Unix timestamp (seconds)
    pub fn to_unix_timestamp(&self) -> f64 {
        // Days from 1899-12-30 to 1970-01-01 = 25569 days
        const DAYS_OFFSET: i64 = 25569;
        const MICROS_PER_DAY: i64 = 86_400_000_000;
        const MICROS_PER_SECOND: f64 = 1_000_000.0;

        let unix_micros = self.date_time - (DAYS_OFFSET * MICROS_PER_DAY);
        unix_micros as f64 / MICROS_PER_SECOND
    }

    /// Convert to Unix timestamp in milliseconds (for JavaScript)
    pub fn to_unix_millis(&self) -> i64 {
        (self.to_unix_timestamp() * 1000.0) as i64
    }

    /// Check if this is a single trade/tick record (Open == 0.0)
    /// In tick mode: Open=0, High=AskPrice, Low=BidPrice, Close=LastTradePrice
    pub fn is_single_trade(&self) -> bool {
        self.open == 0.0
    }

    /// Get the trade price from a tick record
    /// For tick data: close contains the last trade price
    pub fn tick_price(&self) -> f32 {
        self.close
    }

    /// Get the bid price from a tick record
    /// For tick data: low contains the bid price
    pub fn tick_bid(&self) -> f32 {
        self.low
    }

    /// Get the ask price from a tick record
    /// For tick data: high contains the ask price
    pub fn tick_ask(&self) -> f32 {
        self.high
    }

    /// Check if record has valid OHLC values for a bar (not NaN, not infinite, reasonable range)
    pub fn is_valid_bar(&self) -> bool {
        // Must not be a tick record
        if self.is_single_trade() {
            return false;
        }

        // Check for NaN or infinity
        if !self.open.is_finite()
            || !self.high.is_finite()
            || !self.low.is_finite()
            || !self.close.is_finite()
        {
            return false;
        }

        // Check for zero or negative prices (invalid for futures)
        if self.open <= 0.0 || self.high <= 0.0 || self.low <= 0.0 || self.close <= 0.0 {
            return false;
        }

        // Check OHLC relationship: high >= low, high >= open/close, low <= open/close
        if self.high < self.low {
            return false;
        }
        if self.high < self.open || self.high < self.close {
            return false;
        }
        if self.low > self.open || self.low > self.close {
            return false;
        }

        true
    }

    /// Check if this is a valid tick record
    /// Sierra Chart stores ticks with open=0, close=trade price
    /// Bid/ask only updates have close=0 (no trade), we skip those
    pub fn is_valid_tick(&self) -> bool {
        if !self.is_single_trade() {
            return false;
        }

        // For tick records, close is the trade price
        // close=0 means this is a bid/ask update without a trade - skip it
        if !self.close.is_finite() || self.close <= 0.0 {
            return false;
        }

        true
    }

    /// Check if this is a bid/ask only tick (no trade)
    /// These have open=0 and close=0, but may have valid high (ask) and low (bid)
    pub fn is_bid_ask_only(&self) -> bool {
        self.is_single_trade() && self.close == 0.0
    }
}

/// SCID file reader
pub struct ScidReader {
    reader: BufReader<File>,
    header: ScidHeader,
    record_count: u64,
    current_record: u64,
}

impl ScidReader {
    /// Open an SCID file for reading
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file =
            File::open(path).with_context(|| format!("Failed to open SCID file: {:?}", path))?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        let mut reader = BufReader::new(file);
        let header = ScidHeader::read(&mut reader)?;

        // Calculate number of records
        let data_size = file_size - header.header_size as u64;
        let record_count = data_size / header.record_size as u64;

        debug!(
            "Opened SCID file: {:?}, version={}, record_size={}, records={}",
            path, header.version, header.record_size, record_count
        );

        Ok(Self {
            reader,
            header,
            record_count,
            current_record: 0,
        })
    }

    /// Get the total number of records in the file
    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Read the next record
    pub fn read_record(&mut self) -> Result<Option<ScidRecord>> {
        if self.current_record >= self.record_count {
            return Ok(None);
        }

        let record = ScidRecord::read(&mut self.reader)?;
        self.current_record += 1;
        Ok(Some(record))
    }

    /// Seek to a specific record index
    pub fn seek_to_record(&mut self, index: u64) -> Result<()> {
        if index >= self.record_count {
            bail!(
                "Record index {} out of range (max {})",
                index,
                self.record_count
            );
        }

        let offset = self.header.header_size as u64 + (index * self.header.record_size as u64);
        self.reader.seek(SeekFrom::Start(offset))?;
        self.current_record = index;
        Ok(())
    }

    /// Read the last N records from the file
    pub fn read_last_n_records(&mut self, n: u64) -> Result<Vec<ScidRecord>> {
        let start_index = if self.record_count > n {
            self.record_count - n
        } else {
            0
        };

        self.seek_to_record(start_index)?;

        let mut records = Vec::with_capacity(n as usize);
        while let Some(record) = self.read_record()? {
            records.push(record);
        }

        Ok(records)
    }

    /// Read records from a specific Unix timestamp onwards
    pub fn read_records_since(&mut self, since_unix_timestamp: f64) -> Result<Vec<ScidRecord>> {
        // Binary search to find the starting record
        let start_index = self.binary_search_timestamp(since_unix_timestamp)?;
        self.seek_to_record(start_index)?;

        let mut records = Vec::new();
        while let Some(record) = self.read_record()? {
            records.push(record);
        }

        Ok(records)
    }

    /// Binary search to find the first record at or after a given timestamp
    fn binary_search_timestamp(&mut self, target_unix_timestamp: f64) -> Result<u64> {
        if self.record_count == 0 {
            return Ok(0);
        }

        let mut low = 0u64;
        let mut high = self.record_count - 1;

        while low < high {
            let mid = low + (high - low) / 2;
            self.seek_to_record(mid)?;

            if let Some(record) = self.read_record()? {
                if record.to_unix_timestamp() < target_unix_timestamp {
                    low = mid + 1;
                } else {
                    high = mid;
                }
            } else {
                break;
            }
        }

        Ok(low)
    }
}

/// Load historical bars from an SCID file
pub fn load_scid_bars(scid_path: &Path, max_bars: Option<usize>) -> Result<Vec<ScidRecord>> {
    let mut reader = ScidReader::open(scid_path)?;

    let records = if let Some(max) = max_bars {
        reader.read_last_n_records(max as u64)?
    } else {
        let mut records = Vec::new();
        while let Some(record) = reader.read_record()? {
            records.push(record);
        }
        records
    };

    info!("Loaded {} records from {:?}", records.len(), scid_path);

    Ok(records)
}

/// OHLCV bar built from SCID records
#[derive(Debug, Clone)]
pub struct ScidBar {
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub tick_count: u32,
}

/// Build an M1 bar from SCID records for a specific minute
///
/// - `scid_path`: Path to the SCID file
/// - `bar_timestamp_ms`: The bar's opening timestamp in milliseconds (aligned to minute boundary)
///
/// Returns None if no ticks found for that minute
pub fn build_m1_bar_from_scid(scid_path: &Path, bar_timestamp_ms: i64) -> Result<Option<ScidBar>> {
    let mut reader = ScidReader::open(scid_path)?;

    // Convert bar timestamp to Unix seconds for search
    let bar_start_sec = bar_timestamp_ms as f64 / 1000.0;
    let bar_end_sec = bar_start_sec + 60.0;

    // Find records starting from bar_start
    let start_index = reader.binary_search_timestamp(bar_start_sec)?;
    reader.seek_to_record(start_index)?;

    let mut bar: Option<ScidBar> = None;

    while let Some(record) = reader.read_record()? {
        let record_ts = record.to_unix_timestamp();

        // Stop if we've passed the bar's end time
        if record_ts >= bar_end_sec {
            break;
        }

        // Skip records before bar start
        if record_ts < bar_start_sec {
            continue;
        }

        // Only process valid tick records
        if !record.is_valid_tick() {
            continue;
        }

        let price = record.tick_price() as f64;
        let volume = record.total_volume as f64;

        match &mut bar {
            Some(b) => {
                b.high = b.high.max(price);
                b.low = b.low.min(price);
                b.close = price;
                b.volume += volume;
                b.tick_count += 1;
            }
            None => {
                bar = Some(ScidBar {
                    timestamp_ms: bar_timestamp_ms,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume,
                    tick_count: 1,
                });
            }
        }
    }

    Ok(bar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_conversion() {
        // Create a test record with a known timestamp
        let record = ScidRecord {
            // 2024-01-01 00:00:00 UTC
            // Days from 1899-12-30 to 2024-01-01 = 45293 days
            // 45293 * 86400 * 1000000 = 3913315200000000 microseconds
            date_time: 3913315200000000,
            open: 100.0,
            high: 105.0,
            low: 99.0,
            close: 103.0,
            num_trades: 10,
            total_volume: 1000,
            bid_volume: 500,
            ask_volume: 500,
        };

        let unix_ts = record.to_unix_timestamp();
        // 2024-01-01 00:00:00 UTC = 1704067200
        assert!((unix_ts - 1704067200.0).abs() < 1.0);
    }
}
