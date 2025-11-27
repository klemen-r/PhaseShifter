# PhaseShifter (backend only)

Quick and messy summary:
- Rust crate `phaseshifter-core/` holds the engine.
- Takes csv data and tracks phases, anchor and shift + nodes (JSONL outputs for phase updates and node events).
- Sample data lives in `data/es_1h.csv`.

How to run:
1. `cd phaseshifter-core`
2. `cargo run -- --csv ../data/<your_csv>.csv --phase-window <N> --depth-days <D> --timeframe <label> --out-json ../data/phase_updates.jsonl --node-events-log ../data/node_events.jsonl`
   - Defaults still write phase updates to `../data/phase_updates.jsonl` and node events to `../data/node_events.jsonl` if you omit the last two flags.
   - You can pass any CSV path under `../data/` (e.g., `../data/es_1h.csv`, `../data/volx_5m.csv`).
   - Be explicit with the `../data/` prefix; otherwise a bare `phase_updates.jsonl` will land in `phaseshifter-core/` (not desired).
3. From the repo root, inspect open nodes (uses the same files written above): `python scripts/show_open_nodes.py --nodes data/node_events.jsonl --phases data/phase_updates.jsonl --symbol <SYMBOL>`
   - If you run the script while inside `phaseshifter-core/`, point to the parent paths: `python ../scripts/show_open_nodes.py --nodes ../data/node_events.jsonl --phases ../data/phase_updates.jsonl --symbol <SYMBOL>`

Notes:
- No frontend here. You'll need to search elsewhere :(
- Logs/outputs (`*.jsonl`, `*.log`, `stdout.json`) are ignored by Git. Build junk is under `target/`.

( . Y . )
