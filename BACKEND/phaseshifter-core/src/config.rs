use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Deserialize;

const DEFAULT_SYMBOL: &str = "NQ=F";
const DEFAULT_CSV_PATH: &str = "../data/nq_1h.csv";
const DEFAULT_PHASE_WINDOW: usize = 7;
const DEFAULT_DEPTH_DAYS: i64 = 50;
const DEFAULT_TIMEFRAME: Option<&str> = Some("1h");
const DEFAULT_WARMUP_CANDLES: usize = 0;
// default JSONL sinks under repo-root/data when user does not pass paths
const DEFAULT_PHASE_UPDATES_PATH: Option<&str> = Some("../data/phase_updates.jsonl");
const DEFAULT_NODE_EVENTS_LOG: Option<&str> = Some("../data/node_events.jsonl");
// built-in fallbacks before file/CLI overrides kick in

#[derive(Debug, Parser)]
#[command(
    name = "phaseshifter-core",
    version,
    about = "CLI runner for the PhaseShifter engine"
)]
pub struct CliArgs {
    #[arg(long, value_name = "SYMBOL", help = "Symbol identifier")]
    pub symbol: Option<String>,

    #[arg(long, value_name = "PATH", help = "Path to input CSV with OHLCV data")]
    pub csv: Option<PathBuf>,

    #[arg(
        long,
        value_name = "N",
        help = "Phase window length for Donchian midpoint"
    )]
    pub phase_window: Option<usize>,

    #[arg(long, value_name = "DAYS", help = "Depth in days (metadata)")]
    pub depth_days: Option<i64>,

    #[arg(long, value_name = "LABEL", help = "Timeframe label metadata")]
    pub timeframe: Option<String>,

    #[arg(long, value_name = "PATH", help = "Path to TOML config file")]
    pub config: Option<PathBuf>,

    #[arg(long, value_name = "N", help = "Warmup candles before emitting DM")]
    pub warmup_candles: Option<usize>,

    #[arg(
        long,
        value_name = "LEVEL",
        help = "Log filter (e.g. error,warn,info,debug,trace)"
    )]
    pub log_level: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Optional path to write phase_update JSONL output"
    )]
    pub out_json: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Optional path to write node event JSONL output"
    )]
    // optional node event log; default can kick in later if unset
    pub node_events_log: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub symbol: String,
    pub csv: PathBuf,
    pub phase_window: usize,
    pub depth_days: i64,
    pub timeframe: Option<String>,
    pub warmup_candles: usize,
    pub log_level: Option<String>,
    pub out_json: Option<PathBuf>,
    pub node_events_log: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    symbol: Option<String>,
    csv: Option<PathBuf>,
    phase_window: Option<usize>,
    depth_days: Option<i64>,
    timeframe: Option<String>,
    warmup_candles: Option<usize>,
    log_level: Option<String>,
    out_json: Option<PathBuf>,
    node_events_log: Option<PathBuf>,
}

/// Try to infer a symbol from an input CSV path.
pub fn infer_symbol_from_path<P: AsRef<Path>>(path: P) -> Option<String> {
    let stem = path.as_ref().file_stem()?.to_string_lossy();
    let first_segment = stem.split('_').next()?.trim();

    if first_segment.is_empty() {
        return None;
    }

    let mapped = match first_segment.to_ascii_lowercase().as_str() {
        "nq" => "NQ=F".to_string(),
        "es" => "ES=F".to_string(),
        _ => first_segment.to_ascii_uppercase(),
    };

    Some(mapped)
}

