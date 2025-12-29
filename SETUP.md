# PhaseShifter Setup Guide

Quick setup checklist for running PhaseShifter on a new machine.

## Prerequisites

### Required Software

| Software | Version | Download | Purpose |
|----------|---------|----------|---------|
| **Rust** | Latest stable | https://rustup.rs | Backend server |
| **Node.js** | 18+ | https://nodejs.org | Frontend |
| **Sierra Chart** | Latest | https://sierrachart.com | Live CME data (optional) |

### Optional (for yfinance mode)

| Software | Version | Download | Purpose |
|----------|---------|----------|---------|
| **Python** | 3.9+ | https://python.org | Alternative data source |

## Quick Start Checklist

### 1. Install Rust

```bash
# macOS/Linux
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows - download installer from https://rustup.rs

# Verify
rustc --version
cargo --version
```

### 2. Install Node.js

Download from https://nodejs.org (LTS version recommended)

```bash
# Verify
node --version   # Should be 18+
npm --version
```

### 3. Clone & Build Backend

```bash
cd BACKEND/phaseshifter-server
cargo build --release
```

This will download dependencies and compile. First build takes a few minutes.

### 4. Install Frontend Dependencies

```bash
cd frontend/phaseshifter
npm install
```

### 5. Start the Application

**Terminal 1 - Rust Server:**
```bash
cd BACKEND/phaseshifter-server
cargo run --release -- --symbols NQ,ES --log-level info
```

**Terminal 2 - Frontend:**
```bash
cd frontend/phaseshifter
npm run dev
```

**Browser:**
Open http://localhost:3000

## Platform-Specific Notes

### Windows

- Sierra Chart SCID files default location: `D:\Trading\Sierra\Data\`
- If using Sierra Chart, start the server BEFORE opening Sierra Chart
- The ACSIL study needs to be compiled (see README.md for details)

### macOS/Linux

- No Sierra Chart support (Windows only)
- Use yfinance mode for testing:
  ```bash
  cd BACKEND/server
  pip install -r requirements.txt
  python main.py --mode yfinance
  ```
- Historical SCID files can be copied from Windows machine

## Configuration

### Change WebSocket Port

```bash
# Rust server
cargo run --release -- --ws-port 8080

# Frontend expects port 8000 by default
# Edit frontend/phaseshifter/lib/websocket/ if needed
```

### Change Sierra Data Folder

```bash
cargo run --release -- --sierra-data-folder "/path/to/scid/files"
```

### Change Symbols

```bash
cargo run --release -- --symbols NQ,ES,YM
```

## Data Sources

### Sierra Chart Mode (Primary)

Requirements:
1. Sierra Chart installed and licensed
2. SCID files in data folder (e.g., `NQH26-CME.scid`)
3. ACSIL study compiled and added to chart

Data flow: Sierra Chart -> ACSIL study -> TCP:9000 -> Rust server -> WS:8000 -> Frontend

### yfinance Mode (Alternative)

For testing without Sierra Chart:

```bash
cd BACKEND/server
pip install -r requirements.txt
python main.py --mode yfinance
```

Supported symbols: BTC-USD, QQQ, SPY, AAPL, etc.

## Troubleshooting

### "Connection refused" on startup

- Check ports 8000 and 9000 are not in use
- Start Rust server before frontend

### No historical data

- Verify SCID files exist in the data folder
- Check file naming: `{SYMBOL}-CME.scid` (e.g., `NQH26-CME.scid`)

### Build errors

```bash
# Clean and rebuild
cd BACKEND/phaseshifter-server
cargo clean
cargo build --release
```

### Frontend errors

```bash
# Clear cache and reinstall
cd frontend/phaseshifter
rm -rf node_modules .next
npm install
npm run dev
```

## Ports Used

| Port | Service | Protocol |
|------|---------|----------|
| 3000 | Frontend (Next.js) | HTTP |
| 8000 | WebSocket Server | WS |
| 9000 | Sierra TCP (ACSIL) | TCP |

## File Locations

| File/Folder | Purpose |
|-------------|---------|
| `BACKEND/phaseshifter-server/` | Rust server source |
| `BACKEND/phaseshifter-core/` | Phase engine library |
| `BACKEND/sierra-study/` | Sierra Chart ACSIL study |
| `BACKEND/server/` | Python server (yfinance) |
| `frontend/phaseshifter/` | Next.js frontend |
| `D:\Trading\Sierra\Data\` | Default SCID location (Windows) |

## Useful Commands

```bash
# Run with debug logging
cargo run --release -- --log-level debug

# Type check frontend
cd frontend/phaseshifter && npx tsc --noEmit

# Format Rust code
cargo fmt

# Run Rust tests
cargo test
```
