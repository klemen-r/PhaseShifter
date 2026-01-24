//! Cluster Calculation Module
//!
//! Implements anchor-normalized, adaptive-gap clustering algorithm
//! for grouping projection nodes from multiple timeframe/window scenarios.
//!
//! Nodes are tracked with open/closed status - a node becomes closed when
//! its projected target is hit (high >= target for bullish, low <= target for bearish).
//! Only OPEN nodes are used for clustering.
//!
//! Each scenario (timeframe + phase_window) tracks its own anchor independently.
//! Nodes are projected using their own scenario's current anchor.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tracing::debug;

/// Scenario key: (timeframe, phase_window)
pub type ScenarioKey = (String, usize);

/// Node status (open = target not yet hit, closed = target was hit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Open,
    Closed,
}

/// A tracked projection node from a phase engine scenario
#[derive(Debug, Clone)]
pub struct TrackedNode {
    pub side: ProjectionSide,    // bullish/bearish
    pub timeframe: String,       // e.g., "M1", "M5", "M15"
    pub phase_window: usize,     // Donchian window used
    pub distance_pct: f64,       // Distance as percentage from anchor
    pub anchor_at_creation: f64, // Anchor when node was created
    pub created_at: i64,         // Timestamp
    pub status: NodeStatus,      // Open or Closed
    pub closed_at: Option<i64>,  // When the node was closed (target hit)
}

impl TrackedNode {
    /// Create a new open node
    pub fn new(
        side: ProjectionSide,
        timeframe: String,
        phase_window: usize,
        distance_pct: f64,
        anchor_at_creation: f64,
        created_at: i64,
    ) -> Self {
        Self {
            side,
            timeframe,
            phase_window,
            distance_pct,
            anchor_at_creation,
            created_at,
            status: NodeStatus::Open,
            closed_at: None,
        }
    }

    /// Get the scenario key for this node
    pub fn scenario_key(&self) -> ScenarioKey {
        (self.timeframe.clone(), self.phase_window)
    }

    /// Calculate projected price given current anchor
    pub fn projected_price(&self, current_anchor: f64) -> f64 {
        match self.side {
            ProjectionSide::Bullish => current_anchor * (1.0 + self.distance_pct),
            ProjectionSide::Bearish => current_anchor * (1.0 - self.distance_pct),
        }
    }

    /// Update node with bar data - returns true if node was closed
    pub fn update_with_bar(
        &mut self,
        current_anchor: f64,
        high: f64,
        low: f64,
        timestamp: i64,
    ) -> bool {
        if self.status == NodeStatus::Closed {
            return false;
        }

        let projection = self.projected_price(current_anchor);
        let breached = match self.side {
            ProjectionSide::Bullish => high >= projection,
            ProjectionSide::Bearish => low <= projection,
        };

        if breached {
            self.status = NodeStatus::Closed;
            self.closed_at = Some(timestamp);
            true
        } else {
            false
        }
    }

    /// Convert to ProjectionNode for clustering (uses current anchor for value)
    pub fn to_projection_node(&self, current_anchor: f64) -> ProjectionNode {
        ProjectionNode {
            value: self.projected_price(current_anchor),
            side: self.side,
            timeframe: self.timeframe.clone(),
            phase_window: self.phase_window,
            distance_pct: self.distance_pct,
            anchor_at_creation: self.anchor_at_creation,
            created_at: self.created_at,
        }
    }
}

/// A projection node for clustering (derived from TrackedNode)
#[derive(Debug, Clone)]
pub struct ProjectionNode {
    pub value: f64,              // Projected target price (calculated from current anchor)
    pub side: ProjectionSide,    // bullish/bearish
    pub timeframe: String,       // e.g., "M1", "M5", "M15"
    pub phase_window: usize,     // Donchian window used
    pub distance_pct: f64,       // Distance as percentage from anchor
    pub anchor_at_creation: f64, // Anchor when node was created
    pub created_at: i64,         // Timestamp
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectionSide {
    Bullish,
    Bearish,
}

impl std::fmt::Display for ProjectionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectionSide::Bullish => write!(f, "bullish"),
            ProjectionSide::Bearish => write!(f, "bearish"),
        }
    }
}

