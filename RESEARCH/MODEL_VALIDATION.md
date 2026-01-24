# PhaseShifter Model Validation Progress

## Overview
Following the 9-step modeling guide to validate the PhaseShifter trading strategy.

**Last Updated:** 2026-01-24
**Current Step:** 1 (Not Started)

---

## Repository Hygiene (research branch)

This section documents exactly what can be committed from `RESEARCH/` and how to keep the branch mergeable into `main`.

### Allowed to Commit (ONLY THESE)
- `RESEARCH/src/**`
- `RESEARCH/ml/**`
- `RESEARCH/Cargo.toml`
- `RESEARCH/Cargo.lock`
- `RESEARCH/MODEL_VALIDATION.md`
- `RESEARCH/SESSION_REPORT.md`

### Must Be Ignored (NEVER COMMIT)
- `RESEARCH/target/**`
- `RESEARCH/data/**`
- `RESEARCH/charts/**`
- `RESEARCH/*.csv`
- `RESEARCH/*.log`
- `.claude/**`
- `server_log.txt`
- Any generated binaries or cache files (`*.exe`, `*.dll`, `*.so`, `*.dylib`, `*.o`, `*.rlib`, `*.a`, `*.pdb`, `*.tmp`, `*.cache`)

### Required .gitignore Snippet (repo root)
```
# Rust build artifacts
/RESEARCH/target/

# Research outputs
/RESEARCH/data/
/RESEARCH/charts/
RESEARCH/*.csv
RESEARCH/*.log

# Local tool state
/.claude/

# Logs
/server_log.txt
*.log

# Binaries / caches
*.exe
*.dll
*.so
*.dylib
*.o
*.rlib
*.a
*.pdb
*.tmp
*.cache
```

### Clean Commit Workflow (run from repo root)
1. `git checkout research`
2. `git reset`
3. Update `.gitignore` with the snippet above
4. `git status --ignored`
5. `git add RESEARCH/src RESEARCH/ml RESEARCH/Cargo.toml RESEARCH/Cargo.lock RESEARCH/MODEL_VALIDATION.md RESEARCH/SESSION_REPORT.md .gitignore`
6. `git status -s` and `git diff --cached`
7. `git commit -m "Add research source code and validation artifacts"`
8. `git status` (must be clean)

---

## Progress Tracker

| Step | Status | Started | Completed | Notes |
|------|--------|---------|-----------|-------|
| 1. Data Acquisition & Prep | ⬜ NOT STARTED | - | - | |
| 2. Dataset Splitting | ⬜ NOT STARTED | - | - | |
| 3. Feature Engineering | ⬜ NOT STARTED | - | - | |
| 4. Model Selection | ⬜ NOT STARTED | - | - | |
| 5. Market Regime Testing | ⬜ NOT STARTED | - | - | |
| 6. Performance Evaluation | ⬜ NOT STARTED | - | - | |
| 7. Equity Curve | ⬜ NOT STARTED | - | - | |
| 8. Monte Carlo Simulation | ⬜ NOT STARTED | - | - | |
| 9. Optimization & Stress Test | ⬜ NOT STARTED | - | - | |

Status Legend: ⬜ NOT STARTED | 🔄 IN PROGRESS | ✅ COMPLETED

---

## Step 1: Data Acquisition and Preparation

### Objectives
- [ ] Verify SCID data integrity (no gaps, proper timestamps)
- [ ] Check for outliers using z-scores
- [ ] Document data coverage (dates, contracts)
- [ ] Verify tick size and price alignment
- [ ] Check volume distribution

### Data Sources
- Sierra Chart SCID files: `D:\Trading\Sierra\Data\`
- Contracts (current MNQ set): MNQU25, MNQZ25, MNQH26

### Deliverables
- Data quality report
- List of any excluded periods
- Summary statistics

### Results
*(To be filled)*

---

## Step 2: Dataset Splitting and Structuring

### Objectives
- [ ] Define train/validation/test periods (chronological, no shuffle)
- [ ] Train: ~70% of data
- [ ] Validation: ~15% of data  
- [ ] Test: ~15% of data (holdout, never touched until final eval)
- [ ] Document exact date boundaries

### Walk-Forward Design
```
|-------- TRAIN --------|--- VAL ---|--- TEST ---|
     70%                    15%          15%
```

### Deliverables
- Date boundaries for each split
- Sample counts per split

### Results
*(To be filled)*

---

## Step 3: Feature Engineering

### Objectives
- [ ] Document all features used in entry decision
- [ ] Check feature distributions (no look-ahead bias)
- [ ] Verify features are available at decision time

### Current Features (from backtest)
Captured at **zone exit** (entry-agnostic):
1. `direction` - Mean-reversion context based on zone vs anchor
2. `exited_above_zone` / `exited_below_zone` - Exit direction
3. `m1_at_exit_is_bullish`, `m5_at_exit_is_bullish` - Bar direction at exit
4. `cluster_width_pct` - Zone width as % of price
5. `cluster_count` / `cluster_unique_scenarios` - Zone strength
6. `zone_to_anchor_distance_pct` - Distance to anchor
7. 80+ snapshot features (returns, volume, delta, volatility, session, MTF, order flow)
8. Post-exit path metrics (max up/down 30m/60m/session, anchor-touch timing, re-entry timing, max zone overshoot)

### Deliverables
- Feature correlation matrix
- Feature importance analysis
- Confirmation of no look-ahead bias

### Results
*(To be filled)*

---

## Step 4: Model Selection and Training

### Objectives
- [ ] Define the trading rules precisely
- [ ] Fit rules on TRAINING data only
- [ ] Tune any parameters on VALIDATION data
- [ ] Document the final model/rules

### Trading Model (ML-Driven)
```
APPROACH:
- NO hardcoded entry/stop rules (avoid data mining / overfitting)
- Capture ALL potential features at ZONE EXIT and let ML discover what matters
- Labels are interaction-only (entry-agnostic)

