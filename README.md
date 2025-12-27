# PhaseShifter

Real-time trading analysis platform that identifies price phase shifts and projects target prices using Donchian midpoint calculations. Streams live market data from Sierra Chart, detects phase transitions, creates projection nodes, and clusters them across multiple timeframe scenarios.

![Architecture](https://img.shields.io/badge/Stack-Rust%20%7C%20Next.js%20%7C%20WebSocket-blue)
![Real-Time](https://img.shields.io/badge/Data-Live%20TCP%20Stream-green)
![CME Data](https://img.shields.io/badge/CME-Via%20ACSIL-orange)

## Table of Contents

- [Features](#features)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [How It Works](#how-it-works)
- [Running Modes](#running-modes)
- [Configuration](#configuration)
- [Project Structure](#project-structure)
- [Development](#development)
- [Troubleshooting](#troubleshooting)
- [License](#license)

## Features

- **Real-Time Streaming**: Live tick data from Sierra Chart via custom ACSIL study
- **Multi-Timeframe Analysis**: Simultaneous phase detection across M1, M5, M15 timeframes
- **Phase Detection**: Donchian midpoint-based bullish/bearish phase identification
- **Node Projections**: Dynamic price targets based on phase extremes
- **Cluster Analysis**: Anchor-normalized adaptive-gap clustering across scenarios
- **Auto Contract Detection**: CME futures front-month contract rolling
- **Historical Data**: SCID file loading from Sierra Chart for backtesting
- **yfinance Support**: Alternative data source for non-CME symbols (BTC-USD, QQQ, SPY, etc.)

## Quick Start

### Prerequisites

1. **Sierra Chart** with ACSIL compiler (Visual Studio Build Tools)
2. **Rust** (latest stable) - https://rustup.rs
3. **Node.js** 18+ and npm
4. **Python 3.9+** (optional, for yfinance mode)

### Option A: Sierra Chart Mode (CME Futures - NQ, ES, YM)

**1. Install ACSIL Study**
```bash
# Copy the study to Sierra Chart
copy BACKEND\sierra-study\PhaseShifterStream.cpp "D:\Trading\Sierra\ACS_Source\"

# In Sierra Chart: Analysis -> Build Custom Studies DLL
# Add to chart: Analysis -> Studies -> Add Custom Study -> PhaseShifter Stream
```

**2. Start Rust Server**
```bash
cd BACKEND/phaseshifter-server
cargo run --release -- --symbols NQ,ES
```
Server will:
- Load ~50 days of historical data from SCID files
- Listen for ACSIL study on TCP port 9000
- Serve WebSocket on port 8000

**3. Start Sierra Chart**
- Open chart for NQH26-CME (or current front-month)
- ACSIL study background turns green when connected
- Live ticks stream automatically

**4. Start Frontend**
```bash
cd frontend/phaseshifter
npm install  # first time only
npm run dev
```

**5. Open Browser**
- Navigate to http://localhost:3000
- Charts show historical data + live updates

### Option B: yfinance Mode (BTC-USD, QQQ, SPY, etc.)

For symbols not available through Sierra Chart:

```bash
# Terminal 1: Start Python server
cd BACKEND/server
pip install -r requirements.txt  # first time only
python main.py --mode yfinance

# Terminal 2: Start Frontend
cd frontend/phaseshifter
npm run dev

# Browser: http://localhost:3000
# Subscribe to: BTC-USD, QQQ, SPY, AAPL, etc.
```

## Architecture

### High-Level Data Flow

```
+------------------+     +------------------+     +------------------+
|   Sierra Chart   |     |   Rust Server    |     |    Frontend      |
|                  |     |                  |     |                  |
|  NQH26-CME chart |     | TCP:9000  WS:8000|     |  Next.js React   |
|       |          |     |       |          |     |       |          |
|  ACSIL Study     |---->| sierra_tcp.rs    |---->| TradingView      |
|  (live ticks)    | TCP | BarBuilder       | WS  | Charts           |
+------------------+     | PhaseEngine      |     +------------------+
                         | ClusterManager   |
+------------------+     +------------------+
|   SCID Files     |          |
|  (historical)    |----------+
+------------------+   startup load
```

### Sierra Chart Mode (Primary)

```
Sierra Chart Chart (NQH26-CME)
    |
    v
ACSIL Study (PhaseShifterStream.cpp)
    | hash symbol, stream live ticks
    v
TCP Socket (127.0.0.1:9000)
    | 32-byte binary messages
    v
Rust Server (sierra_tcp)
    | parse ticks, map symbol_id -> name
    v
BarBuilder
    | construct M1, M5, M15 bars
    | also loads historical from SCID files
    v
PhaseEngine (4 scenarios)
    | detect phases, create nodes
    | M1/pw50, M5/pw7, M5/pw20, M15/pw10
    v
ClusterManager
    | track nodes, close on target hit
    | cluster remaining open nodes
    v
WebSocket Server (port 8000)
    | broadcast ticks, bars, phases, nodes, clusters
    v
Frontend Charts (Next.js + TradingView)
```

### yfinance Mode (Alternative)

```
yfinance API
    |
    v
Python Server (data_streamer.py)
    | poll every 60 seconds
    | cache candles
    v
WebSocket Server (ws_server.py)
    | broadcast candles
    v
Pipeline Runner (pipeline_runner.py)
    | run phase analysis every 5 minutes
    | call Rust core + show_open_nodes.py
    v
Frontend Charts (same UI)
```

## How It Works

### Phase Detection

The core algorithm uses the **Donchian Midpoint (DM)** to detect market phases:

```
DM = (highest_close + lowest_close) / 2 over N periods

Rising DM  -> Bullish phase
Falling DM -> Bearish phase
```

### Node Creation

When the phase flips, a **node** (projection target) is created:

1. **Freeze the extreme**: Capture the high (bullish) or low (bearish) from the previous phase
2. **Calculate distance**: `distance_pct = (extreme - anchor) / anchor`
3. **Project target**: As the anchor updates, target = `anchor * (1 +/- distance_pct)`

**Example:**
```
Price in bullish phase: 18000 -> 18500 (anchor: 18000, extreme: 18500)
Phase flips to bearish
-> Creates bullish node: distance_pct = (18500-18000)/18000 = 2.78%
-> If anchor moves to 18200, target = 18200 * 1.0278 = 18706
```

### Node Lifecycle

```
1. CREATED     - Phase flip triggers node creation
                 distance_pct frozen at creation
                 status = Open

2. PROJECTED   - Target updates with each bar
                 target = current_anchor * (1 +/- distance_pct)

3. CLOSED      - Price hits target
                 Bullish: high >= target
                 Bearish: low <= target
                 status = Closed

4. CLUSTERED   - Open nodes grouped by similar projections
                 Requires nodes from >= 2 scenarios
```

### Clustering Algorithm

Open nodes from multiple scenarios are clustered using anchor-normalized adaptive-gap clustering:

1. Collect all open nodes from all scenarios
2. Sort by projected price
3. Calculate gaps between consecutive nodes
4. Merge nodes within adaptive threshold based on volatility
5. Output clusters with: side, low, high, count, unique_scenarios

## Running Modes

### Mode Comparison

| Feature | Sierra Chart Mode | yfinance Mode |
|---------|------------------|---------------|
| Symbols | NQ, ES, YM (CME) | BTC-USD, QQQ, SPY, etc. |
| Data Source | ACSIL TCP stream | yfinance API |
| Update Speed | Real-time ticks | 60-second polling |
| Historical | SCID files | yfinance history |
| Server | Rust | Python |
| Clustering | Rust ClusterManager | Python pipeline |

### Sierra Chart Mode Commands

```bash
# Start Rust server with default symbols
cd BACKEND/phaseshifter-server
cargo run --release -- --symbols NQ,ES

# With all options
cargo run --release -- \
  --symbols NQ,ES,YM \
  --ws-port 8000 \
  --sierra-tcp-port 9000 \
  --sierra-data-folder "D:\Trading\Sierra\Data" \
  --log-level info
```

### yfinance Mode Commands

```bash
# Start Python server
cd BACKEND/server
python main.py --mode yfinance

# With options
python main.py \
  --mode yfinance \
  --host localhost \
  --port 8000 \
  --poll-interval 60 \
  --pipeline-interval 300
```

### Historical Analysis (Optional)

For batch analysis without live streaming:

```bash
pip install pandas yfinance

# Single symbol analysis
python run_phase_pipeline.py BTC-USD

# Output: nodes.txt with clustered projections
```

## Configuration

### Rust Server Options

| Option | Default | Description |
|--------|---------|-------------|
| `--symbols` | `NQ,ES` | Comma-separated base symbols |
| `--ws-port` | `8000` | WebSocket server port |
| `--sierra-tcp-port` | `9000` | TCP port for ACSIL study |
| `--sierra-data-folder` | `D:\Trading\Sierra\Data` | SCID file location |
| `--log-level` | `info` | Logging verbosity (debug, info, warn, error) |

### Python Server Options

| Option | Default | Description |
|--------|---------|-------------|
| `--mode` | `dtc` | Data mode: `yfinance` or `dtc` |
| `--host` | `localhost` | WebSocket host |
| `--port` | `8000` | WebSocket port |
| `--poll-interval` | `60` | yfinance polling interval (seconds) |
| `--pipeline-interval` | `300` | Cluster recalculation interval (seconds) |

### Scenario Configuration

Default scenarios (edit in `server.rs` for Rust, `pipeline_runner.py` for Python):

| Timeframe | Phase Window | Description |
|-----------|--------------|-------------|
| M1 | 50 | Fast, 50-period on 1-minute bars |
| M5 | 7 | Medium, 7-period on 5-minute bars |
| M5 | 20 | Medium, 20-period on 5-minute bars |
| M15 | 10 | Slow, 10-period on 15-minute bars |

### Frontend Configuration

WebSocket URL in `frontend/phaseshifter/lib/websocket/`:
```typescript
// Default connects to localhost:8000
// Both Rust and Python servers use same port
```

### Sierra Chart Setup

1. **Install ACSIL Study:**
   - Copy `BACKEND/sierra-study/PhaseShifterStream.cpp` to `Sierra Chart\ACS_Source\`
   - In Sierra Chart: Analysis -> Build Custom Studies DLL
   - Add to chart: Analysis -> Studies -> Add Custom Study -> PhaseShifter Stream

2. **Study Settings:**
   - Server Port: 9000
   - Log Interval: 10000 (logs every 10k ticks)

3. **Chart Setup:**
   - Use front-month contract: NQH26-CME, ESH26-CME, etc.
   - Green background = connected to Rust server
   - Red background = waiting for server

4. **SCID Files:**
   - Location: `D:\Trading\Sierra\Data\{symbol}-CME.scid`
   - Server loads ~5 million records on startup

## Project Structure

```
PhaseShifter/
|-- BACKEND/
|   |-- phaseshifter-server/          # Rust real-time server
|   |   |-- src/
|   |   |   |-- main.rs               # CLI entry point
|   |   |   |-- server.rs             # WebSocket server + coordinator
|   |   |   |-- sierra_tcp.rs         # TCP listener for ACSIL
|   |   |   |-- bar_builder.rs        # Multi-timeframe bar construction
|   |   |   |-- clusters.rs           # Node tracking + clustering
|   |   |   |-- scid.rs               # SCID file reader (historical)
|   |   |   |-- contracts.rs          # Auto contract detection
|   |   |   '-- dtc/                  # DTC protocol (unused for CME)
|   |   '-- Cargo.toml
|   |
|   |-- phaseshifter-core/            # Rust phase engine library
|   |   |-- src/
|   |   |   |-- lib.rs                # PhaseEngine core
|   |   |   |-- nodes.rs              # Node state management
|   |   |   |-- config.rs             # Configuration
|   |   |   '-- csv_source.rs         # CSV data loading
|   |   '-- Cargo.toml
|   |
|   |-- sierra-study/                 # Sierra Chart ACSIL study
|   |   '-- PhaseShifterStream.cpp    # Live tick streaming via TCP
|   |
|   |-- server/                       # Python server (yfinance mode)
|   |   |-- main.py                   # Server entry point
|   |   |-- ws_server.py              # WebSocket server
|   |   |-- data_streamer.py          # yfinance data polling
|   |   |-- pipeline_runner.py        # Periodic cluster analysis
|   |   |-- bar_builder.py            # Python bar construction
|   |   '-- requirements.txt
|   |
|   '-- data/                         # JSONL outputs (git-ignored)
|
|-- frontend/phaseshifter/            # Next.js React frontend
|   |-- app/
|   |   |-- normalChart/page.tsx      # Main chart page
|   |   |-- webSocket/page.tsx        # Debug page
|   |   '-- api/                      # API routes
|   |-- lib/
|   |   |-- websocket/                # WebSocket client
|   |   |   |-- types.ts              # Message type definitions
|   |   |   |-- TradingDataContext.tsx # Data state management
|   |   |   '-- index.ts              # Public API
|   |   '-- chart/                    # Chart utilities
|   |-- components/                   # UI components
|   '-- package.json
|
|-- run_phase_pipeline.py             # Historical analysis script
'-- README.md                         # This file
```

## Development

### Building

```bash
# Rust server (optimized)
cd BACKEND/phaseshifter-server
cargo build --release

# Rust core library
cd BACKEND/phaseshifter-core
cargo build --release

# Frontend
cd frontend/phaseshifter
npm run build
```

### Testing

```bash
# Rust tests
cd BACKEND/phaseshifter-core
cargo test

cd BACKEND/phaseshifter-server
cargo test

# Frontend type check
cd frontend/phaseshifter
npx tsc --noEmit
npm run lint
```

### Code Formatting

```bash
# Rust
cargo fmt

# Frontend
npm run lint -- --fix
```

### Debug Logging

```bash
# Rust server with debug output
cargo run -- --log-level debug

# Check WebSocket messages
# Navigate to http://localhost:3000/webSocket
```

## Troubleshooting

### ACSIL Study Issues

| Problem | Solution |
|---------|----------|
| Won't compile | Ensure `WIN32_LEAN_AND_MEAN` and `_WINSOCKAPI_` defined before includes |
| Shows red (not connected) | Start Rust server first, check port 9000 not blocked |
| No ticks streaming | Check market is open, verify green background indicator |

### Rust Server Issues

| Problem | Solution |
|---------|----------|
| No historical data | Verify SCID files exist in `D:\Trading\Sierra\Data\` |
| Symbol format errors | Server expects base symbols (NQ), auto-converts to NQH26-CME |
| Port already in use | Check no other process on ports 8000 or 9000 |

### Frontend Issues

| Problem | Solution |
|---------|----------|
| "Waiting for data" | Verify server running, check browser console for WS errors |
| No clusters | Wait for phase flips, clusters need nodes from 2+ scenarios |
| Wrong data | Check you're subscribed to correct symbol |

### Python Server Issues

| Problem | Solution |
|---------|----------|
| yfinance errors | Check internet connection, rate limiting |
| No clusters | Wait for pipeline run (every 5 min by default) |
| Import errors | Run `pip install -r requirements.txt` |

### General Debug Steps

1. Check server logs for errors
2. Open http://localhost:3000/webSocket for raw message inspection
3. Verify correct ports: 8000 (WebSocket), 9000 (Sierra TCP)
4. For Sierra: check Message Log in Sierra Chart

## WebSocket Protocol

### Client -> Server

```json
{"type": "subscribe", "ticker": "NQ"}
{"type": "unsubscribe", "ticker": "NQ"}
{"type": "get_clusters", "ticker": "NQ"}
{"type": "set_auto_clusters", "ticker": "NQ", "enabled": true}
```

### Server -> Client

```json
{"type": "connected", "client_id": "abc123", "symbols": ["NQ", "ES"]}
{"type": "tick", "symbol": "NQ", "price": 18500.25, "volume": 10, "timestamp": 1703...}
{"type": "bar_update", "symbol": "NQ", "timeframe": "M5", "open": 18500, "high": 18510, ...}
{"type": "bar_closed", "symbol": "NQ", "timeframe": "M5", ...}
{"type": "phase_update", "symbol": "NQ", "timeframe": "M5", "phase": "bullish", "anchor": 18500, "dm": 18505}
{"type": "node_created", "symbol": "NQ", "timeframe": "M5", "direction": "bullish", "distance_pct": 0.0278, ...}
{"type": "clusters", "ticker": "NQ", "anchor": 18500, "clusters": [...], "nodes": [...]}
```

## Contributing

1. Format Rust code: `cargo fmt`
2. Run tests: `cargo test`
3. Check frontend types: `npx tsc --noEmit`
4. Test with live data before committing

## License

MIT License - See LICENSE file for details
