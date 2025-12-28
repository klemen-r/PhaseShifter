User approved Claude's plan
  ⎿  Plan saved to: ~/.claude/plans/memoized-dazzling-dragonfly.md · /plan to edit
     Python WebSocket Server Plan

     Requirements

     - WebSocket server handling multiple simultaneous connections
     - Stream yfinance 1-minute candle data based on client-requested tickers
     - Run Rust pipeline every 5 minutes, cache cluster/node results
     - Terminal REPL for control (list clients, broadcast, view subscriptions, manual triggers)
     - JSON message protocol
     - Port 8000

     Architecture

     File Structure

     BACKEND/server/
     ├── __init__.py
     ├── main.py              # Entry point - starts server + terminal REPL
     ├── ws_server.py         # WebSocket server, client management
     ├── data_streamer.py     # yfinance 1m data streaming
     ├── pipeline_runner.py   # Runs Rust core every 5 min, caches results
     ├── terminal.py          # Interactive terminal commands
     └── requirements.txt     # websockets, yfinance, pandas

     Components

     1. main.py - Entry Point

     # Starts three concurrent tasks:
     # 1. WebSocket server (port 8000)
     # 2. Terminal REPL (stdin)
     # 3. Pipeline scheduler (every 5 min)

     2. ws_server.py - WebSocket Server

     - Manages connected clients with unique IDs
     - Routes incoming messages to handlers
     - Tracks subscriptions per client (which tickers they want)
     - Broadcasts to specific ticker subscribers

     Client state:
     {
       "id": "client_abc123",
       "websocket": <WebSocket>,
       "subscriptions": {"NQ=F", "BTC-USD"},  # tickers
       "connected_at": timestamp
     }

     3. data_streamer.py - yfinance Streaming

     - Fetches 1m candles for subscribed tickers
     - Polls yfinance every ~60 seconds for new data
     - Pushes new candles to subscribed clients
     - Handles multiple tickers efficiently (batch requests)

     4. pipeline_runner.py - Cluster/Node Cache

     - Runs the Rust pipeline every 5 minutes
     - Caches results per ticker: {ticker: {clusters, nodes, timestamp}}
     - On client request, returns cached data immediately
     - Uses existing run_phase_pipeline.py logic or calls Rust directly

     5. terminal.py - REPL Commands

     > clients              # List connected clients
     > subscriptions        # Show who's watching which ticker
     > broadcast <message>  # Send to all clients
     > kick <client_id>     # Disconnect a client
     > cache                # Show cached pipeline data
     > run <ticker>         # Manually trigger pipeline for ticker
     > status               # Server stats (uptime, connections, etc.)
     > help                 # Show commands

     Message Protocol

     Client → Server

     // Subscribe to ticker data stream
     {"type": "subscribe", "ticker": "NQ=F"}

     // Unsubscribe
     {"type": "unsubscribe", "ticker": "NQ=F"}

     // Request cached clusters/nodes
     {"type": "get_clusters", "ticker": "NQ=F"}

     // Ping
     {"type": "ping"}

     Server → Client

     // New candle data
     {"type": "candle", "ticker": "NQ=F", "data": {
       "time": 1702500000000,
       "open": 21500.5,
       "high": 21510.0,
       "low": 21495.0,
       "close": 21505.25,
       "volume": 1234
     }}

     // Cluster/node data (on request or after pipeline run)
     {"type": "clusters", "ticker": "NQ=F", "data": {
       "anchor": 21500.0,
       "clusters": [...],
       "nodes": [...],
       "generated_at": "2024-12-14T10:30:00Z"
     }}

     // Subscription confirmed
     {"type": "subscribed", "ticker": "NQ=F"}

     // Error
     {"type": "error", "message": "..."}

     // Pong
     {"type": "pong"}

     Data Flow

     ┌─────────────────────────────────────────────────────────┐
     │                    Python WS Server                      │
     │                                                          │
     │  ┌──────────────┐    ┌───────────────┐                 │
     │  │ Terminal     │    │ Pipeline      │ ← runs every 5m │
     │  │ REPL         │    │ Runner        │                 │
     │  └──────────────┘    └───────┬───────┘                 │
     │         │                    │                          │
     │         ▼                    ▼                          │
     │  ┌──────────────────────────────────────┐              │
     │  │           WS Server Core             │              │
     │  │  - Client management                 │              │
     │  │  - Message routing                   │              │
     │  │  - Subscription tracking             │              │
     │  └──────────────────┬───────────────────┘              │
     │                     │                                   │
     │         ┌───────────┴───────────┐                      │
     │         ▼                       ▼                      │
     │  ┌──────────────┐       ┌──────────────┐              │
     │  │ Data         │       │ Cluster      │              │
     │  │ Streamer     │       │ Cache        │              │
     │  │ (yfinance)   │       │ (per ticker) │              │
     │  └──────────────┘       └──────────────┘              │
     │                                                         │
     └─────────────────────────────────────────────────────────┘
              │                       │
              ▼                       ▼
        [yfinance API]          [Rust Core]

     Implementation Order

     1. ws_server.py - Basic WebSocket server with client tracking
     2. terminal.py - REPL commands (clients, broadcast, help)
     3. main.py - Wire up server + terminal with asyncio
     4. data_streamer.py - yfinance polling and streaming
     5. pipeline_runner.py - Integrate with existing pipeline, add caching

     Dependencies (requirements.txt)

     websockets>=12.0
     yfinance>=0.2.0
     pandas>=2.0.0

     Usage

     cd BACKEND/server
     pip install -r requirements.txt
     python main.py

     # Terminal shows:
     # PhaseShifter WS Server running on ws://localhost:8000
     # Type 'help' for commands
     # > _
     last thing made
     ⏺ Write(~/Documents/PhaseShifterSt/PhaseShifter/BACKEND/server/terminal.py)
       ⎿  Wrote 238 lines to ../../BACKEND/server/terminal.py
          """
          Terminal REPL for server control.
          """
          import asyncio
          import sys
          from datetime import datetime
          from typing import TYPE_CHECKING
          if TYPE_CHECKING:
          … +228 lines (ctrl+o to expand)
       ⎿  Limit reached · resets 10pm (Europe/Ljubljana) · /upgrade to Max or turn on /extra-usage
          Opening your options…
