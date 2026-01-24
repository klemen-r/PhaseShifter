//! Multi-Timeframe Tracker - Phase alignment across scenarios
//!
//! Tracks phase state across multiple timeframe/phase_window scenarios:
//! - HTF (Higher Timeframe) trend from H1
//! - HTF DM (Donchian Midpoint) distance
//! - Alignment score (how many scenarios agree on direction)

use std::collections::HashMap;

use crate::bar_builder::Timeframe;

/// Phase direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Bullish,
    Bearish,
}

impl Phase {
    /// Convert to numeric value (1 = bullish, -1 = bearish)
    pub fn to_f64(&self) -> f64 {
        match self {
            Phase::Bullish => 1.0,
            Phase::Bearish => -1.0,
        }
    }
}

/// Scenario key: (timeframe, phase_window)
type ScenarioKey = (Timeframe, usize);

/// Phase state for a single scenario
#[derive(Debug, Clone)]
struct ScenarioState {
    phase: Phase,
    anchor: f64,      // Current Donchian Midpoint
    last_update: i64, // Timestamp of last update
}

/// Multi-timeframe tracker for phase alignment features
#[derive(Debug)]
pub struct MtfTracker {
    // Phase state per scenario
    scenarios: HashMap<ScenarioKey, ScenarioState>,

    // HTF reference (uses H1 by default, or highest available)
    htf_timeframe: Timeframe,
    htf_phase_window: usize,
}

impl MtfTracker {
    /// Create a new MTF tracker
    pub fn new() -> Self {
        Self {
            scenarios: HashMap::new(),
            htf_timeframe: Timeframe::M5,
            htf_phase_window: 20, // Default to M5/20 for <=5m confirmation context
        }
    }

    /// Create with custom HTF reference
    pub fn with_htf(htf_timeframe: Timeframe, htf_phase_window: usize) -> Self {
        let mut tracker = Self::new();
        tracker.htf_timeframe = htf_timeframe;
        tracker.htf_phase_window = htf_phase_window;
        tracker
    }

    /// Update phase state for a scenario
    pub fn on_phase_update(
        &mut self,
        timeframe: Timeframe,
        phase_window: usize,
        phase: Phase,
        anchor: f64,
        timestamp_ms: i64,
    ) {
        let key = (timeframe, phase_window);
        self.scenarios.insert(
            key,
            ScenarioState {
                phase,
                anchor,
                last_update: timestamp_ms,
            },
        );
    }

    /// Get the HTF scenario state
    fn htf_state(&self) -> Option<&ScenarioState> {
        self.scenarios
            .get(&(self.htf_timeframe, self.htf_phase_window))
    }

    // === Feature Getters ===

    /// HTF trend direction (1 = bullish, -1 = bearish, 0 = unknown)
    pub fn htf_trend(&self) -> f64 {
        self.htf_state().map(|s| s.phase.to_f64()).unwrap_or(0.0)
    }

    /// Distance from current price to HTF DM (anchor) in ticks
    pub fn htf_dm_distance(&self, current_price: f64, tick_size: f64) -> f64 {
        self.htf_state()
            .map(|s| (current_price - s.anchor) / tick_size)
            .unwrap_or(0.0)
    }

    /// HTF anchor (DM) price
    pub fn htf_anchor(&self) -> Option<f64> {
        self.htf_state().map(|s| s.anchor)
    }

    /// Alignment score: proportion of scenarios that agree with HTF direction
    /// Returns value between 0.0 (all disagree) and 1.0 (all agree)
    pub fn tf_alignment(&self) -> f64 {
        let htf_phase = match self.htf_state() {
            Some(s) => s.phase,
            None => return 0.5, // No HTF reference, assume neutral
        };

        if self.scenarios.is_empty() {
            return 0.5;
        }

        let agreeing = self
            .scenarios
            .values()
            .filter(|s| s.phase == htf_phase)
            .count();

        agreeing as f64 / self.scenarios.len() as f64
    }

    /// Alignment score as count: how many scenarios agree with HTF direction
    pub fn tf_alignment_count(&self) -> (usize, usize) {
        let htf_phase = match self.htf_state() {
            Some(s) => s.phase,
            None => return (0, self.scenarios.len()),
        };

        let agreeing = self
            .scenarios
            .values()
            .filter(|s| s.phase == htf_phase)
            .count();

        (agreeing, self.scenarios.len())
    }