KEY FEATURES FOR ML TO EVALUATE:
- exit_direction_aligned: Does exit direction match mean-reversion context?
- m1/m5 bar alignment at exit
- Momentum, delta, volume, volatility, session, MTF context (50+ features)
- Post-exit path metrics (30m/60m/session excursions, re-entry timing, anchor-touch timing)

INTERACTION-ONLY LABELS (no PnL yet):
- anchor_touch_30m / anchor_touch_60m / anchor_touch_session
- max_up/down_ticks_30m/60m/session
- time_to_anchor_* and time_to_reentry_ms

ML OBJECTIVES:
1. Find features that best predict favorable post-exit paths
2. Identify confirmations that improve anchor-touch probability
3. Avoid overfitting by testing on holdout data
4. Keep the model interpretable before any trade execution rules
```

### Deliverables
- Final rule specification
- Training set performance
- Validation set performance

### Results
*(To be filled)*

---

## Step 5: Testing Under Different Market Regimes

### Objectives
- [ ] Define regime classification method
- [ ] Segment test data by regime
- [ ] Evaluate performance in each regime

### Regime Definitions
1. **High Volatility**: VIX > 20 or ATR > X
2. **Low Volatility**: VIX < 15 or ATR < Y
3. **Trending Up**: 20-day MA slope > 0
4. **Trending Down**: 20-day MA slope < 0
5. **Sideways**: ADX < 20

### Deliverables
- Performance by regime
- Identification of weak regimes

### Results
*(To be filled)*

---

## Step 6: Performance Evaluation

### Objectives
- [ ] Calculate Sharpe Ratio
- [ ] Calculate Sortino Ratio
- [ ] Calculate Calmar Ratio
- [ ] Calculate Maximum Drawdown
- [ ] Calculate Win Rate and Profit Factor

### Metrics Formulas
- **Sharpe** = (Return - Rf) / StdDev
- **Sortino** = (Return - Rf) / Downside StdDev
- **Calmar** = Annual Return / Max Drawdown
- **Profit Factor** = Gross Profit / Gross Loss

### Deliverables
- Full metrics table for train/val/test
- Comparison across splits

### Results
*(To be filled)*

---

## Step 7: Visualizing the Equity Curve

### Objectives
- [ ] Plot cumulative returns over time
- [ ] Overlay drawdown periods
- [ ] Mark regime changes
- [ ] Compare to buy-and-hold benchmark

### Deliverables
- Equity curve chart (PNG)
- Drawdown chart (PNG)
- Monthly returns heatmap

### Results
*(To be filled)*

---

## Step 8: Monte Carlo Simulation

### Objectives
- [ ] Resample trade returns (with replacement)
- [ ] Generate 10,000 simulated equity paths
- [ ] Calculate confidence intervals for key metrics
- [ ] Determine probability of ruin

### Analysis
- 5th/50th/95th percentile final equity
- Distribution of max drawdowns
- Distribution of Sharpe ratios

### Deliverables
- Monte Carlo distribution plots
- Confidence intervals table
- Probability of >20% drawdown

### Results
*(To be filled)*

---

## Step 9: Optimization and Stress Testing

### Objectives
- [ ] Test parameter sensitivity
- [ ] Simulate extreme scenarios (flash crash, gap opens)
- [ ] Test with higher slippage/commission
- [ ] Verify no overfitting

### Stress Scenarios
1. Double slippage (4 ticks entry, 2 ticks stop)
2. Double commission ($1.24/side)
3. Remove 10% of best trades
4. Add random gaps (1% of trades)

### Deliverables
- Sensitivity analysis table
- Stress test results
- Final robustness assessment

### Results
*(To be filled)*

---

## Final Summary

### Key Findings
*(To be filled after all steps)*

### Recommendation
*(To be filled)*

### Files Generated
- `data/raw_backtest.csv` - Raw backtest output
- `data/train.csv` - Training set
- `data/val.csv` - Validation set
- `data/test.csv` - Test set
- `charts/equity_curve.png` - Equity curve
- `charts/drawdown.png` - Drawdown chart
- `charts/monte_carlo.png` - MC distribution

---

## Session Log

### 2026-01-24
- Switched backtest output to interaction-only labels (post-exit path metrics, no entry/stop)
- Updated ML plan to use anchor-touch and excursion-based targets

### 2026-01-21
- Cleaned up BACKTEST folder (removed old CSVs, unused source files)
- Created MODEL_VALIDATION.md tracking document
- Ready to start Step 1

*(Add new entries when resuming)*