/// A cluster of projection nodes
#[derive(Debug, Clone, Serialize)]
pub struct Cluster {
    pub side: ProjectionSide,
    pub low: f64,                // Zone floor (price)
    pub high: f64,               // Zone ceiling (price)
    pub count: usize,            // Number of nodes in cluster
    pub unique_scenarios: usize, // Distinct (timeframe, phase_window) pairs
}

/// Full cluster data for a symbol
#[derive(Debug, Clone, Serialize)]
pub struct ClustersData {
    pub ticker: String,
    pub anchor: Option<f64>,
    pub clusters: Vec<Cluster>,
    pub nodes: Vec<NodeInfo>,
    pub generated_at: String,
}

/// Node info for frontend display
#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    pub value: f64,
    pub side: ProjectionSide,
    pub interval: String,
    pub phase_window: usize,
    pub depth_days: i64,
    pub pct_from_anchor: Option<f64>,
}

/// Clustering configuration
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    pub min_unique_scenarios: usize, // Minimum distinct scenarios per cluster (default: 2)
    pub outlier_cutoff_mult: f64,    // Multiplier for outlier cutoff (default: 2.2)
    pub core_gap_mult: f64,          // Multiplier for core gap (default: 2.0)
    pub max_width_mult: f64,         // Multiplier for max cluster width (default: 5.0)
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            min_unique_scenarios: 2,
            outlier_cutoff_mult: 2.2,
            core_gap_mult: 2.0,
            max_width_mult: 5.0,
        }
    }
}

/// Internal node representation for clustering
#[derive(Debug)]
struct ClusterNode {
    deviation: f64,            // Normalized deviation from anchor
    scenario: (String, usize), // (timeframe, phase_window)
}

/// Calculate clusters from projection nodes
pub fn calculate_clusters(
    nodes: &[ProjectionNode],
    config: &ClusterConfig,
) -> Option<(f64, Vec<Cluster>)> {
    if nodes.is_empty() {
        return None;
    }

    // Step 1: Compute anchor as median of derived anchors
    let anchor = compute_anchor(nodes)?;
    debug!("Computed anchor: {:.2}", anchor);

    // Step 2: Convert nodes to deviation space and split by side
    let mut bullish_nodes = Vec::new();
    let mut bearish_nodes = Vec::new();
    for n in nodes {
        let cluster_node = ClusterNode {
            deviation: (n.value - anchor) / anchor,
            scenario: (n.timeframe.clone(), n.phase_window),
        };

        match n.side {
            ProjectionSide::Bullish => bullish_nodes.push(cluster_node),
            ProjectionSide::Bearish => bearish_nodes.push(cluster_node),
        }
    }

    let mut clusters = Vec::new();

    // Cluster bullish nodes
    if bullish_nodes.len() >= 2 {
        let bullish_clusters = cluster_side(bullish_nodes, ProjectionSide::Bullish, anchor, config);
        clusters.extend(bullish_clusters);
    }

    // Cluster bearish nodes
    if bearish_nodes.len() >= 2 {
        let bearish_clusters = cluster_side(bearish_nodes, ProjectionSide::Bearish, anchor, config);
        clusters.extend(bearish_clusters);
    }

    // Step 4: Deduplicate overlapping clusters
    let clusters = dedupe_overlapping_clusters(clusters);

    Some((anchor, clusters))
}

/// Compute anchor as median of derived anchors from all nodes
fn compute_anchor(nodes: &[ProjectionNode]) -> Option<f64> {
    let mut derived_anchors = Vec::with_capacity(nodes.len());
    for n in nodes {
        if n.distance_pct.abs() < 1e-10 {
            continue; // Skip zero-distance nodes
        }
        // Derive anchor: value = anchor * (1 � distance_pct)
        // anchor = value / (1 � distance_pct)
        let anchor = match n.side {
            ProjectionSide::Bullish => n.value / (1.0 + n.distance_pct),
            ProjectionSide::Bearish => n.value / (1.0 - n.distance_pct),
        };
        if anchor.is_finite() && anchor > 0.0 {
            derived_anchors.push(anchor);
        }
    }

    if derived_anchors.is_empty() {
        return None;
    }

    Some(median(&derived_anchors))
}

