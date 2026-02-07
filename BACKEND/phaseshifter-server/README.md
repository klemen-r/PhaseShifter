# PhaseShifter Real-Time Streaming Server

High-performance Rust server that listens to Sierra Chart via the ACSIL TCP study and streams real-time market data to the frontend via WebSocket.

## Features

- **ACSIL TCP Integration**: Receives real-time tick data from the PhaseShifterStream study
- **Auto Contract Detection**: Automatically detects front-month futures contracts (NQ → NQH26, ES → ESH26, etc.)
- **Multi-Timeframe Bar Building**: Constructs OHLCV bars for M1, M5, M15, H1 timeframes from tick data
- **PhaseEngine Integration**: Real-time phase detection and node creation using the phaseshifter-core engine
- **WebSocket Broadcasting**: Streams ticks, bars, phase updates, and nodes to connected clients
- **Nanosecond Startup Timer**: Logs initialization time with nanosecond precision for performance monitoring

## Usage

```bash
# Build the server
cargo build --release

# Run with default settings (ACSIL TCP on localhost:9000, serves WebSocket on localhost:8000)
cargo run --release

# Run with custom settings
cargo run --release -- \
  --ws-host 127.0.0.1 \
  --ws-port 8000 \
  --sierra-tcp-port 9000 \
  --sierra-data-folder "D:\\Trading\\Sierra\\Data" \
  --symbols NQ,ES \
  --log-level info
```

## Command-Line Arguments

| Argument | Default | Description |
|----------|---------|-------------|
| `--ws-host` | `127.0.0.1` | WebSocket server bind address |
| `--ws-port` | `8000` | WebSocket server port |
| `--sierra-tcp-port` | `9000` | ACSIL TCP port for live ticks |
| `--sierra-data-folder` | `D:\Trading\Sierra\Data` | SCID file location |
| `--symbols` | `NQ,ES` | Comma-separated base symbols (e.g., "NQ,ES,YM") |
| `--log-level` | `info` | Log level: error, warn, info, debug, trace |

## Contract Auto-Detection

The server automatically detects front-month futures contracts based on CME's quarterly expiration cycle (H=March, M=June, U=September, Z=December). It rolls to the next contract approximately 10 days into the expiry month.

Examples:
- `NQ` → `NQH26` (March 2026)
- `ES` → `ESH26` (March 2026)
- `YM` → `YMH26` (March 2026)

## WebSocket Protocol

The server broadcasts the following message types to connected clients:

### Connection
```json
{
  "type": "connected",
  "client_id": 1,
  "symbols": ["NQ", "ES"]
}
```

### Tick Data
```json
{
  "type": "tick",
  "symbol": "NQ",
  "price": 18500.50,
  "volume": 5.0,
  "timestamp": 1704067200000,
  "bid": 18500.25,
  "ask": 18500.75
}
```

### Bar Updates
```json
{
  "type": "bar_update",
  "symbol": "NQ",
  "timeframe": "1m",
  "time": 1704067200000,
  "open": 18500.00,
  "high": 18501.00,
  "low": 18499.50,
  "close": 18500.50,
  "volume": 125.0
}
```

### Bar Closed
```json
{
  "type": "bar_closed",
  "symbol": "NQ",
  "timeframe": "1m",
  "time": 1704067200000,
  "open": 18500.00,
  "high": 18501.00,
  "low": 18499.50,
  "close": 18500.75,
  "volume": 150.0
}
```

### Phase Updates
```json
{
  "type": "phase_update",
  "symbol": "NQ",
  "timeframe": "5m",
  "time": 1704067500000,
  "phase": "bullish",
  "anchor": 18495.25,
  "dm": 18496.50
}
```

### Node Created
```json
{
  "type": "node_created",
  "symbol": "NQ",
  "timeframe": "5m",
  "direction": "bullish",
  "distance_pct": 0.0025,
  "anchor": 18495.25,
  "extreme": 18541.50,
  "created_at": 1704067500000
}
```

## Client Messages

Clients can send the following messages to the server:

### Subscribe
```json
{
  "type": "subscribe",
  "symbol": "NQ",
  "timeframes": ["1m", "5m", "15m"]
}
```

### Get History
```json
{
  "type": "get_history",
  "symbol": "NQ",
  "timeframe": "5m",
  "limit": 500
}
```

### Ping
```json
{
  "type": "ping",
  "timestamp": 1704067200000
}
```

## Architecture

```
Sierra Chart (ACSIL TCP) → Sierra TCP Server → BarBuilder → PhaseEngine
                                      ↓            ↓
                                  WebSocket Server
                                      ↓
                                Frontend Clients
```

1. **Sierra TCP Server**: Receives ticks from the ACSIL study over TCP
2. **BarBuilder**: Constructs multi-timeframe OHLCV bars from tick data
3. **PhaseEngine**: Processes closed bars to detect phase shifts and create nodes
4. **WebSocket Server**: Broadcasts all events to connected clients with subscription filtering

## Development

```bash
# Run in development mode with debug logging
cargo run -- --log-level debug

# Run tests
cargo test

# Format code
cargo fmt

# Check for issues
cargo clippy
```

## Dependencies

- `tokio` - Async runtime
- `tokio-tungstenite` - WebSocket server
- `serde` / `serde_json` - Serialization
- `phaseshifter-core` - Phase engine and node calculations
- `dashmap` - Concurrent hash map for client state
- `tracing` - Logging

## License

Same as parent project
