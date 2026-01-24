"""
Step 4n: Features Needed for DOM/Footprint Entry
=================================================
What we need to add to the backtest for proper order flow entries.
"""

print("""
========================================================================
ORDER FLOW FEATURES NEEDED FOR DOM/FOOTPRINT STYLE ENTRIES
========================================================================

CURRENTLY HAVE:
- cumulative_delta_1m: Net delta over last minute
- cumulative_delta_5m: Net delta over 5 minutes
- delta_supports_trade: Delta agrees with trade direction (bool)
- delta_divergence: Delta disagrees with price (bool)
- volume_spike_ratio: Current vs average volume
- large_trade_count_1m: Count of 50+ lot trades

MISSING (need to add to VolumeTracker):

1. DELTA FLIP
   - Track previous delta sign
   - Flag when delta crosses zero
   - Time since last flip
   - For entries: "delta just flipped to support our direction"

   Implementation:
   ```rust
   pub fn delta_flipped_bullish(&self) -> bool {
       // Was negative, now positive
       self.prev_delta_5m < 0 && self.sum_delta_5m > 0
   }

   pub fn delta_flipped_bearish(&self) -> bool {
       // Was positive, now negative
       self.prev_delta_5m > 0 && self.sum_delta_5m < 0
   }
   ```

2. ABSORPTION DETECTION
   - High volume but price range is tight
   - Track bar-by-bar: volume vs range
   - Flag when volume >> normal but range << normal

   Implementation:
   ```rust
   pub fn absorption_detected(&self, bar_range: f64, avg_range: f64) -> bool {
       let vol_ratio = self.volume_spike_ratio();
       let range_ratio = bar_range / avg_range;
       vol_ratio > 2.0 && range_ratio < 0.5  // High vol, tight range
   }
   ```

3. DELTA EXHAUSTION
   - Delta was strong, now weakening
   - Potential reversal signal

   Implementation:
   ```rust
   pub fn delta_exhaustion(&self) -> bool {
       // 1m delta reversing vs 5m delta
       let delta_1m = self.sum_delta_1m;
       let delta_5m = self.sum_delta_5m;
       // 5m positive but 1m turning negative = exhaustion
       (delta_5m > 100 && delta_1m < -50) ||
       (delta_5m < -100 && delta_1m > 50)
   }
   ```

4. IMBALANCE STACKING
   - Consecutive aggressive buying/selling
   - Track ask_volume vs bid_volume ratio per tick
   - Flag when N consecutive ticks are imbalanced

   Implementation:
   ```rust
   pub fn imbalance_streak(&self) -> i32 {
       // Count consecutive bullish/bearish imbalances
       // Positive = buying streak, negative = selling streak
   }
   ```

5. LARGE TRADE DIRECTION
   - Not just count, but direction of large trades
   - Are big players buying or selling?

   Implementation:
   ```rust
   pub fn large_trade_delta_1m(&self) -> i64 {
       // Sum of delta for trades >= threshold only
   }
   ```

========================================================================
RECOMMENDED BACKTEST CHANGES
========================================================================

1. Add to VolumeTracker:
   - prev_delta_5m (for flip detection)
   - delta_flip_time_ms (when last flip occurred)
   - large_trade_delta (directional large trades)

2. Add to FeatureExtractor / FeatureSnapshot:
   - delta_just_flipped_bullish: bool
   - delta_just_flipped_bearish: bool
   - ms_since_delta_flip: i64
   - absorption_bar: bool (from bar data)
   - delta_exhaustion: bool
   - large_trade_delta_1m: i64

3. Add to backtest main.rs:
   - Capture these at signal time
   - Add to FeatureRecord for CSV output

========================================================================
ALTERNATIVE: Derive from existing data
========================================================================

Can approximate some signals without code changes:

1. DELTA FLIP PROXY:
   - delta_near_zero (|delta_5m| < 100) + delta_supports_trade
   - Means delta is around zero and pointing our way
   - Already shows 9.8% WR (vs 7% baseline)

2. ABSORPTION PROXY:
   - volume_spike_ratio > 2 AND ret_1m.abs() < 0.0002
   - High volume, no price move
   - Shows 9.4% WR

3. DELTA STRENGTH:
   - high_delta_supports (|delta_5m| > 500 AND supports)
   - Shows 10.8% WR, -0.16R EV (best EV found)

========================================================================
CONCLUSION
========================================================================

Current data can get us to ~12-15% WR with right filters.
To get DOM/footprint quality entries, need delta flip detection.

RECOMMENDED PATH:
1. Run live with current filters (delta_sup + vol + bars)
2. Add delta flip tracking to VolumeTracker
3. Re-run backtest with new features
4. Test if delta flip timing improves entries

""")
