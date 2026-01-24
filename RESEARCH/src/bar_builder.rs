//! Bar builder - constructs OHLCV bars from ticks

use std::collections::VecDeque;

use crate::{Bar, Tick};

/// Timeframe enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    S15, // 15 seconds
    M1,
    M5,
    M15,
    M30,
    H1,
    H4,
    D1,
}

impl Timeframe {
    /// Duration in milliseconds
    pub fn millis(&self) -> i64 {
        match self {
            Timeframe::S15 => 15_000,
            Timeframe::M1 => 60_000,
            Timeframe::M5 => 300_000,
            Timeframe::M15 => 900_000,
            Timeframe::M30 => 1_800_000,
            Timeframe::H1 => 3_600_000,
            Timeframe::H4 => 14_400_000,
            Timeframe::D1 => 86_400_000,
        }
    }

    /// String representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::S15 => "15s",
            Timeframe::M1 => "1m",
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::M30 => "30m",
            Timeframe::H1 => "1h",
            Timeframe::H4 => "4h",
            Timeframe::D1 => "1d",
        }
    }
}

/// Get bar open time for a given timestamp and timeframe
pub fn get_bar_open_time(timestamp_ms: i64, timeframe: Timeframe) -> i64 {
    let interval = timeframe.millis();
    (timestamp_ms / interval) * interval
}

/// Single timeframe bar builder
#[derive(Debug)]
pub struct SingleBarBuilder {
    timeframe: Timeframe,
    current_bar: Option<Bar>,
    history: VecDeque<Bar>,
    max_history: usize,
}

impl SingleBarBuilder {
    pub fn new(timeframe: Timeframe, max_history: usize) -> Self {
        Self {
            timeframe,
            current_bar: None,
            history: VecDeque::with_capacity(max_history),
            max_history,
        }
    }

    /// Process a tick, returns completed bar if bar closed
    pub fn on_tick(&mut self, tick: &Tick) -> Option<Bar> {
        let bar_time = get_bar_open_time(tick.timestamp_ms, self.timeframe);
        let mut completed = None;

        match &mut self.current_bar {
            Some(bar) if bar.timestamp_ms == bar_time => {
                // Update existing bar
                bar.high = bar.high.max(tick.price);
                bar.low = bar.low.min(tick.price);
                bar.close = tick.price;
                bar.volume += tick.volume as f64;
                bar.tick_count += 1;
            }
            Some(bar) => {
                // Bar closed, start new one
                let closed_bar = *bar;
                self.history.push_back(closed_bar);
                if self.history.len() > self.max_history {
                    self.history.pop_front();
                }
                completed = Some(closed_bar);

                *bar = Bar {
                    timestamp_ms: bar_time,
                    open: tick.price,
                    high: tick.price,
                    low: tick.price,
                    close: tick.price,
                    volume: tick.volume as f64,
                    tick_count: 1,
                };
            }
            None => {
                // First bar
                self.current_bar = Some(Bar {
                    timestamp_ms: bar_time,
                    open: tick.price,
                    high: tick.price,
                    low: tick.price,
                    close: tick.price,
                    volume: tick.volume as f64,
                    tick_count: 1,
                });
            }
        }

        completed
    }

    /// Get current (incomplete) bar
    pub fn current_bar(&self) -> Option<&Bar> {
        self.current_bar.as_ref()
    }

    /// Get bar history
    pub fn history(&self) -> &VecDeque<Bar> {
        &self.history
    }

    /// Get last N completed bars
    pub fn last_n_bars(&self, n: usize) -> Vec<&Bar> {
        self.history.iter().rev().take(n).collect()
    }

    /// Get bar at index from end (0 = most recent completed)
    pub fn bar_at(&self, index: usize) -> Option<&Bar> {
        if index < self.history.len() {
            Some(&self.history[self.history.len() - 1 - index])
        } else {
            None
        }
    }

    /// Number of completed bars
    pub fn bar_count(&self) -> usize {
        self.history.len()
    }
}

/// Multi-timeframe bar builder
#[derive(Debug)]
pub struct MultiBarBuilder {
    builders: Vec<(Timeframe, SingleBarBuilder)>,
}

