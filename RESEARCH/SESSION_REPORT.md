# Session Report (2026-01-24)

## Summary
- Shifted the backtest from trade/outcome modeling to interaction-only labeling for ML.
- Added post-exit path metrics (30m/60m/RTH close), re-entry timing, and bidirectional excursions.
- Aligned ML prep scripts and validation doc with the interaction-only workflow.

## Goals
- Remove hardcoded entry/stop assumptions so ML can learn confirmations.
- Use zone-exit as the decision anchor and measure mean-reversion behavior toward the anchor.
- Keep confirmation features on <=5m timeframes and cap post-exit windows at RTH close.

## Key Decisions
- Interaction labels are based on post-exit price path (no PnL, no stops).
- Direction context stays mean-reversion (zone above anchor = short context; below = long).
- RTH close defines the max window for post-exit tracking.

## Implementation Details
### Backtest Core
- Replaced trade lifecycle with interaction-only lifecycle:
  - Track zone entry → zone exit → post-exit path windows.
  - Record excursions, overshoots beyond zone, anchor-touch timing, re-entry timing.
- New InteractionRecord schema replaces trade-based FeatureRecord.
- Logging now reports anchor-touch rates and time-in-zone.

### Feature + Session Context
- MTF phase updates limited to 15s/1m/5m (confirmation scale).
- Default HTF reference changed to M5/20 for <=5m context.
- Added RTH close accessor for window capping.

### ML Prep
- step1_data_prep.py rewritten for interaction-only dataset (zone_exit_time driven).
- step2_split_data.py rewritten to split on zone_exit_time and summarize anchor-touch metrics.
- step3_feature_analysis.py target switched to anchor_touch_60m.
- FINDINGS.md flagged as legacy (trade-based NQ analysis).

### Documentation
- MODEL_VALIDATION.md updated for interaction-only workflow and current MNQ contracts.

## Repository Hygiene (research branch)
To keep `research` mergeable into `main`, only source and documentation files are committed from `RESEARCH/`.

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

## Files Changed
- BACKTEST/src/main.rs
- BACKTEST/src/feature_extractor.rs
- BACKTEST/src/session_tracker.rs
- BACKTEST/src/mtf_tracker.rs
- BACKTEST/ml/step1_data_prep.py
- BACKTEST/ml/step2_split_data.py
- BACKTEST/ml/step3_feature_analysis.py
- BACKTEST/ml/FINDINGS.md
- BACKTEST/MODEL_VALIDATION.md

## Behavior Changes
- Output CSV now contains interaction-only labels and post-exit path metrics.
- No wins/losses/PNL/stop logic.
- Confirmation features only from <=5m timeframes.

## Tests Run
- cargo check (BACKTEST)

## Commands Attempted
- cargo run --release -- --symbol MNQ --data-dir "D:\Trading\Sierra\Data" --output "zone_interactions.csv"
  - Aborted by user.

## Next Steps
- Run the interaction-only backtest for MNQ to regenerate the dataset.
- Execute ML steps 1–3 on the new dataset.
- Update remaining ML scripts that still assume anchor_hit/outcome_r if needed.