    /// Count of bullish scenarios
    pub fn bullish_count(&self) -> usize {
        self.scenarios
            .values()
            .filter(|s| s.phase == Phase::Bullish)
            .count()
    }

    /// Count of bearish scenarios
    pub fn bearish_count(&self) -> usize {
        self.scenarios
            .values()
            .filter(|s| s.phase == Phase::Bearish)
            .count()
    }

    /// Total number of tracked scenarios
    pub fn scenario_count(&self) -> usize {
        self.scenarios.len()
    }

    /// Get phase for a specific scenario
    pub fn get_phase(&self, timeframe: Timeframe, phase_window: usize) -> Option<Phase> {
        self.scenarios
            .get(&(timeframe, phase_window))
            .map(|s| s.phase)
    }

    /// Get anchor for a specific scenario
    pub fn get_anchor(&self, timeframe: Timeframe, phase_window: usize) -> Option<f64> {
        self.scenarios
            .get(&(timeframe, phase_window))
            .map(|s| s.anchor)
    }

    /// Check if all scenarios are aligned (same direction)
    pub fn is_fully_aligned(&self) -> bool {
        if self.scenarios.len() < 2 {
            return false;
        }

        let first_phase = self.scenarios.values().next().map(|s| s.phase);
        first_phase.map_or(false, |p| self.scenarios.values().all(|s| s.phase == p))
    }

    /// Get the majority phase direction
    pub fn majority_phase(&self) -> Option<Phase> {
        let bullish = self.bullish_count();
        let bearish = self.bearish_count();

        if bullish > bearish {
            Some(Phase::Bullish)
        } else if bearish > bullish {
            Some(Phase::Bearish)
        } else {
            None // Tie
        }
    }
}

impl Default for MtfTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_updates() {
        let mut tracker = MtfTracker::new();
        let time = 1000000_i64;

        tracker.on_phase_update(Timeframe::M1, 50, Phase::Bullish, 18000.0, time);
        tracker.on_phase_update(Timeframe::M5, 7, Phase::Bullish, 18010.0, time);
        tracker.on_phase_update(Timeframe::M5, 20, Phase::Bearish, 17990.0, time);
        tracker.on_phase_update(Timeframe::H1, 7, Phase::Bullish, 18050.0, time);

        assert_eq!(tracker.scenario_count(), 4);
        assert_eq!(tracker.bullish_count(), 3);
        assert_eq!(tracker.bearish_count(), 1);
    }

    #[test]
    fn test_htf_trend() {
        let mut tracker = MtfTracker::new();
        let time = 1000000_i64;

        // Set HTF (H1/7) as bullish
        tracker.on_phase_update(Timeframe::H1, 7, Phase::Bullish, 18050.0, time);

        assert_eq!(tracker.htf_trend(), 1.0);
        assert_eq!(tracker.htf_anchor(), Some(18050.0));
    }

    #[test]
    fn test_alignment() {
        let mut tracker = MtfTracker::new();
        let time = 1000000_i64;

        // HTF bullish
        tracker.on_phase_update(Timeframe::H1, 7, Phase::Bullish, 18050.0, time);

        // 3 bullish, 1 bearish
        tracker.on_phase_update(Timeframe::M1, 50, Phase::Bullish, 18000.0, time);
        tracker.on_phase_update(Timeframe::M5, 7, Phase::Bullish, 18010.0, time);
        tracker.on_phase_update(Timeframe::M5, 20, Phase::Bearish, 17990.0, time);

        // Alignment = 3/4 = 0.75 (including HTF itself)
        let alignment = tracker.tf_alignment();
        assert!((alignment - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_dm_distance() {
        let mut tracker = MtfTracker::new();
        let time = 1000000_i64;

        tracker.on_phase_update(Timeframe::H1, 7, Phase::Bullish, 18000.0, time);

        // Current price 18025, tick_size 0.25
        // Distance = (18025 - 18000) / 0.25 = 100 ticks
        let dist = tracker.htf_dm_distance(18025.0, 0.25);
        assert!((dist - 100.0).abs() < 0.1);
    }
}
