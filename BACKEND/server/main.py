#!/usr/bin/env python3
"""
PhaseShifter WebSocket Server (yfinance mode)

Streams real-time market data from yfinance and provides cluster/node
analysis to connected clients.

Run with:
    python main.py                    # Default settings
    python main.py --port 8001        # Custom port
"""

import asyncio
import logging
import signal
import sys
from typing import Optional

from pipeline_runner import PipelineRunner
from terminal import Terminal
from ws_server import WebSocketServer
from data_streamer import DataStreamer

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%H:%M:%S",
)

logger = logging.getLogger(__name__)


class PhaseShifterServer:
    def __init__(
        self,
        host: str = "localhost",
        port: int = 8001,
        poll_interval: int = 60,
        pipeline_interval: int = 300,
    ):
        self.host = host
        self.port = port
        self.poll_interval = poll_interval
        self.pipeline_interval = pipeline_interval

        self._running = False
        self._restart_requested = False

        self._init_components()

    def _init_components(self):
        """Initialize all server components."""
        self.server = WebSocketServer(host=self.host, port=self.port)
        self.pipeline = PipelineRunner(self.server, run_interval=self.pipeline_interval)
        self.streamer = DataStreamer(self.server, poll_interval=self.poll_interval)
        self.terminal = Terminal(self.server, self.streamer, self.pipeline, on_restart=self.request_restart)

        # Wire up WebSocket callbacks
        self.server.on_get_clusters = self._handle_get_clusters
        self.server.on_subscribe = self._handle_subscribe

    def _handle_subscribe(self, client_id: str, ticker: str):
        """Handle subscription - send initial candle data immediately."""
        asyncio.create_task(self._safe_subscribe(client_id, ticker))

    async def _safe_subscribe(self, client_id: str, ticker: str):
        """Safely handle subscription with error handling."""
        try:
            await self.streamer.send_initial_data(client_id, ticker)
        except Exception as e:
            logger.error(f"Error subscribing {client_id} to {ticker}: {e}")
            # Send error message to client
            await self.server.send_to_client(client_id, {
                "type": "error",
                "message": f"Failed to subscribe to {ticker}: {str(e)}",
                "ticker": ticker,
            })

    async def _handle_get_clusters(self, client_id: str, ticker: str) -> Optional[dict]:
        """Handle get_clusters request from a client."""
        try:
            print(f"    {client_id} requested clusters for {ticker}")
            data = await self.pipeline.get_cached_data(ticker)
            if data:
                cluster_count = len(data.get("clusters", []))
                node_count = len(data.get("nodes", []))
                print(
                    f"    → Sending {cluster_count} clusters, {node_count} nodes to {client_id}"
                )
            else:
                print(f"    → No cluster data available for {ticker}")
            return data
        except Exception as e:
            logger.error(f"Error getting clusters for {ticker}: {e}")
            return None

    async def start(self):
        """Start all components."""
        print("=" * 50)
        print("  PhaseShifter WebSocket Server")
        print("=" * 50)
        print(f"  Mode: yfinance")
        print(f"  Port: {self.port}")
        print(f"  Poll interval: {self.poll_interval}s")
        print()

        self._running = True
        await self.server.start()
        await self.streamer.start()
        await self.pipeline.start()

        print()

    async def stop(self):
        """Stop all components."""
        print("\nShutting down...")
        self._running = False
        self.terminal.stop()
        await self.streamer.stop()
        await self.pipeline.stop()
        await self.server.stop()

    def request_restart(self):
        """Request a hot restart of the server."""
        self._restart_requested = True
        self.terminal.stop()

    async def restart(self):
        """Perform a hot restart - stop and reinitialize components."""
        print("\n" + "=" * 50)
        print("  HOT RESTART")
        print("=" * 50)

        # Stop current components
        await self.streamer.stop()
        await self.pipeline.stop()
        await self.server.stop()

        # Small delay to let things settle
        await asyncio.sleep(0.5)

        # Reinitialize and start
        self._init_components()
        self._restart_requested = False

        await self.start()

    async def run(self):
        """Run the server with terminal REPL."""
        await self.start()

        # Set up signal handlers
        loop = asyncio.get_event_loop()

        def signal_handler():
            self.terminal.stop()

        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                loop.add_signal_handler(sig, signal_handler)
            except NotImplementedError:
                # Windows doesn't support add_signal_handler
                pass

        try:
            while True:
                await self.terminal.run()

                if self._restart_requested:
                    await self.restart()
                else:
                    break
        finally:
            if self._running:
                await self.stop()


def main():
    import argparse

    parser = argparse.ArgumentParser(description="PhaseShifter WebSocket Server")
    parser.add_argument(
        "--host",
        default="localhost",
        help="Host to bind to (default: localhost)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8001,
        help="Port to bind to (default: 8001)",
    )
    parser.add_argument(
        "--poll-interval",
        type=int,
        default=60,
        help="yfinance polling interval in seconds (default: 60)",
    )
    parser.add_argument(
        "--pipeline-interval",
        type=int,
        default=300,
        help="Pipeline run interval in seconds (default: 300)",
    )

    args = parser.parse_args()

    server = PhaseShifterServer(
        host=args.host,
        port=args.port,
        poll_interval=args.poll_interval,
        pipeline_interval=args.pipeline_interval,
    )

    try:
        asyncio.run(server.run())
    except KeyboardInterrupt:
        print("\nInterrupted")
        sys.exit(0)


if __name__ == "__main__":
    main()
