"""
Terminal REPL for server control.

Supports yfinance streaming mode with hot restart capability.
"""

import asyncio
from datetime import datetime
from typing import TYPE_CHECKING, Callable, Optional

if TYPE_CHECKING:
    from data_streamer import DataStreamer
    from pipeline_runner import PipelineRunner
    from ws_server import WebSocketServer


class Terminal:
    def __init__(
        self,
        server: "WebSocketServer",
        streamer: "DataStreamer",
        pipeline: "PipelineRunner",
        on_restart: Optional[Callable[[], None]] = None,
    ):
        self.server = server
        self.streamer = streamer
        self.pipeline = pipeline
        self._running = False
        self._on_restart = on_restart

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

        elif cmd == "restart":
            await self._restart()

        elif cmd == "clear":
            if not args:
                print("Usage: clear <ticker> or clear cache")
            else:
                await self._clear(args)

        elif cmd == "unsub":
            if not args:
                print("Usage: unsub <ticker> or unsub all")
            else:
                await self._unsub(args)

        elif cmd == "quit" or cmd == "exit":
            self._running = False
            print("Shutting down...")

        else:
            print(f"Unknown command: {cmd}. Type 'help' for available commands.")

    def _print_help(self):
        """Print help message."""
        print("""
Available commands:
  clients              List connected clients
  subscriptions, subs  Show ticker subscriptions
  broadcast <message>  Send message to all clients
  kick <client_id>     Disconnect a client
  cache                Show cached pipeline data
  run <ticker>         Manually trigger pipeline for ticker
  tickers              Show active tickers being streamed
  status               Server status and stats
  restart              Hot restart the server (keeps running)
  clear <ticker>       Clear cache for a ticker
  clear cache          Clear all cached data
  unsub <ticker>       Unsubscribe all clients from a ticker
  unsub all            Unsubscribe all clients from all tickers
  help                 Show this help message
  quit, exit           Shutdown the server
""")

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
        ticker = ticker.upper()
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
        print(f"  Mode: yfinance")
        active = streamer_status.get("active_tickers", [])
        print(f"  Active tickers: {len(active)}")
        if active:
            for ticker in active:
                print(f"    - {ticker}")
        print(f"  Poll interval: {streamer_status.get('poll_interval', 60)}s")

        print("\nPipeline Status:")
        cache = self.pipeline.get_cache_status()
        print(f"  Cached tickers: {len(cache)}")
        print(f"  Run interval: {self.pipeline.run_interval}s")
        print()

    def _show_active_tickers(self):
        """Show tickers being actively streamed."""
        tickers = self.streamer.get_status().get("active_tickers", [])

        if not tickers:
            print("No active tickers")
            return

        print(f"\nActive tickers ({len(tickers)}):")
        for ticker in sorted(tickers):
            subs = len(self.server.get_subscribers(ticker))
            print(f"  {ticker} ({subs} subscriber(s))")
        print()

    async def _restart(self):
        """Trigger a hot restart."""
        if self._on_restart:
            print("Initiating hot restart...")
            self._on_restart()
        else:
            print("Hot restart not available")

    async def _clear(self, args: str):
        """Clear cache for a ticker or all cache."""
        if args.lower() == "cache":
            self.pipeline.clear_cache()
            print("Cleared all cached data")
        else:
            ticker = args.upper()
            if self.pipeline.clear_cache_for_ticker(ticker):
                print(f"Cleared cache for {ticker}")
            else:
                print(f"No cache entry for {ticker}")

    async def _unsub(self, args: str):
        """Unsubscribe all clients from a ticker or all tickers."""
        if args.lower() == "all":
            results = await self.server.unsubscribe_all_tickers()
            if not results:
                print("No active subscriptions")
            else:
                total = sum(results.values())
                print(f"Unsubscribed {total} client(s) from {len(results)} ticker(s):")
                for ticker, count in sorted(results.items()):
                    print(f"  {ticker}: {count} client(s)")
                # Also clear cache
                self.pipeline.clear_cache()
                print("Also cleared all cached data")
        else:
            ticker = args.upper()
            count = await self.server.unsubscribe_all_from_ticker(ticker)
            if count > 0:
                print(f"Unsubscribed {count} client(s) from {ticker}")
                # Also clear cache for this ticker
                self.pipeline.clear_cache_for_ticker(ticker)
                print(f"Also cleared cache for {ticker}")
            else:
                print(f"No clients subscribed to {ticker}")

    def stop(self):
        """Stop the terminal."""
        self._running = False
