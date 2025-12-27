"""
Terminal REPL for server control.

Supports both DTC and yfinance streaming modes.
"""

import asyncio
from datetime import datetime
from typing import TYPE_CHECKING, Any, Union

if TYPE_CHECKING:
    from data_streamer import DataStreamer
    from dtc_streamer import DTCStreamer
    from pipeline_runner import PipelineRunner
    from ws_server import WebSocketServer


class Terminal:
    def __init__(
        self,
        server: "WebSocketServer",
        streamer: Union["DataStreamer", "DTCStreamer"],
        pipeline: "PipelineRunner",
    ):
        self.server = server
        self.streamer = streamer
        self.pipeline = pipeline
        self._running = False

        # Detect streamer type
        self._is_dtc = hasattr(streamer, "dtc_client")

    async def run(self):
        """Run the terminal REPL."""
        self._running = True
        print("\nType 'help' for available commands\n")

        while self._running:
            try:
                # Read input asynchronously
                line = await asyncio.get_event_loop().run_in_executor(
                    None, lambda: input("> ")
                )
                line = line.strip()
                if not line:
                    continue

                await self._execute(line)

            except EOFError:
                break
            except KeyboardInterrupt:
                print("\nUse 'quit' to exit")

    async def _execute(self, line: str):
        """Execute a command."""
        parts = line.split(maxsplit=1)
        cmd = parts[0].lower()
        args = parts[1] if len(parts) > 1 else ""

        if cmd == "help":
            self._print_help()

        elif cmd == "clients":
            self._show_clients()

        elif cmd == "subscriptions" or cmd == "subs":
            self._show_subscriptions()

        elif cmd == "broadcast":
            if not args:
                print("Usage: broadcast <message>")
            else:
                await self._broadcast(args)

        elif cmd == "kick":
            if not args:
                print("Usage: kick <client_id>")
            else:
                await self._kick(args)

        elif cmd == "cache":
            self._show_cache()

        elif cmd == "run":
            if not args:
                print("Usage: run <ticker>")
            else:
                await self._run_pipeline(args)

        elif cmd == "status":
            self._show_status()

        elif cmd == "tickers":
            self._show_active_tickers()

        elif cmd == "dtc":
            self._show_dtc_status()

        elif cmd == "bars":
            if not args:
                print("Usage: bars <ticker> [timeframe]")
            else:
                self._show_bars(args)

        elif cmd == "sub":
            if not args:
                print("Usage: sub <ticker>")
            else:
                await self._subscribe_ticker(args)

        elif cmd == "unsub":
            if not args:
                print("Usage: unsub <ticker>")
            else:
                await self._unsubscribe_ticker(args)

        elif cmd == "quit" or cmd == "exit":
            self._running = False
            print("Shutting down...")

        else:
            print(f"Unknown command: {cmd}. Type 'help' for available commands.")

    def _print_help(self):
        """Print help message."""
        base_help = """
Available commands:
  clients              List connected clients
  subscriptions, subs  Show ticker subscriptions
  broadcast <message>  Send message to all clients
  kick <client_id>     Disconnect a client
  cache                Show cached pipeline data
  run <ticker>         Manually trigger pipeline for ticker
  tickers              Show active tickers being streamed
  status               Server status and stats
  help                 Show this help message
  quit, exit           Shutdown the server
"""
        dtc_help = """
DTC Commands (Sierra Chart mode):
  dtc                  Show DTC connection status
  bars <ticker> [tf]   Show bar data (tf: 1m, 5m, 15m)
  sub <ticker>         Subscribe to ticker (e.g., NQH26)
  unsub <ticker>       Unsubscribe from ticker
"""
        print(base_help)
        if self._is_dtc:
            print(dtc_help)

    def _show_clients(self):
        """Show connected clients."""
        clients = self.server.list_clients()
        if not clients:
            print("No clients connected")
            return

        print(f"\nConnected clients ({len(clients)}):")
        print("-" * 60)
        for c in clients:
            subs = ", ".join(c["subscriptions"]) if c["subscriptions"] else "(none)"
            auto = (
                ", ".join(c.get("auto_clusters", [])) if c.get("auto_clusters") else ""
            )
            print(f"  {c['id']}")
            print(f"    Connected: {c['connected_at']}")
            print(f"    Subscriptions: {subs}")
            if auto:
                print(f"    Auto-clusters: {auto}")
        print()

    def _show_subscriptions(self):
        """Show subscriptions by ticker."""
        subs = self.server.get_subscriptions_summary()
        if not subs:
            print("No active subscriptions")
            return

        print("\nSubscriptions by ticker:")
        print("-" * 40)
        for ticker, client_ids in sorted(subs.items()):
            print(f"  {ticker}: {len(client_ids)} client(s)")
            for cid in client_ids:
                print(f"    - {cid}")
        print()

    async def _broadcast(self, message: str):
        """Broadcast a message to all clients."""
        await self.server.broadcast(
            {
                "type": "broadcast",
                "message": message,
                "timestamp": datetime.utcnow().isoformat(),
            }
        )
        print(f"Broadcasted to {len(self.server.clients)} client(s)")

    async def _kick(self, client_id: str):
        """Kick a client."""
        if client_id not in self.server.clients:
            print(f"Client not found: {client_id}")
            return
        await self.server.kick_client(client_id)
        print(f"Kicked client: {client_id}")

    def _show_cache(self):
        """Show cached pipeline data."""
        cache = self.pipeline.get_cache_status()
        if not cache:
            print("Cache is empty")
            return

        print("\nCached pipeline data:")
        print("-" * 50)
        for ticker, info in sorted(cache.items()):
            print(f"  {ticker}:")
            print(f"    Generated: {info['generated_at']}")
            print(f"    Clusters: {info['cluster_count']}")
            print(f"    Nodes: {info['node_count']}")
            age_sec = info.get("age_seconds", 0)
            print(f"    Age: {age_sec:.0f}s")
        print()

    async def _run_pipeline(self, ticker: str):
        """Manually trigger pipeline for a ticker."""
        print(f"Running pipeline for {ticker}...")
        try:
            result = await self.pipeline.run_for_ticker(ticker)
            if result:
                print(f"Pipeline completed for {ticker}")
                print(f"  Clusters: {len(result.get('clusters', []))}")
                print(f"  Nodes: {len(result.get('nodes', []))}")
            else:
                print(f"Pipeline returned no data for {ticker}")
        except Exception as e:
            print(f"Pipeline failed: {e}")

    def _show_status(self):
        """Show server status."""
        status = self.server.get_status()
        streamer_status = self.streamer.get_status()

        print("\nServer Status:")
        print("-" * 40)
        print(f"  Address: ws://{status['host']}:{status['port']}")
        print(f"  Clients: {status['clients']}")
        if status["uptime_seconds"]:
            uptime = status["uptime_seconds"]
            hours = int(uptime // 3600)
            mins = int((uptime % 3600) // 60)
            secs = int(uptime % 60)
            print(f"  Uptime: {hours}h {mins}m {secs}s")
        print(f"  Started: {status['started_at']}")

        print("\nStreamer Status:")
        if self._is_dtc:
            print(f"  Mode: DTC (Sierra Chart)")
            print(f"  DTC Connected: {streamer_status.get('dtc_connected', False)}")
            print(f"  DTC State: {streamer_status.get('dtc_state', 'unknown')}")
            symbols = streamer_status.get("symbols", {})
            print(f"  Subscribed symbols: {len(symbols)}")
            for sym, info in symbols.items():
                subscribed = "✓" if info.get("subscribed") else "○"
                price = info.get("last_price", 0)
                ticks = info.get("tick_count", 0)
                print(f"    {subscribed} {sym}: ${price:.2f} ({ticks} ticks)")
        else:
            print(f"  Mode: yfinance")
            active = streamer_status.get("active_tickers", [])
            print(f"  Active tickers: {len(active)}")
            print(f"  Poll interval: {streamer_status.get('poll_interval', 60)}s")

        print("\nPipeline Status:")
        cache = self.pipeline.get_cache_status()
        print(f"  Cached tickers: {len(cache)}")
        print(f"  Run interval: {self.pipeline.run_interval}s")
        print()

    def _show_active_tickers(self):
        """Show tickers being actively streamed."""
        if self._is_dtc:
            tickers = self.streamer.get_active_tickers()
        else:
            tickers = self.streamer.get_status().get("active_tickers", [])

        if not tickers:
            print("No active tickers")
            return

        print(f"\nActive tickers ({len(tickers)}):")
        for ticker in sorted(tickers):
            subs = len(self.server.get_subscribers(ticker))
            print(f"  {ticker} ({subs} subscriber(s))")
        print()

    def _show_dtc_status(self):
        """Show detailed DTC connection status."""
        if not self._is_dtc:
            print("Not running in DTC mode")
            return

        status = self.streamer.get_status()
        print("\nDTC Connection Status:")
        print("-" * 50)
        print(f"  Host: {status.get('host')}:{status.get('port')}")
        print(f"  Connected: {status.get('dtc_connected', False)}")
        print(f"  State: {status.get('dtc_state', 'unknown')}")

        # Show symbols
        symbols = status.get("symbols", {})
        if symbols:
            print(f"\nSubscribed Symbols ({len(symbols)}):")
            for sym, info in symbols.items():
                sierra = info.get("sierra_symbol", sym)
                subscribed = "✓" if info.get("subscribed") else "○"
                price = info.get("last_price", 0)
                ticks = info.get("tick_count", 0)
                print(f"  {subscribed} {sym} → {sierra}")
                print(f"      Last: ${price:.2f}, Ticks: {ticks}")

        # Show bar builder stats
        bar_stats = status.get("bar_builder", {})
        if bar_stats:
            print(f"\nBar Builder:")
            print(f"  Total ticks: {bar_stats.get('tick_count', 0)}")
            print(f"  Bars closed: {bar_stats.get('bars_closed', 0)}")
            timeframes = bar_stats.get("timeframes", [])
            print(f"  Timeframes: {', '.join(timeframes)}")

            history = bar_stats.get("history_sizes", {})
            for sym, tfs in history.items():
                print(f"  {sym}:")
                for tf, count in tfs.items():
                    print(f"    {tf}: {count} bars")
        print()

    def _show_bars(self, args: str):
        """Show bar data for a ticker."""
        if not self._is_dtc:
            print("Bar data only available in DTC mode")
            return

        parts = args.split()
        ticker = parts[0]
        timeframe = parts[1] if len(parts) > 1 else "1m"

        candles = self.streamer.get_candles(ticker, timeframe)
        if not candles:
            print(f"No bar data for {ticker} ({timeframe})")
            return

        print(f"\n{ticker} {timeframe} bars ({len(candles)} total, last 10):")
        print("-" * 70)
        print(f"  {'Time':<20} {'Open':>10} {'High':>10} {'Low':>10} {'Close':>10}")
        print("-" * 70)

        for candle in candles[-10:]:
            ts = candle.get("time", 0) / 1000
            time_str = datetime.fromtimestamp(ts).strftime("%Y-%m-%d %H:%M")
            print(
                f"  {time_str:<20} "
                f"{candle.get('open', 0):>10.2f} "
                f"{candle.get('high', 0):>10.2f} "
                f"{candle.get('low', 0):>10.2f} "
                f"{candle.get('close', 0):>10.2f}"
            )
        print()

    async def _subscribe_ticker(self, ticker: str):
        """Subscribe to a ticker (DTC mode)."""
        if not self._is_dtc:
            print("Direct subscription only available in DTC mode")
            return

        ticker = ticker.upper()
        print(f"Subscribing to {ticker}...")
        success = await self.streamer.subscribe(ticker)
        if success:
            print(f"Subscribed to {ticker}")
        else:
            print(f"Failed to subscribe to {ticker}")

    async def _unsubscribe_ticker(self, ticker: str):
        """Unsubscribe from a ticker (DTC mode)."""
        if not self._is_dtc:
            print("Direct unsubscription only available in DTC mode")
            return

        ticker = ticker.upper()
        await self.streamer.unsubscribe(ticker)
        print(f"Unsubscribed from {ticker}")

    def stop(self):
        """Stop the terminal."""
        self._running = False