/// Cluster nodes of a single side using adaptive-gap algorithm
fn cluster_side(
    mut nodes: Vec<ClusterNode>,
    side: ProjectionSide,
    anchor: f64,
    config: &ClusterConfig,
) -> Vec<Cluster> {
    if nodes.len() < 2 {
        return Vec::new();
    }

    // Sort by deviation
    nodes.sort_by(|a, b| a.deviation.partial_cmp(&b.deviation).unwrap());

    // Calculate gaps between consecutive nodes
    let gaps: Vec<f64> = nodes
        .windows(2)
        .map(|w| (w[1].deviation - w[0].deviation).abs())
        .collect();

    if gaps.is_empty() {
        return Vec::new();
    }

    // Compute seed gap from 20th-50th percentile of sorted non-zero gaps
    let mut nonzero_gaps: Vec<f64> = gaps.iter().copied().filter(|&g| g > 1e-10).collect();
    if nonzero_gaps.is_empty() {
        return Vec::new();
    }
    nonzero_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let start_idx = nonzero_gaps.len() * 20 / 100;
    let end_idx = (nonzero_gaps.len() * 50 / 100).max(start_idx + 1);
    let seed_gap = median(&nonzero_gaps[start_idx..end_idx]);

    // Initial seeding: group nodes with gap <= seed_gap
    let seed_groups = initial_seeding(&nodes, seed_gap);

    // Refine each seed group
    let mut clusters = Vec::new();
    for group in seed_groups {
        if group.len() < 2 {
            continue;
        }

        // Compute scale for this group
        let scale = compute_scale(&group);
        if scale < 1e-10 {
            continue;
        }

        // Outlier removal
        let out_cut = config.outlier_cutoff_mult * scale;
        let mut deviations = Vec::with_capacity(group.len());
        deviations.extend(group.iter().map(|n| n.deviation));
        let center = median(&deviations);
        let pruned: Vec<_> = group
            .into_iter()
            .filter(|n| (n.deviation - center).abs() <= out_cut)
            .collect();

        if pruned.len() < 2 {
            continue;
        }

        // Subcluster with core_gap
        let core_gap = config.core_gap_mult * scale;
        let subclusters = subcluster(&pruned, core_gap);

        for subcluster in subclusters {
            // Check width constraint
            let (min_dev, max_dev) = subcluster.iter().fold(
                (f64::INFINITY, f64::NEG_INFINITY),
                |(min_dev, max_dev), n| (min_dev.min(n.deviation), max_dev.max(n.deviation)),
            );
            let width_dev = max_dev - min_dev;

            let max_width = config.max_width_mult * scale;
            if width_dev > max_width {
                continue;
            }

            // Check unique scenarios
            let mut unique_scenarios = HashSet::with_capacity(subcluster.len());
            for n in &subcluster {
                unique_scenarios.insert((n.scenario.0.as_str(), n.scenario.1));
            }
            if unique_scenarios.len() < config.min_unique_scenarios {
                continue;
            }

            // Convert back to prices
            let low = anchor * (1.0 + min_dev);
            let high = anchor * (1.0 + max_dev);

            clusters.push(Cluster {
                side,
                low,
                high,
                count: subcluster.len(),
                unique_scenarios: unique_scenarios.len(),
            });
        }
    }

    clusters
}

/// Initial seeding: group nodes with consecutive gaps <= seed_gap
fn initial_seeding<'a>(
    sorted_nodes: &'a [ClusterNode],
    seed_gap: f64,
) -> Vec<Vec<&'a ClusterNode>> {
    let mut groups = Vec::new();
    let mut current_group = vec![&sorted_nodes[0]];

    for i in 1..sorted_nodes.len() {
        let gap = (sorted_nodes[i].deviation - sorted_nodes[i - 1].deviation).abs();
        if gap <= seed_gap {
            current_group.push(&sorted_nodes[i]);
        } else {
            if !current_group.is_empty() {
                groups.push(current_group);
            }
            current_group = vec![&sorted_nodes[i]];
        }
    }

    if !current_group.is_empty() {
        groups.push(current_group);
    }

    groups
}

/// Compute scale as median of smallest gaps (handles multimodal distributions)
fn compute_scale(nodes: &[&ClusterNode]) -> f64 {
    if nodes.len() < 2 {
        return 0.0;
    }

    let mut sorted = nodes.to_vec();
    sorted.sort_by(|a, b| a.deviation.partial_cmp(&b.deviation).unwrap());

    let gaps: Vec<f64> = sorted
        .windows(2)
        .map(|w| (w[1].deviation - w[0].deviation).abs())
        .filter(|&g| g > 1e-10)
        .collect();

    if gaps.is_empty() {
        return 0.0;
    }

    let mut sorted_gaps = gaps;
    sorted_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Take median of smallest half of gaps
    let n = (sorted_gaps.len() + 1) / 2;
    median(&sorted_gaps[..n])
}

