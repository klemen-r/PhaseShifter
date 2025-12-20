"""
Terminal REPL for server control.
"""

import asyncio
import sys
from datetime import datetime
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ws_server import WebSocketServer
    from data_streamer import DataStreamer
    from pipeline_runner import PipelineRunner


class Terminal:
    def __init__(
        self,
        server: "WebSocketServer",
        streamer: "DataStreamer",
        pipeline: "PipelineRunner",
    ):
        self.server = server
        self.streamer = streamer
        self.pipeline = pipeline
        self._running = False

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
            print(f"  {c['id']}")
            print(f"    Connected: {c['connected_at']}")
            print(f"    Subscriptions: {subs}")
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
        await self.server.broadcast({
            "type": "broadcast",
            "message": message,
            "timestamp": datetime.utcnow().isoformat(),
        })
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
        if status['uptime_seconds']:
            uptime = status['uptime_seconds']
            hours = int(uptime // 3600)
            mins = int((uptime % 3600) // 60)
            secs = int(uptime % 60)
            print(f"  Uptime: {hours}h {mins}m {secs}s")
        print(f"  Started: {status['started_at']}")

        print("\nStreamer Status:")
        print(f"  Active tickers: {len(streamer_status['active_tickers'])}")
        print(f"  Poll interval: {streamer_status['poll_interval']}s")

        print("\nPipeline Status:")
        cache = self.pipeline.get_cache_status()
        print(f"  Cached tickers: {len(cache)}")
        print(f"  Run interval: {self.pipeline.run_interval}s")
        print()

    def _show_active_tickers(self):
        """Show tickers being actively streamed."""
        tickers = self.streamer.get_status()["active_tickers"]
        if not tickers:
            print("No active tickers")
            return

        print(f"\nActive tickers ({len(tickers)}):")
        for ticker in sorted(tickers):
            subs = len(self.server.get_subscribers(ticker))
            print(f"  {ticker} ({subs} subscriber(s))")
        print()

    def stop(self):
        """Stop the terminal."""
        self._running = False
