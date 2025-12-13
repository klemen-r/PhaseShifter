# PhaseShifter

Rust PhaseShifter engine plus a standalone Next.js UI, living in one repo but buildable and runnable independently.

## Repo layout

```
BACKEND/                 Rust core + data and analysis scripts
  phaseshifter-core/     Rust crate with the PhaseShifter engine
  data/                  Sample CSVs + helpers (download, JSONL outputs)
  scripts/               Python utilities (e.g., show_open_nodes.py)
FRONTEND/                UI and playgrounds
  phaseshifter/          Next.js + Prisma app (local SQLite dev.db)
  testsPython/           Misc. Python experiments
run_phase_pipeline.py    Helper to fetch data, run multiple configs, append nodes.txt
default.txt, nodes.txt   Local outputs (ignored by Git)
```

## Prerequisites

- Rust (stable) with `cargo`
- Python 3.10+ for data helpers (`pip install pandas yfinance` for downloads)
- Node.js 18+ (20+ recommended) with `npm`

## Backend quickstart

Example run against the bundled sample data:

```bash
cd BACKEND/phaseshifter-core
cargo run -- --csv ../data/nq_1h.csv --phase-window 7 --depth-days 50 --timeframe 1h \
  --out-json ../data/phase_updates.jsonl --node-events-log ../data/node_events.jsonl
```

- Defaults for `--csv`, `--out-json`, and `--node-events-log` already point at `../data`, so you can omit them once you’re comfortable.
- A per-run TOML config can be supplied with `--config <path>`; CLI flags still override the file.
- The engine writes JSONL logs to `BACKEND/data/` (git-ignored).

Inspect open nodes from those outputs:

```bash
cd BACKEND
python scripts/show_open_nodes.py --nodes data/node_events.jsonl --phases data/phase_updates.jsonl --symbol NQ=F
```

Batch pipeline (fetch data via yfinance, run multiple configs, and append `nodes.txt`):

```bash
pip install pandas yfinance
python run_phase_pipeline.py NQ=F
# allow single-scenario clusters (skip cross-timeframe validation)
python run_phase_pipeline.py BTC-USD --allow-single-scenario
```

- Pipeline clusters are now anchor-normalized and adaptive-gap based: a common anchor is inferred from all nodes, deviations are clustered per side with percentile seed gaps, scale-based pruning/splitting, width limits, and overlap dedupe (prefers narrower zones, then scenario breadth, then count).

## Frontend quickstart (Next.js + Prisma)

```bash
cd FRONTEND/phaseshifter
npm install
npm run dev         # http://localhost:3000
npm run lint        # quick health check
# npm run build && npm start for a production-like run
```

- Uses the local SQLite database at `prisma/dev.db`. If you change the schema, run `npx prisma migrate dev`. Use `npx prisma studio` to inspect data.
- No backend wiring is enforced yet; the UI can be pointed at any future API.

## Notes for pushing to GitHub

- Outputs under `BACKEND/data/` and root `nodes.txt` are ignored; keep them for local exploration only.
- Run a quick hygiene pass before pushing:
  - `cargo fmt && cargo test` inside `BACKEND/phaseshifter-core`
  - `npm run lint` (and optionally `npm run build`) inside `FRONTEND/phaseshifter`
- Keep backend/frontend changes in separate commits where possible; they are maintained independently today.