/// Split nodes into subclusters based on core_gap
fn subcluster<'a>(nodes: &[&'a ClusterNode], core_gap: f64) -> Vec<Vec<&'a ClusterNode>> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let mut sorted = nodes.to_vec();
    sorted.sort_by(|a, b| a.deviation.partial_cmp(&b.deviation).unwrap());

    let mut groups = Vec::new();
    let mut current = vec![sorted[0]];

    for i in 1..sorted.len() {
        let gap = (sorted[i].deviation - sorted[i - 1].deviation).abs();
        if gap <= core_gap {
            current.push(sorted[i]);
        } else {
            if current.len() >= 2 {
                groups.push(current);
            }
            current = vec![sorted[i]];
        }
    }

    if current.len() >= 2 {
        groups.push(current);
    }

    groups
}

/// Deduplicate overlapping clusters, keeping the "strongest" one per overlap group
fn dedupe_overlapping_clusters(mut clusters: Vec<Cluster>) -> Vec<Cluster> {
    if clusters.len() <= 1 {
        return clusters;
    }

    // Sort by (side, low, high)
    clusters.sort_by(|a, b| match (&a.side, &b.side) {
        (ProjectionSide::Bullish, ProjectionSide::Bearish) => std::cmp::Ordering::Less,
        (ProjectionSide::Bearish, ProjectionSide::Bullish) => std::cmp::Ordering::Greater,
        _ => a
            .low
            .partial_cmp(&b.low)
            .unwrap()
            .then(a.high.partial_cmp(&b.high).unwrap()),
    });

    let mut result = Vec::new();
    let mut i = 0;

    while i < clusters.len() {
        let mut overlap_group = vec![clusters[i].clone()];
        let mut j = i + 1;

        // Find all overlapping clusters of same side
        while j < clusters.len() && clusters[j].side == clusters[i].side {
            let prev = &overlap_group.last().unwrap();
            if clusters[j].low <= prev.high {
                overlap_group.push(clusters[j].clone());
            } else {
                break;
            }
            j += 1;
        }

        // Keep the strongest cluster from overlap group
        // Ranking: smallest width, most scenarios, most count
        overlap_group.sort_by(|a, b| {
            let width_a = a.high - a.low;
            let width_b = b.high - b.low;
            width_a
                .partial_cmp(&width_b)
                .unwrap()
                .then(b.unique_scenarios.cmp(&a.unique_scenarios))
                .then(b.count.cmp(&a.count))
        });

        result.push(overlap_group.remove(0));
        i = j;
    }

    result
}

/// Calculate median of a slice
fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

/// Cluster Manager - manages multiple scenarios and computes clusters
///
/// Tracks nodes with open/closed status. A node is closed when its
/// projected target is hit. Only OPEN nodes are used for clustering.
///
/// Each scenario (timeframe + phase_window) tracks its own anchor independently.
/// Nodes are projected using their own scenario's current anchor.
#[derive(Debug)]
pub struct ClusterManager {
    /// Tracked nodes per symbol (includes both open and closed)
    nodes: HashMap<String, Vec<TrackedNode>>,
    /// Current anchor per symbol AND scenario: (symbol, timeframe, phase_window) -> anchor
    scenario_anchors: HashMap<(String, String, usize), f64>,
    /// Cached cluster results per symbol
    cache: HashMap<String, ClustersData>,
    /// Configuration
    config: ClusterConfig,
}

impl ClusterManager {
    pub fn new(config: ClusterConfig) -> Self {
        Self {
            nodes: HashMap::new(),
            scenario_anchors: HashMap::new(),
            cache: HashMap::new(),
            config,
        }
    }

