use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use csv::DeserializeRecordsIntoIter;
use serde::Deserialize;

use crate::{Candle, CandleSource};

#[derive(Debug, Deserialize)]
struct CsvRow {
    // expected columns; aliases cover common header variants
    #[serde(alias = "Timestamp", alias = "Datetime", alias = "timestamp")]
    timestamp: Option<String>,
    #[serde(alias = "Open", alias = "open")]
    open: Option<f64>,
    #[serde(alias = "High", alias = "high")]
    high: Option<f64>,
    #[serde(alias = "Low", alias = "low")]
    low: Option<f64>,
    #[serde(alias = "Close", alias = "close")]
    close: Option<f64>,
    #[serde(alias = "Volume", alias = "volume")]
    volume: Option<f64>,
}

pub struct CsvCandleSource {
    path: PathBuf,
    reader: csv::Reader<File>,
}

impl CsvCandleSource {
    pub fn new(path: PathBuf) -> Result<Self> {
        let reader = csv::Reader::from_path(&path)
            .with_context(|| format!("Failed to open CSV at {}", path.display()))?;
        Ok(Self { path, reader })
    }
}

pub struct CsvCandleIter {
    iter: DeserializeRecordsIntoIter<File, CsvRow>,
    path: PathBuf,
    line: usize, // track line numbers for better errors
}

impl CandleSource for CsvCandleSource {
    type Error = anyhow::Error;
    type IntoIter = CsvCandleIter;

    fn into_iter(self) -> Self::IntoIter {
        CsvCandleIter {
            iter: self.reader.into_deserialize(),
            path: self.path,
            line: 1, // start after header row
        }
    }
}

impl Iterator for CsvCandleIter {
    type Item = Result<Candle>;

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.iter.next()?;
        self.line += 1;
        // translate serde row into Candle with context-rich errors
        Some(match record {
            Ok(row) => row_to_candle(&row, &self.path, self.line),
            Err(err) => Err(anyhow!(err).context(format!(
                "CSV read error at {} line {}",
                self.path.display(),
                self.line
            ))),
        })
    }
}

fn row_to_candle(row: &CsvRow, path: &Path, line: usize) -> Result<Candle> {
    let timestamp_str = row
        .timestamp
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "Missing or empty 'timestamp' field at {} line {}",
                path.display(),
                line
            )
        })?;
    let timestamp_ms = parse_timestamp(timestamp_str, path, line)?;

    let open = row
        .open
        .ok_or_else(|| anyhow!("Missing 'open' field at {} line {}", path.display(), line))?;
    let high = row
        .high
        .ok_or_else(|| anyhow!("Missing 'high' field at {} line {}", path.display(), line))?;
    let low = row
        .low
        .ok_or_else(|| anyhow!("Missing 'low' field at {} line {}", path.display(), line))?;
    let close = row
        .close
        .ok_or_else(|| anyhow!("Missing 'close' field at {} line {}", path.display(), line))?;
    let volume = row
        .volume
        .ok_or_else(|| anyhow!("Missing 'volume' field at {} line {}", path.display(), line))?;

    Ok(Candle {
        timestamp_ms,
        open,
        high,
        low,
        close,
        volume,
    })
}

fn parse_timestamp(value: &str, path: &Path, line: usize) -> Result<i64> {
    let ts_str = value.trim();
    parse_timestamp_inner(ts_str).with_context(|| {
        format!(
            "Failed to parse timestamp '{}' at {} line {}",
            ts_str,
            path.display(),
            line
        )
    })
}

fn parse_timestamp_inner(s: &str) -> Result<i64> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp_millis());
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        let dt = Utc.from_utc_datetime(&naive);
        return Ok(dt.timestamp_millis());
    }

    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow!("Invalid date '{}'", s))?;
        let dt = Utc.from_utc_datetime(&naive);
        return Ok(dt.timestamp_millis());
    }

    Err(anyhow!("Failed to parse timestamp '{}'", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_multiple_timestamp_formats() {
        let base = NaiveDate::from_ymd_opt(2025, 8, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let expected = Utc.from_utc_datetime(&base).timestamp_millis();

        let variants = ["2025-08-15", "2025-08-15 00:00:00", "2025-08-15T00:00:00Z"];

        for ts in variants {
            let parsed = parse_timestamp(ts, Path::new("test.csv"), 1).unwrap();
            assert_eq!(parsed, expected, "failed for {ts}");
        }
    }

    #[test]
    fn rejects_bad_timestamp() {
        let err = parse_timestamp("bad-ts", Path::new("test.csv"), 3)
            .expect_err("expected parse failure");
        let msg = format!("{err}");
        assert!(msg.contains("bad-ts"));
        assert!(msg.contains("test.csv"));
    }

    #[test]
    fn row_to_candle_parses_valid_row() {
        let row = CsvRow {
            timestamp: Some("2025-08-15".to_string()),
            open: Some(1.0),
            high: Some(2.0),
            low: Some(0.5),
            close: Some(1.5),
            volume: Some(10.0),
        };
        let candle = row_to_candle(&row, Path::new("inline.csv"), 2).unwrap();
        let expected_ts = parse_timestamp("2025-08-15", Path::new("inline.csv"), 2).unwrap();
        assert_eq!(candle.timestamp_ms, expected_ts);
        assert_eq!(candle.open, 1.0);
        assert_eq!(candle.high, 2.0);
    }

    #[test]
    fn row_to_candle_reports_missing_field() {
        let row = CsvRow {
            timestamp: Some("2025-08-15".to_string()),
            open: None,
            high: Some(2.0),
            low: Some(0.5),
            close: Some(1.5),
            volume: Some(10.0),
        };
        let err = row_to_candle(&row, Path::new("inline.csv"), 2).expect_err("expected failure");
        let msg = format!("{err}");
        assert!(msg.contains("open"));
        assert!(msg.contains("inline.csv"));
    }
}
