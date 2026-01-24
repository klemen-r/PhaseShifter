# ML Filter Analysis Findings

> NOTE (2026-01-24): Backtest output has been switched to interaction-only labels.
> This file reflects the older trade-based NQ analysis and needs a fresh run
> on the new interaction dataset.

## Executive Summary

After analyzing 16,875 zone interactions across 3 years of NQ data, **no robust entry filter produces consistent positive expectancy for both long and short trades**.

## Key Findings

### 1. Baseline Strategy Has Negative Expectancy

| Metric | Value |
|--------|-------|
| Total trades | 16,875 |
| Win rate | 7.0% |
| Avg RR of winners | 10.2 |
| Gross EV | -0.23R |
| Costs | ~0.47R |
| **Net EV** | **-0.64R** |

### 2. The RR Paradox

The strategy's RR ratio (zone-to-anchor distance / risk) averages ~33, but **winners only average 10 RR** because:
- Most wins come from low-RR setups (46% of wins have RR < 5)
- High-RR setups rarely hit target (RR>20 has <3% win rate)

| RR Bucket | Win Rate | Gross EV | Net EV |
|-----------|----------|----------|--------|
| RR < 2 | 60% | +0.19R | -0.58R |
| RR 2-5 | 26% | +0.11R | -0.45R |
| RR 5-10 | 10% | -0.17R | -0.64R |
| RR 10-20 | 5% | -0.25R | -0.69R |
| RR 20-50 | 2% | -0.33R | -0.81R |
| RR > 50 | <1% | -0.28R | -0.84R |

**Insight:** Low RR has small positive gross edge, but costs (~0.5R) wipe it out.

### 3. Best Filters Found

The only filter with positive net EV across train/val/test:

**`delta_supports_trade + htf_trend_aligned + session_mid_zone` (SHORTS ONLY)**
- 82 trades over 3 years (~2.4/month)
- 22% win rate
- +1.7R net EV
- But LONGS are negative, SHORTS carry all the profit

This is likely **overfitting to a bearish regime**, not a robust edge.

### 4. What Didn't Work

Filters tested with no consistent positive EV:
- `momentum_aligned`
- `bars_aligned` 
- `alignment_score >= 5`
- `high_vol_environment`
- `volume_spike_ratio > 1.5`
- Various RR thresholds
- All pairwise combinations

### 5. Root Cause Analysis

The problem isn't filtering - it's the **strategy mechanics**:

1. **Costs are too high relative to edge**
   - Round-trip costs: ~0.5R (slippage + commission)
   - Best gross edge found: ~0.2R
   - Net: always negative

2. **Target selection issue**
   - Anchor (Donchian midpoint) is often far away
   - Price rarely reaches it before stop
   - Need closer targets or trailing exits

3. **Stop placement issue**
   - Swing-based stops may be too tight
   - Getting stopped out before move completes

## Recommendations

### Option A: Fix the Strategy
1. **Reduce costs**: Tighter execution, better fills
2. **Closer targets**: Consider 1R or 2R targets instead of anchor
3. **Wider stops**: Trade off win rate for better R:R
4. **Trailing stops**: Let winners run, capture more R

### Option B: Different Approach
1. **Zone alerts only**: Use clusters as context, not mechanical entries
2. **Discretionary overlay**: Human judgment for entry timing
3. **Different instruments**: Lower cost instruments (ES vs NQ)

## Files Generated

- `ml/train.csv` - 11,812 trades (2023-02 to 2025-02)
- `ml/val.csv` - 2,531 trades (2025-02 to 2025-05)
- `ml/test.csv` - 2,532 trades (2025-05 to 2026-01)
- `ml/step*.py` - Analysis scripts

## Conclusion

**The zone clustering identifies price levels, but the mechanical entry/exit rules don't capture enough edge to overcome costs.** The ML analysis successfully avoided overfitting by testing across train/val/test splits and requiring both directions to work.

The strategy needs fundamental changes to target selection or cost structure before ML filtering can add value.
