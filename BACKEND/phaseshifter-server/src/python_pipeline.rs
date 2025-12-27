//! Python Pipeline Runner
//!
//! Calls the Python `run_phase_pipeline.py` script via subprocess to generate
//! clusters for yfinance symbols. This ensures cluster generation matches
//! the proven Python implementation exactly.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Cluster data from Python pipeline (matches pinescript_clusters.json format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonCluster {
    pub side: String,
    pub low: f64,
    pub high: f64,
}

/// Per-ticker data from Python pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonTickerData {
    pub timestamp: String,
    pub anchor: Option<f64>,
    pub clusters: Vec<PythonCluster>,
}

/// Clusters data formatted for WebSocket response
#[derive(Debug, Clone, Serialize)]
pub struct PythonClustersData {
    pub ticker: String,
    pub anchor: Option<f64>,
    pub clusters: Vec<PythonClusterResponse>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PythonClusterResponse {
    pub side: String,
    pub low: f64,
    pub high: f64,
    pub width: f64,
}

/// Python pipeline runner that calls run_phase_pipeline.py
pub struct PythonPipelineRunner {
    /// Path to the PhaseShifter root directory
    root_dir: PathBuf,
    /// Path to pinescript_clusters.json
    clusters_cache_path: PathBuf,
    /// Cached cluster data per ticker
    cache: Arc<RwLock<HashMap<String, PythonClustersData>>>,
    /// Python executable path
    python_exe: String,
}

impl PythonPipelineRunner {
    pub fn new(root_dir: PathBuf) -> Self {
        let clusters_cache_path = root_dir.join("pinescript_clusters.json");
        Self {
            root_dir,
            clusters_cache_path,
            cache: Arc::new(RwLock::new(HashMap::new())),
            python_exe: "python".to_string(), // Use system python
        }
    }

    /// Run the Python pipeline for a ticker and return clusters
    pub async fn run_for_ticker(&self, ticker: &str) -> Result<PythonClustersData> {
        let ticker_upper = ticker.to_uppercase();

        info!(
            "[python-pipeline] Running pipeline for {} in {:?}",
            ticker_upper, self.root_dir
        );

        // Run the Python script
        let script_path = self.root_dir.join("run_phase_pipeline.py");
        if !script_path.exists() {
            return Err(anyhow!(
                "Python pipeline script not found: {:?}",
                script_path
            ));
        }

        // Use --allow-single-scenario flag to match the working behavior
        let output = Command::new(&self.python_exe)
            .arg(&script_path)
            .arg(&ticker_upper)
            .arg("--allow-single-scenario")
            .current_dir(&self.root_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            error!(
                "[python-pipeline] Script failed for {}: exit={:?}\nstderr: {}\nstdout: {}",
                ticker_upper, output.status, stderr, stdout
            );
            return Err(anyhow!(
                "Python pipeline failed: {}",
                stderr.lines().last().unwrap_or("unknown error")
            ));
        }

        // Log output
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            debug!("[python-pipeline] {}", line);
        }

        // Read the clusters from pinescript_clusters.json
        let data = self.read_clusters_cache(&ticker_upper).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(ticker_upper.clone(), data.clone());
        }

        info!(
            "[python-pipeline] Generated {} clusters for {}",
            data.clusters.len(),
            ticker_upper
        );

        Ok(data)
    }

    /// Read clusters from the cache file
    async fn read_clusters_cache(&self, ticker: &str) -> Result<PythonClustersData> {
        let content = tokio::fs::read_to_string(&self.clusters_cache_path).await?;
        let cache: HashMap<String, PythonTickerData> = serde_json::from_str(&content)?;

        let ticker_data = cache
            .get(ticker)
            .ok_or_else(|| anyhow!("Ticker {} not found in clusters cache", ticker))?;

        Ok(PythonClustersData {
            ticker: ticker.to_string(),
            anchor: ticker_data.anchor,
            clusters: ticker_data
                .clusters
                .iter()
                .map(|c| PythonClusterResponse {
                    side: c.side.clone(),
                    low: c.low,
                    high: c.high,
                    width: c.high - c.low,
                })
                .collect(),
            generated_at: ticker_data.timestamp.clone(),
        })
    }

    /// Get cached clusters for a ticker (without running pipeline)
    pub async fn get_cached(&self, ticker: &str) -> Option<PythonClustersData> {
        let ticker_upper = ticker.to_uppercase();

        // First check in-memory cache
        {
            let cache = self.cache.read().await;
            if let Some(data) = cache.get(&ticker_upper) {
                return Some(data.clone());
            }
        }

        // Try to read from file cache
        match self.read_clusters_cache(&ticker_upper).await {
            Ok(data) => {
                // Update in-memory cache
                let mut cache = self.cache.write().await;
                cache.insert(ticker_upper, data.clone());
                Some(data)
            }
            Err(_) => None,
        }
    }

    /// Get or run pipeline for a ticker
    pub async fn get_or_run(&self, ticker: &str) -> Result<PythonClustersData> {
        // Check cache first
        if let Some(data) = self.get_cached(ticker).await {
            return Ok(data);
        }

        // Run pipeline
        self.run_for_ticker(ticker).await
    }

    /// Clear cache for a ticker
    pub async fn clear_cache(&self, ticker: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(&ticker.to_uppercase());
    }
}
