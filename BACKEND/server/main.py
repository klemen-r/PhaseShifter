#!/usr/bin/env python3
"""
PhaseShifter WebSocket Server

Streams yfinance data and cluster/node analysis to connected clients.
Run with: python main.py
"""

import asyncio
import signal
import sys
from typing import Optional

from ws_server import WebSocketServer
from data_streamer import DataStreamer
from pipeline_runner import PipelineRunner
from terminal import Terminal


class PhaseShifterServer:
    def __init__(
        self,
        host: str = "localhost",
        port: int = 8000,
        poll_interval: int = 60,
        pipeline_interval: int = 300,
    ):
        self.server = WebSocketServer(host=host, port=port)
        self.streamer = DataStreamer(self.server, poll_interval=poll_interval)
        self.pipeline = PipelineRunner(self.server, run_interval=pipeline_interval)
        self.terminal = Terminal(self.server, self.streamer, self.pipeline)

        # Wire up callbacks
        self.server.on_get_clusters = self._handle_get_clusters
        self.server.on_subscribe = self._handle_subscribe

    def _handle_subscribe(self, client_id: str, ticker: str):
        """Handle subscription - send initial candle data immediately."""
        asyncio.create_task(self.streamer.send_initial_data(client_id, ticker))

    async def _handle_get_clusters(self, client_id: str, ticker: str) -> Optional[dict]:
        """Handle get_clusters request from a client."""
        print(f"    {client_id} requested clusters for {ticker}")
        data = await self.pipeline.get_cached_data(ticker)
        if data:
            cluster_count = len(data.get("clusters", []))
            node_count = len(data.get("nodes", []))
            print(f"    → Sending {cluster_count} clusters, {node_count} nodes to {client_id}")
        else:
            print(f"    → No cluster data available for {ticker}")
        return data

    async def start(self):
        """Start all components."""
        print("=" * 50)
        print("  PhaseShifter WebSocket Server")
        print("=" * 50)
        print()

        await self.server.start()
        await self.streamer.start()
        await self.pipeline.start()

        print()

    async def stop(self):
        """Stop all components."""
        print("\nShutting down...")
        self.terminal.stop()
        await self.streamer.stop()
        await self.pipeline.stop()
        await self.server.stop()

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
            await self.terminal.run()
        finally:
            await self.stop()


def main():
    import argparse

    parser = argparse.ArgumentParser(
        description="PhaseShifter WebSocket Server"
    )
    parser.add_argument(
        "--host",
        default="localhost",
        help="Host to bind to (default: localhost)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8000,
        help="Port to bind to (default: 8000)",
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
