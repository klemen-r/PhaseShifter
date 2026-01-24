//! SCID File Reader - Streaming version for backtest
//!
//! Reads Sierra Chart's Intraday Data (.scid) files with memory-mapped streaming.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{bail, Context, Result};
use tracing::{debug, info};

use crate::{EnhancedTick, Tick};

/// SCID file header (56 bytes)
#[derive(Debug, Clone)]
pub struct ScidHeader {
    pub file_type_id: [u8; 4],
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
#[derive(Debug, Clone, Copy)]
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

    /// Convert SCID timestamp to Unix milliseconds
    pub fn to_unix_millis(&self) -> i64 {
        const DAYS_OFFSET: i64 = 25569;
        const MICROS_PER_DAY: i64 = 86_400_000_000;

        let unix_micros = self.date_time - (DAYS_OFFSET * MICROS_PER_DAY);
        unix_micros / 1000
    }

    /// Check if this is a tick record (not a bar)
    pub fn is_tick(&self) -> bool {
        self.open == 0.0 || self.open < -1e30
    }

    /// Check if this is a valid trade tick (has trade price)
    pub fn is_valid_tick(&self) -> bool {
        self.is_tick() && self.close.is_finite() && self.close > 0.0
    }

    /// Convert to Tick struct (basic version)
    pub fn to_tick(&self) -> Tick {
        Tick {
            timestamp_ms: self.to_unix_millis(),
            price: self.close as f64,
            volume: self.total_volume,
            bid: self.low as f64,
            ask: self.high as f64,
        }
    }

    /// Convert to EnhancedTick struct (preserves all fields for feature extraction)
    pub fn to_enhanced_tick(&self) -> EnhancedTick {
        EnhancedTick {
            timestamp_ms: self.to_unix_millis(),
            price: self.close as f64,
            volume: self.total_volume,
            bid_volume: self.bid_volume,
            ask_volume: self.ask_volume,
            num_trades: self.num_trades,
            bid: self.low as f64,
            ask: self.high as f64,
        }
    }
}

/// Streaming SCID file reader
pub struct ScidReader {
    reader: BufReader<File>,
    header: ScidHeader,
    record_count: u64,
    current_record: u64,
    path: String,
}

impl ScidReader {
    /// Open an SCID file for reading
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let path_str = path_ref.display().to_string();

        let file = File::open(path_ref)
            .with_context(|| format!("Failed to open SCID file: {}", path_str))?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        let mut reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer
        let header = ScidHeader::read(&mut reader)?;

        let data_size = file_size - header.header_size as u64;
        let record_count = data_size / header.record_size as u64;

        debug!(
            "Opened SCID file: {}, version={}, records={}",
            path_str, header.version, record_count
        );

        Ok(Self {
            reader,
            header,
            record_count,
            current_record: 0,
            path: path_str,
        })
    }

    /// Get total record count
    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Get file path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Read next record
    pub fn next_record(&mut self) -> Result<Option<ScidRecord>> {
        if self.current_record >= self.record_count {
            return Ok(None);
        }

        let record = ScidRecord::read(&mut self.reader)?;
        self.current_record += 1;
        Ok(Some(record))
    }

    /// Seek to specific record index
    pub fn seek_to(&mut self, index: u64) -> Result<()> {
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

    /// Current position
    pub fn position(&self) -> u64 {
        self.current_record
    }

    /// Reset to beginning
    pub fn reset(&mut self) -> Result<()> {
        self.seek_to(0)
    }
}

/// Iterator over valid ticks in an SCID file
pub struct TickIterator {
    reader: ScidReader,
}

impl TickIterator {
    pub fn new(reader: ScidReader) -> Self {
        Self { reader }
    }
}