pub fn build_runtime_config(args: &CliArgs) -> Result<RuntimeConfig> {
    // layer defaults -> file -> CLI
    let file_cfg = load_file_config(args.config.as_deref())?;

    let csv = pick(
        args.csv.clone(),
        file_cfg.csv.clone(),
        PathBuf::from(DEFAULT_CSV_PATH),
    );

    let inferred_symbol = infer_symbol_from_path(&csv);

    let symbol = args
        .symbol
        .clone()
        .or(file_cfg.symbol.clone())
        .or(inferred_symbol)
        .unwrap_or_else(|| DEFAULT_SYMBOL.to_string());
    let phase_window = pick(
        args.phase_window,
        file_cfg.phase_window,
        DEFAULT_PHASE_WINDOW,
    );
    let depth_days = pick(args.depth_days, file_cfg.depth_days, DEFAULT_DEPTH_DAYS);
    let timeframe = pick_optional_string(args.timeframe.clone(), file_cfg.timeframe.clone());
    let warmup_candles = pick(
        args.warmup_candles,
        file_cfg.warmup_candles,
        DEFAULT_WARMUP_CANDLES,
    );
    let log_level = args.log_level.clone().or(file_cfg.log_level.clone());
    let out_json = args
        .out_json
        .clone()
        .or(file_cfg.out_json.clone())
        .or_else(default_phase_updates_path);
    let node_events_log = args
        .node_events_log
        .clone()
        .or(file_cfg.node_events_log.clone())
        .or_else(default_node_events_log);

    validate_phase_window(phase_window)?;
    validate_depth_days(depth_days)?;
    validate_csv_path(&csv)?;
    validate_output_path(out_json.as_deref())?;
    validate_output_path(node_events_log.as_deref())?;

    Ok(RuntimeConfig {
        symbol,
        csv,
        phase_window,
        depth_days,
        timeframe,
        warmup_candles,
        log_level,
        out_json,
        node_events_log,
    })
}

fn load_file_config(path: Option<&Path>) -> Result<FileConfig> {
    let Some(path) = path else {
        return Ok(FileConfig::default());
    };

    // optional TOML file for defaults
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file at {}", path.display()))?;
    let parsed: FileConfig = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse TOML config at {}", path.display()))?;
    Ok(parsed)
}

fn validate_phase_window(value: usize) -> Result<()> {
    if value == 0 {
        bail!("phase_window must be greater than zero");
    }
    Ok(())
}

fn validate_depth_days(value: i64) -> Result<()> {
    if value <= 0 {
        bail!("depth_days must be positive");
    }
    Ok(())
}

fn validate_csv_path(path: &Path) -> Result<()> {
    // catch bad paths early
    let metadata = fs::metadata(path).with_context(|| {
        format!(
            "CSV file '{}' does not exist or is not accessible",
            path.display()
        )
    })?;

    if !metadata.is_file() {
        bail!("CSV path '{}' is not a file", path.display());
    }

    Ok(())
}

fn validate_output_path(path: Option<&Path>) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            bail!("output directory '{}' does not exist", parent.display());
        }
        if !parent.as_os_str().is_empty() && parent.is_file() {
            bail!("output path parent '{}' is a file", parent.display());
        }
    }

    Ok(())
}

fn default_phase_updates_path() -> Option<PathBuf> {
    DEFAULT_PHASE_UPDATES_PATH.map(PathBuf::from)
}

fn default_node_events_log() -> Option<PathBuf> {
    DEFAULT_NODE_EVENTS_LOG.map(PathBuf::from)
}

fn pick<T: Clone>(cli: Option<T>, file: Option<T>, default: T) -> T {
    cli.or(file).unwrap_or(default)
}

fn pick_optional_string(cli: Option<String>, file: Option<String>) -> Option<String> {
    cli.or(file)
        .or_else(|| DEFAULT_TIMEFRAME.map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::infer_symbol_from_path;
    use std::path::Path;

    #[test]
    fn infers_nq_from_nested_path() {
        let symbol = infer_symbol_from_path(Path::new("data/nq_1h.csv"));
        assert_eq!(symbol.as_deref(), Some("NQ=F"));
    }

    #[test]
    fn infers_es_from_relative_path() {
        let symbol = infer_symbol_from_path(Path::new("./es_1h.csv"));
        assert_eq!(symbol.as_deref(), Some("ES=F"));
    }

    #[test]
    fn uppercases_unknown_segment() {
        let symbol = infer_symbol_from_path(Path::new("volx_5m.csv"));
        assert_eq!(symbol.as_deref(), Some("VOLX"));
    }
}