impl MultiBarBuilder {
    pub fn new(timeframes: &[Timeframe], max_history: usize) -> Self {
        let builders = timeframes
            .iter()
            .map(|&tf| (tf, SingleBarBuilder::new(tf, max_history)))
            .collect();

        Self { builders }
    }

    /// Process tick across all timeframes, returns completed bars
    pub fn on_tick(&mut self, tick: &Tick) -> Vec<(Timeframe, Bar)> {
        let mut completed = Vec::new();

        for (tf, builder) in &mut self.builders {
            if let Some(bar) = builder.on_tick(tick) {
                completed.push((*tf, bar));
            }
        }

        completed
    }

    /// Get builder for specific timeframe
    pub fn get_builder(&self, timeframe: Timeframe) -> Option<&SingleBarBuilder> {
        self.builders
            .iter()
            .find(|(tf, _)| *tf == timeframe)
            .map(|(_, b)| b)
    }

    /// Get mutable builder for specific timeframe
    pub fn get_builder_mut(&mut self, timeframe: Timeframe) -> Option<&mut SingleBarBuilder> {
        self.builders
            .iter_mut()
            .find(|(tf, _)| *tf == timeframe)
            .map(|(_, b)| b)
    }

    /// Get all timeframes
    pub fn timeframes(&self) -> Vec<Timeframe> {
        self.builders.iter().map(|(tf, _)| *tf).collect()
    }
}

/// Rolling statistics calculator
#[derive(Debug)]
pub struct RollingStats {
    window: VecDeque<f64>,
    max_size: usize,
    sum: f64,
    sum_sq: f64,
}

impl RollingStats {
    pub fn new(max_size: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(max_size),
            max_size,
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.window.len() >= self.max_size {
            if let Some(old) = self.window.pop_front() {
                self.sum -= old;
                self.sum_sq -= old * old;
            }
        }
        self.window.push_back(value);
        self.sum += value;
        self.sum_sq += value * value;
    }

    pub fn mean(&self) -> f64 {
        if self.window.is_empty() {
            0.0
        } else {
            self.sum / self.window.len() as f64
        }
    }

    pub fn std_dev(&self) -> f64 {
        if self.window.len() < 2 {
            return 0.0;
        }
        let n = self.window.len() as f64;
        let variance = (self.sum_sq - (self.sum * self.sum) / n) / (n - 1.0);
        variance.max(0.0).sqrt()
    }

    pub fn len(&self) -> usize {
        self.window.len()
    }

    pub fn is_full(&self) -> bool {
        self.window.len() >= self.max_size
    }
}

/// ATR calculator
#[derive(Debug)]
pub struct AtrCalculator {
    period: usize,
    true_ranges: VecDeque<f64>,
    sum_true_ranges: f64,
    prev_close: Option<f64>,
}

impl AtrCalculator {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            true_ranges: VecDeque::with_capacity(period),
            sum_true_ranges: 0.0,
            prev_close: None,
        }
    }

    pub fn update(&mut self, bar: &Bar) -> f64 {
        let tr = if let Some(prev_close) = self.prev_close {
            // True Range = max(high - low, |high - prev_close|, |low - prev_close|)
            (bar.high - bar.low)
                .max((bar.high - prev_close).abs())
                .max((bar.low - prev_close).abs())
        } else {
            bar.high - bar.low
        };

        self.prev_close = Some(bar.close);

        if self.true_ranges.len() >= self.period {
            if let Some(old) = self.true_ranges.pop_front() {
                self.sum_true_ranges -= old;
            }
        }
        self.true_ranges.push_back(tr);
        self.sum_true_ranges += tr;

        // Calculate ATR as SMA of true ranges
        if self.true_ranges.is_empty() {
            0.0
        } else {
            self.sum_true_ranges / self.true_ranges.len() as f64
        }
    }

    pub fn value(&self) -> f64 {
        if self.true_ranges.is_empty() {
            0.0
        } else {
            self.sum_true_ranges / self.true_ranges.len() as f64
        }
    }

    pub fn is_ready(&self) -> bool {
        self.true_ranges.len() >= self.period
    }
}