impl Iterator for TickIterator {
    type Item = Result<Tick>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.reader.next_record() {
                Ok(Some(record)) => {
                    if record.is_valid_tick() {
                        return Some(Ok(record.to_tick()));
                    }
                    // Skip non-tick records
                    continue;
                }
                Ok(None) => return None,
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

/// Load multiple SCID files and chain them together
pub struct MultiScidReader {
    paths: Vec<String>,
    current_index: usize,
    current_reader: Option<ScidReader>,
    total_records: u64,
}

impl MultiScidReader {
    /// Create from a list of SCID file paths
    pub fn new(paths: Vec<String>) -> Result<Self> {
        let mut total_records = 0u64;

        // Count total records across all files
        for path in &paths {
            let reader = ScidReader::open(path)?;
            total_records += reader.record_count();
        }

        info!(
            "MultiScidReader: {} files, {} total records",
            paths.len(),
            total_records
        );

        Ok(Self {
            paths,
            current_index: 0,
            current_reader: None,
            total_records,
        })
    }

    /// Get total record count across all files
    pub fn total_records(&self) -> u64 {
        self.total_records
    }

    /// Get next tick from the chain of files
    pub fn next_tick(&mut self) -> Result<Option<Tick>> {
        loop {
            // Initialize reader if needed
            if self.current_reader.is_none() {
                if self.current_index >= self.paths.len() {
                    return Ok(None);
                }
                self.current_reader = Some(ScidReader::open(&self.paths[self.current_index])?);
            }

            // Try to get next record from current reader
            if let Some(reader) = &mut self.current_reader {
                match reader.next_record()? {
                    Some(record) => {
                        if record.is_valid_tick() {
                            return Ok(Some(record.to_tick()));
                        }
                        continue;
                    }
                    None => {
                        // Current file exhausted, move to next
                        self.current_reader = None;
                        self.current_index += 1;
                        continue;
                    }
                }
            }
        }
    }
}

/// Find all SCID files matching a symbol prefix in a directory, sorted by date
pub fn find_scid_files(data_dir: &Path, symbol_prefix: &str) -> Result<Vec<String>> {
    let mut files: Vec<String> = Vec::new();
    let prefix_len = symbol_prefix.len();

    for entry in std::fs::read_dir(data_dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Match contract patterns: {PREFIX}H24-CME.scid, {PREFIX}M25-CME.scid, etc.
            if name.starts_with(symbol_prefix)
                && name.ends_with("-CME.scid")
                && name.len() > prefix_len + 7
                && name
                    .chars()
                    .nth(prefix_len + 1)
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            {
                files.push(path.display().to_string());
            }
        }
    }

    // Sort by contract date (extract year/month from name)
    let prefix_len_copy = prefix_len;
    files.sort_by(|a, b| {
        let extract_date = |s: &str| -> (i32, i32) {
            let name = Path::new(s)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if name.len() < prefix_len_copy + 3 {
                return (0, 0);
            }

            let month_code = name.chars().nth(prefix_len_copy).unwrap_or('X');
            let month = match month_code {
                'H' => 3,  // March
                'M' => 6,  // June
                'U' => 9,  // September
                'Z' => 12, // December
                _ => 0,
            };

            let year_start = prefix_len_copy + 1;
            let year: i32 = name[year_start..year_start + 2].parse().unwrap_or(0);
            let full_year = if year >= 5 { 2000 + year } else { 2100 + year };

            (full_year, month)
        };

        extract_date(a).cmp(&extract_date(b))
    });

    info!("Found {} {} SCID files", files.len(), symbol_prefix);
    for f in &files {
        debug!("  {}", f);
    }

    Ok(files)
}

/// Find all NQ SCID files in a directory, sorted by date
pub fn find_nq_scid_files(data_dir: &Path) -> Result<Vec<String>> {
    find_scid_files(data_dir, "NQ")
}

/// Find all MNQ SCID files in a directory, sorted by date
pub fn find_mnq_scid_files(data_dir: &Path) -> Result<Vec<String>> {
    find_scid_files(data_dir, "MNQ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_conversion() {
        // SCID uses microseconds since 1899-12-30
        // 2024-01-01 00:00:00 UTC = 1704067200 seconds since 1970-01-01
        // Days from 1899-12-30 to 1970-01-01 = 25569 days
        // Days from 1899-12-30 to 2024-01-01 = 25569 + (54 years worth) = ~45292 days
        // In microseconds: 45292 * 86400 * 1_000_000 = 3913228800000000
        let record = ScidRecord {
            date_time: 3913228800000000, // 2024-01-01 00:00:00 UTC
            open: 0.0,
            high: 100.0,
            low: 99.0,
            close: 99.5,
            num_trades: 1,
            total_volume: 10,
            bid_volume: 5,
            ask_volume: 5,
        };

        let ms = record.to_unix_millis();
        // Should be around 2024-01-01 00:00:00 UTC = 1704067200000 ms
        assert!((ms - 1704067200000).abs() < 1000, "Got {} ms", ms);
    }
}
