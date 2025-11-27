# PhaseShifter

## Overview

This repository contains two separate subprojects that live in the same Git repo but are logically independent:

- **BACKEND** – Rust-based CLI engine with supporting Python utilities and local data files.
- **FRONTEND** – Next.js + Prisma app that provides a UI layer and uses a local SQLite database.

There is **no enforced integration** between them yet. You can develop and run each side on its own.

---

## BACKEND

**Location**

- Rust crate: `BACKEND/phaseshifter-core/`
- Data files: `BACKEND/data/`
- Python scripts: `BACKEND/scripts/`
- Backend-specific notes: `BACKEND/README.md`

**Role**

- Read OHLCV or similar data from CSV files in `BACKEND/data/`.
- Process that data using the PhaseShifter engine in `phaseshifter-core/src/`.
- Emit JSONL outputs (e.g. `node_events.jsonl`, `phase_updates.jsonl`) that can be inspected or visualized by other tools.
- Provide helper utilities in Python (e.g. `show_open_nodes.py`) to explore those outputs.

---

## FRONTEND

**Location**

- Next.js app: `FRONTEND/phaseshifter/`
- Python tests/playground: `FRONTEND/testsPython/`

**Role**

- Next.js (App Router) UI with:
  - `app/` – pages, layout, web socket page, API route(s).
  - `components/` – reusable UI components (sidebar, path list, theme toggle, base UI primitives).
  - `hooks/` – custom hooks (e.g. `use-mobile` for responsive behavior).
  - `lib/` – shared helpers and utilities.
  - `prisma/` – Prisma schema, migrations, and local SQLite `dev.db`.
  - `public/` – static images and SVG assets.
- Acts as a standalone frontend that can be wired to any backend or data source later.

---

## Development (Backend)

From the repo root:

```bash
cd BACKEND/phaseshifter-core
cargo run
Use CLI flags as defined in src/config.rs / src/main.rs to point to the desired CSV file and configure the engine.

To inspect outputs with Python:

bash
Copy code
cd BACKEND
python scripts/show_open_nodes.py --help
Adjust arguments (paths, symbol, etc.) as needed.

Development (Frontend)
From the repo root:

bash
Copy code
cd FRONTEND/phaseshifter
npm install
npm run dev
This starts the Next.js dev server (by default on http://localhost:3000).

Prisma files live in FRONTEND/phaseshifter/prisma/:

schema.prisma – data model.

dev.db – local SQLite dev database.

migrations/ – migration history.

You can manage the schema and database with:

bash
Copy code
npx prisma migrate dev
npx prisma studio
Notes
Backend and frontend are intentionally kept separate. You can decide later how (or whether) to connect them (HTTP, WebSocket, file-based, etc.).

The root .gitignore is configured for Rust, Node/Next.js, Python, Prisma, and common editor/OS junk.
