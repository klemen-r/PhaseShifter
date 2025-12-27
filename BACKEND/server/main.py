#!/usr/bin/env python3
"""
PhaseShifter WebSocket Server

Streams real-time market data from Sierra Chart (DTC) or yfinance,
and provides cluster/node analysis to connected clients.

Run with:
    python main.py                    # DTC mode (Sierra Chart)
    python main.py --mode yfinance    # yfinance mode (fallback)
"""

import asyncio
import logging
import signal
import sys
from typing import Optional

from pipeline_runner import PipelineRunner
from terminal import Terminal
from ws_server import WebSocketServer

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%H:%M:%S",
)


class PhaseShifterServer:
    def __init__(
        self,
        host: str = "localhost",
        port: int = 8000,
        mode: str = "dtc",  # "dtc" or "yfinance"
        dtc_host: str = "127.0.0.1",
        dtc_port: int = 11099,
        poll_interval: int = 60,
        pipeline_interval: int = 300,
    ):
        self.mode = mode
        self.server = WebSocketServer(host=host, port=port)
        self.pipeline = PipelineRunner(self.server, run_interval=pipeline_interval)

        # Initialize streamer based on mode
        if mode == "dtc":
            from dtc_streamer import DTCStreamer

            self.streamer = DTCStreamer(
                self.server,
                host=dtc_host,
                port=dtc_port,
            )
            # Wire up bar closed callback for pipeline triggers
            self.streamer.on_bar_closed = self._handle_bar_closed
        else:
            from data_streamer import DataStreamer

            self.streamer = DataStreamer(self.server, poll_interval=poll_interval)

        self.terminal = Terminal(self.server, self.streamer, self.pipeline)

        # Wire up WebSocket callbacks
        self.server.on_get_clusters = self._handle_get_clusters
        self.server.on_subscribe = self._handle_subscribe

    def _handle_subscribe(self, client_id: str, ticker: str):
        """Handle subscription - send initial candle data immediately."""
        if self.mode == "dtc":
            asyncio.create_task(self._subscribe_dtc(client_id, ticker))
        else:
            asyncio.create_task(self.streamer.send_initial_data(client_id, ticker))

    async def _subscribe_dtc(self, client_id: str, ticker: str):
        """Handle DTC subscription."""
        await self.streamer.subscribe(ticker)
        await self.streamer.send_initial_data(client_id, ticker)

    async def _handle_bar_closed(self, bar):
        """Handle bar closed event - trigger pipeline for relevant timeframes."""
        from dtc_streamer import to_display_symbol

        ticker = to_display_symbol(bar.symbol)
        timeframe = bar.timeframe

        # Map timeframes to pipeline configurations
        # Only trigger pipeline for timeframes we care about
        trigger_timeframes = {"1m", "5m", "15m"}

        if timeframe in trigger_timeframes:
            # Check if any client has auto-clusters enabled for this ticker
            auto_subscribers = self.server.get_auto_cluster_subscribers(ticker)
            if auto_subscribers:
                print(f"[Pipeline] Bar closed trigger: {ticker} {timeframe}")
                await self.pipeline.run_for_ticker(ticker)

    async def _handle_get_clusters(self, client_id: str, ticker: str) -> Optional[dict]:
        """Handle get_clusters request from a client."""
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

    async def start(self):
        """Start all components."""
        print("=" * 50)
        print("  PhaseShifter WebSocket Server")
        print("=" * 50)
        print(f"  Mode: {self.mode.upper()}")
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

    parser = argparse.ArgumentParser(description="PhaseShifter WebSocket Server")
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
        "--mode",
        choices=["dtc", "yfinance"],
        default="dtc",
        help="Data source mode: dtc (Sierra Chart) or yfinance (default: dtc)",
    )
    parser.add_argument(
        "--dtc-host",
        default="127.0.0.1",
        help="DTC server host (default: 127.0.0.1)",
    )
    parser.add_argument(
        "--dtc-port",
        type=int,
        default=11099,
        help="DTC server port (default: 11099)",
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
        mode=args.mode,
        dtc_host=args.dtc_host,
        dtc_port=args.dtc_port,
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