    /// Add a new node for a symbol (starts as Open)
    pub fn add_node(&mut self, symbol: &str, node: TrackedNode) {
        let nodes = self
            .nodes
            .entry(symbol.to_string())
            .or_insert_with(Vec::new);
        nodes.push(node);
        // Invalidate cache when nodes change
        self.cache.remove(symbol);

        let open_count = nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Open)
            .count();
        debug!(
            "Added node for {}: {} open / {} total nodes",
            symbol,
            open_count,
            nodes.len()
        );
    }

    /// Set the anchor for a specific scenario (timeframe + phase_window)
    pub fn set_scenario_anchor(
        &mut self,
        symbol: &str,
        timeframe: &str,
        phase_window: usize,
        anchor: f64,
    ) {
        let key = (symbol.to_string(), timeframe.to_string(), phase_window);
        self.scenario_anchors.insert(key, anchor);
        // Invalidate cache as projections depend on anchors
        self.cache.remove(symbol);
    }

    /// Get the anchor for a specific scenario
    pub fn get_scenario_anchor(
        &self,
        symbol: &str,
        timeframe: &str,
        phase_window: usize,
    ) -> Option<f64> {
        let key = (symbol.to_string(), timeframe.to_string(), phase_window);
        self.scenario_anchors.get(&key).copied()
    }

    /// Update nodes for a specific scenario with new bar data
    /// Only updates nodes belonging to this scenario using this scenario's anchor
    pub fn update_scenario_with_bar(
        &mut self,
        symbol: &str,
        timeframe: &str,
        phase_window: usize,
        anchor: f64,
        high: f64,
        low: f64,
        timestamp: i64,
    ) -> usize {
        // Update stored anchor for this scenario
        self.set_scenario_anchor(symbol, timeframe, phase_window, anchor);

        let nodes = match self.nodes.get_mut(symbol) {
            Some(n) => n,
            None => return 0,
        };

        let mut closed_count = 0;
        for node in nodes.iter_mut() {
            // Only process nodes from this specific scenario
            if node.timeframe != timeframe || node.phase_window != phase_window {
                continue;
            }

            if node.update_with_bar(anchor, high, low, timestamp) {
                closed_count += 1;
                debug!(
                    "Node closed for {} ({}/{}): {:?} @ {:.2} (target hit)",
                    symbol,
                    timeframe,
                    phase_window,
                    node.side,
                    node.projected_price(anchor)
                );
            }
        }

        if closed_count > 0 {
            // Invalidate cache when nodes are closed
            self.cache.remove(symbol);
            debug!(
                "Closed {} nodes for {} ({}/{}) on bar update",
                closed_count, symbol, timeframe, phase_window
            );
        }

        closed_count
    }

    /// Clear all nodes for a symbol
    pub fn clear_nodes(&mut self, symbol: &str) {
        self.nodes.remove(symbol);
        self.cache.remove(symbol);
        // Also clear scenario anchors for this symbol
        self.scenario_anchors.retain(|(s, _, _), _| s != symbol);
    }

    /// Get or compute clusters for a symbol (uses only OPEN nodes)
    /// Each node is projected using its own scenario's current anchor
    pub fn get_clusters(&mut self, symbol: &str) -> Option<&ClustersData> {
        if !self.cache.contains_key(symbol) {
            let result = self.compute_clusters(symbol)?;
            self.cache.insert(symbol.to_string(), result);
        }

        self.cache.get(symbol)
    }

    fn compute_clusters(&self, symbol: &str) -> Option<ClustersData> {
        let tracked_nodes = self.nodes.get(symbol)?;

        // Filter to only OPEN nodes while building projections
        let mut projection_nodes = Vec::with_capacity(tracked_nodes.len());
        let mut missing_anchors = HashSet::new();
        let mut open_count = 0;

        for node in tracked_nodes {
            if node.status != NodeStatus::Open {
                continue;
            }
            open_count += 1;
            let anchor_key = (
                symbol.to_string(),
                node.timeframe.clone(),
                node.phase_window,
            );
            if let Some(&scenario_anchor) = self.scenario_anchors.get(&anchor_key) {
                projection_nodes.push(node.to_projection_node(scenario_anchor));
            } else {
                // No anchor for this scenario yet - skip node but log it
                missing_anchors.insert((node.timeframe.clone(), node.phase_window));
            }
        }

        debug!(
            "get_clusters for {}: {} open / {} total nodes",
            symbol,
            open_count,
            tracked_nodes.len()
        );

        if open_count == 0 {
            debug!("No open nodes for {}, returning None", symbol);
            return None;
        }

        if !missing_anchors.is_empty() {
            debug!(
                "Missing anchors for {} scenarios: {:?}",
                missing_anchors.len(),
                missing_anchors
            );
        }

        if projection_nodes.is_empty() {
            debug!("No projectable nodes for {} (missing anchors?)", symbol);
            return None;
        }

        let cluster_result = calculate_clusters(&projection_nodes, &self.config);
        debug!(
            "calculate_clusters for {} returned: {:?}",
            symbol,
            cluster_result.as_ref().map(|(a, c)| (a, c.len()))
        );

        let (anchor, clusters) = cluster_result?;

        // Build node info for frontend (only open nodes with valid projections)
        let mut node_infos = Vec::with_capacity(projection_nodes.len());
        for n in &projection_nodes {
            node_infos.push(NodeInfo {
                value: n.value,
                side: n.side,
                interval: n.timeframe.clone(),
                phase_window: n.phase_window,
                depth_days: 50,
                pct_from_anchor: Some(n.distance_pct * 100.0),
            });
        }

        let result = ClustersData {
            ticker: symbol.to_string(),
            anchor: Some(anchor),
            clusters,
            nodes: node_infos,
            generated_at: chrono::Utc::now().to_rfc3339(),
        };

        debug!(
            "Calculated {} clusters for {} from {} open nodes (median anchor: {:.2})",
            result.clusters.len(),
            symbol,
            projection_nodes.len(),
            anchor
        );

        Some(result)
    }

    /// Get count of open nodes for a symbol
    pub fn open_node_count(&self, symbol: &str) -> usize {
        self.nodes.get(symbol).map_or(0, |n| {
            n.iter().filter(|n| n.status == NodeStatus::Open).count()
        })
    }

    /// Get total node count for a symbol (open + closed)
    pub fn node_count(&self, symbol: &str) -> usize {
        self.nodes.get(symbol).map_or(0, |n| n.len())
    }

    /// Get all tracked nodes for a symbol
    pub fn get_nodes(&self, symbol: &str) -> Option<&Vec<TrackedNode>> {
        self.nodes.get(symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_median() {
        assert_eq!(median(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(median(&[5.0]), 5.0);
    }

    #[test]
    fn test_compute_anchor() {
        let nodes = vec![
            ProjectionNode {
                value: 105.0,
                side: ProjectionSide::Bullish,
                timeframe: "M5".to_string(),
                phase_window: 7,
                distance_pct: 0.05, // 5% above anchor
                anchor_at_creation: 100.0,
                created_at: 0,
            },
            ProjectionNode {
                value: 95.0,
                side: ProjectionSide::Bearish,
                timeframe: "M5".to_string(),
                phase_window: 7,
                distance_pct: 0.05, // 5% below anchor
                anchor_at_creation: 100.0,
                created_at: 0,
            },
        ];

        let anchor = compute_anchor(&nodes).unwrap();
        assert!((anchor - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_cluster_calculation() {
        let nodes = vec![
            // Cluster 1: ~5% bullish
            ProjectionNode {
                value: 105.0,
                side: ProjectionSide::Bullish,
                timeframe: "M5".to_string(),
                phase_window: 7,
                distance_pct: 0.05,
                anchor_at_creation: 100.0,
                created_at: 0,
            },
            ProjectionNode {
                value: 105.2,
                side: ProjectionSide::Bullish,
                timeframe: "M15".to_string(),
                phase_window: 10,
                distance_pct: 0.052,
                anchor_at_creation: 100.0,
                created_at: 0,
            },
            // Cluster 2: ~10% bullish (should not cluster with 5%)
            ProjectionNode {
                value: 110.0,
                side: ProjectionSide::Bullish,
                timeframe: "M5".to_string(),
                phase_window: 7,
                distance_pct: 0.10,
                anchor_at_creation: 100.0,
                created_at: 0,
            },
            ProjectionNode {
                value: 110.1,
                side: ProjectionSide::Bullish,
                timeframe: "M15".to_string(),
                phase_window: 10,
                distance_pct: 0.101,
                anchor_at_creation: 100.0,
                created_at: 0,
            },
        ];

        let config = ClusterConfig::default();
        let result = calculate_clusters(&nodes, &config);
        assert!(result.is_some());

        let (anchor, clusters) = result.unwrap();
        assert!((anchor - 100.0).abs() < 1.0);
        assert!(clusters.len() >= 1); // Should have at least one cluster
    }
}
