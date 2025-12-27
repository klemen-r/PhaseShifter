"""
DTC-based data streaming - real-time market data from Sierra Chart.

Replaces yfinance polling with live tick data from Sierra Chart via DTC protocol.
Builds multi-timeframe bars and streams to connected WebSocket clients.
"""

import asyncio
import logging
import time
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING, Callable, Optional

from bar_builder import AsyncBarBuilder, Bar
from dtc_client import ConnectionState, DTCClient, Tick

if TYPE_CHECKING:
    from ws_server import WebSocketServer


logger = logging.getLogger(__name__)


# Symbol mapping: display ticker -> Sierra Chart symbol
# Supports both yfinance-style (NQ=F) and direct Sierra symbols (NQH26)
SYMBOL_MAP = {
    "NQ=F": "NQH26",
    "ES=F": "ESH26",
    # Direct Sierra symbols map to themselves
    "NQH26": "NQH26",
    "ESH26": "ESH26",
}

# Reverse mapping: Sierra symbol -> preferred display ticker
# For Sierra symbols, we keep them as-is (no conversion to yfinance style)
REVERSE_SYMBOL_MAP = {
    "NQH26": "NQH26",  # Keep Sierra format for display
    "ESH26": "ESH26",
}


def to_sierra_symbol(ticker: str) -> str:
    """
    Convert ticker to Sierra Chart symbol.

    Accepts:
    - yfinance style: "NQ=F" -> "NQH26"
    - Direct Sierra: "NQH26" -> "NQH26"
    """
    # Check explicit mapping first
    if ticker in SYMBOL_MAP:
        return SYMBOL_MAP[ticker]
    # If it looks like a Sierra futures symbol (letter+number pattern), use as-is
    if len(ticker) >= 4 and ticker[-1].isdigit():
        return ticker
    return ticker


def to_display_symbol(sierra_symbol: str) -> str:
    """
    Convert Sierra Chart symbol to display ticker.

    For now, keeps Sierra format (NQH26) for display consistency.
    """
    return REVERSE_SYMBOL_MAP.get(sierra_symbol, sierra_symbol)


@dataclass
class SymbolState:
    """Tracks state for a subscribed symbol."""

    ticker: str  # Display ticker (e.g., "NQ=F")
    sierra_symbol: str  # Sierra symbol (e.g., "NQH26")
    subscribed: bool = False
    last_price: float = 0.0
    last_tick_time: float = 0.0
    tick_count: int = 0


class DTCStreamer:
    """
    Real-time data streamer using Sierra Chart DTC protocol.

    Features:
    - Connects to Sierra Chart DTC server
    - Subscribes to market data for requested symbols
    - Builds multi-timeframe bars from ticks
    - Streams ticks and candles to WebSocket clients
    - Triggers pipeline on bar close events
    """

    def __init__(
        self,
        server: "WebSocketServer",
        host: str = "127.0.0.1",
        port: int = 11099,
        timeframes: list[str] = None,
        reconnect_delay: float = 5.0,
    ):
        self.server = server
        self.host = host
        self.port = port
        self.timeframes = timeframes or ["1m", "5m", "15m"]
        self.reconnect_delay = reconnect_delay

        # Components
        self.dtc_client = DTCClient(
            host=host,
            port=port,
            client_name="PhaseShifter",
            reconnect_delay=reconnect_delay,
        )
        self.bar_builder = AsyncBarBuilder(
            timeframes=self.timeframes,
            emit_updates=True,
            update_throttle_ms=100,
        )

        # State
        self._running = False
        self._symbols: dict[str, SymbolState] = {}  # ticker -> SymbolState
        self._connection_task: Optional[asyncio.Task] = None
        self._bar_processor_task: Optional[asyncio.Task] = None

        # Callbacks
        self.on_bar_closed: Optional[Callable[[Bar], asyncio.Future]] = None

        # Wire up DTC client callbacks
        self.dtc_client.on_tick = self._on_tick
        self.dtc_client.on_connection_state = self._on_connection_state
        self.dtc_client.on_error = self._on_error

    async def start(self):
        """Start the DTC streamer."""
        if self._running:
            return

        self._running = True
        logger.info(f"Starting DTC streamer (connecting to {self.host}:{self.port})")

        # Start connection in background
        self._connection_task = asyncio.create_task(self._connection_loop())

        # Start bar processor
        self._bar_processor_task = asyncio.create_task(self._bar_closed_processor())

        print(f"DTC streamer started (server: {self.host}:{self.port})")

    async def stop(self):
        """Stop the DTC streamer."""
        self._running = False

        if self._bar_processor_task:
            self._bar_processor_task.cancel()
            try:
                await self._bar_processor_task
            except asyncio.CancelledError:
                pass

        if self._connection_task:
            self._connection_task.cancel()
            try:
                await self._connection_task
            except asyncio.CancelledError:
                pass

        await self.dtc_client.disconnect()
        print("DTC streamer stopped")

    async def _connection_loop(self):
        """Maintain DTC connection."""
        while self._running:
            if not self.dtc_client.is_connected:
                success = await self.dtc_client.connect()
                if success:
                    # Resubscribe to all symbols
                    await self._resubscribe_all()
                else:
                    await asyncio.sleep(self.reconnect_delay)
            else:
                await asyncio.sleep(1.0)

    async def _resubscribe_all(self):
        """Resubscribe to all tracked symbols after reconnect."""
        for ticker, state in self._symbols.items():
            if not state.subscribed:
                await self.dtc_client.subscribe(state.sierra_symbol)
                logger.info(f"Resubscribed to {ticker} ({state.sierra_symbol})")

    async def _bar_closed_processor(self):
        """Process bar closed events from the bar builder."""
        while self._running:
            try:
                bar = await self.bar_builder.get_next_closed_bar(timeout=1.0)
                if bar:
                    await self._handle_bar_closed(bar)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error processing bar closed: {e}")

    async def _handle_bar_closed(self, bar: Bar):
        """Handle a completed bar - broadcast and trigger pipeline."""
        # Convert Sierra symbol back to display ticker
        ticker = to_display_symbol(bar.symbol)

        logger.info(
            f"Bar closed: {ticker} {bar.timeframe} "
            f"O={bar.open:.2f} H={bar.high:.2f} L={bar.low:.2f} C={bar.close:.2f} V={bar.volume:.0f}"
        )

        # Broadcast completed candle to subscribers
        await self.server.broadcast_to_ticker(
            ticker,
            {
                "type": "candle",
                "ticker": ticker,
                "data": bar.to_dict(),
            },
        )

        # Trigger external callback (for pipeline)
        if self.on_bar_closed:
            try:
                await self.on_bar_closed(bar)
            except Exception as e:
                logger.error(f"Error in bar_closed callback: {e}")

    def _on_tick(self, tick: Tick):
        """Handle incoming tick from DTC client."""
        # Map Sierra symbol to display ticker
        ticker = to_display_symbol(tick.symbol)

        # Update symbol state
        if ticker in self._symbols:
            state = self._symbols[ticker]
            state.last_price = tick.price
            state.last_tick_time = tick.timestamp
            state.tick_count += 1

        # Feed to bar builder
        self.bar_builder.on_tick(tick)

        # Broadcast tick to subscribers
        asyncio.create_task(self._broadcast_tick(ticker, tick))

    async def _broadcast_tick(self, ticker: str, tick: Tick):
        """Broadcast tick update to WebSocket clients."""
        await self.server.broadcast_to_ticker(
            ticker,
            {
                "type": "tick",
                "ticker": ticker,
                "data": {
                    "price": tick.price,
                    "volume": tick.volume,
                    "timestamp": int(tick.timestamp * 1000),
                    "bid": tick.bid_price,
                    "ask": tick.ask_price,
                },
            },
        )

    def _on_connection_state(self, state: ConnectionState):
        """Handle DTC connection state changes."""
        logger.info(f"DTC connection state: {state.name}")

        # Update subscription states on disconnect
        if state == ConnectionState.DISCONNECTED:
            for symbol_state in self._symbols.values():
                symbol_state.subscribed = False

    def _on_error(self, message: str):
        """Handle DTC client errors."""
        logger.error(f"DTC error: {message}")

    async def subscribe(self, ticker: str) -> bool:
        """
        Subscribe to market data for a ticker.

        Args:
            ticker: Ticker symbol (e.g., "NQ=F" or "NQH26")

        Returns:
            True if subscription was initiated
        """
        sierra_symbol = to_sierra_symbol(ticker)

        if ticker not in self._symbols:
            self._symbols[ticker] = SymbolState(
                ticker=ticker,
                sierra_symbol=sierra_symbol,
            )

        if self.dtc_client.is_connected:
            success = await self.dtc_client.subscribe(sierra_symbol)
            if success:
                self._symbols[ticker].subscribed = True
                logger.info(f"Subscribed to {ticker} ({sierra_symbol})")
            return success
        else:
            logger.info(f"Queued subscription for {ticker} (DTC not connected)")
            return True

    async def unsubscribe(self, ticker: str) -> bool:
        """Unsubscribe from market data for a ticker."""
        if ticker not in self._symbols:
            return True

        state = self._symbols[ticker]

        if self.dtc_client.is_connected:
            await self.dtc_client.unsubscribe(state.sierra_symbol)

        del self._symbols[ticker]
        self.bar_builder.clear_symbol(state.sierra_symbol)
        logger.info(f"Unsubscribed from {ticker}")
        return True

    def get_candles(self, ticker: str, timeframe: str = "1m") -> list[dict]:
        """Get all candles for a ticker/timeframe."""
        sierra_symbol = to_sierra_symbol(ticker)
        return self.bar_builder.get_candles_dict(sierra_symbol, timeframe)

    def get_current_bar(self, ticker: str, timeframe: str = "1m") -> Optional[dict]:
        """Get the current forming bar."""
        sierra_symbol = to_sierra_symbol(ticker)
        bar = self.bar_builder.get_current_bar(sierra_symbol, timeframe)
        return bar.to_dict() if bar else None

    async def send_initial_data(self, client_id: str, ticker: str):
        """Send historical candles to a newly subscribed client."""
        sierra_symbol = to_sierra_symbol(ticker)

        # Get 1m bars for chart display
        candles = self.bar_builder.get_candles_dict(sierra_symbol, "1m")

        if candles:
            print(f"    → Sending {len(candles)} candles to {client_id}")
            for candle in candles:
                await self.server.send_to_client(
                    client_id,
                    {
                        "type": "candle",
                        "ticker": ticker,
                        "data": candle,
                    },
                )
        else:
            print(f"    → No candle data yet for {ticker} (waiting for ticks)")

            # Send current price if available
            if ticker in self._symbols:
                state = self._symbols[ticker]
                if state.last_price > 0:
                    await self.server.send_to_client(
                        client_id,
                        {
                            "type": "tick",
                            "ticker": ticker,
                            "data": {
                                "price": state.last_price,
                                "volume": 0,
                                "timestamp": int(time.time() * 1000),
                            },
                        },
                    )

    def get_status(self) -> dict:
        """Get streamer status."""
        return {
            "running": self._running,
            "dtc_connected": self.dtc_client.is_connected,
            "dtc_state": self.dtc_client.state.name,
            "host": self.host,
            "port": self.port,
            "symbols": {
                ticker: {
                    "sierra_symbol": state.sierra_symbol,
                    "subscribed": state.subscribed,
                    "last_price": state.last_price,
                    "tick_count": state.tick_count,
                }
                for ticker, state in self._symbols.items()
            },
            "bar_builder": self.bar_builder.get_stats(),
        }

    def get_active_tickers(self) -> list[str]:
        """Get list of active ticker subscriptions."""
        return list(self._symbols.keys())


class HybridStreamer:
    """
    Hybrid streamer that can use either DTC or yfinance.

    Falls back to yfinance if DTC connection fails or for unsupported symbols.
    """

    def __init__(
        self,
        server: "WebSocketServer",
        dtc_host: str = "127.0.0.1",
        dtc_port: int = 11099,
        use_dtc: bool = True,
        yfinance_poll_interval: int = 60,
    ):
        self.server = server
        self.use_dtc = use_dtc

        # Create both streamers
        self.dtc_streamer = (
            DTCStreamer(
                server=server,
                host=dtc_host,
                port=dtc_port,
            )
            if use_dtc
            else None
        )

        # Import yfinance streamer lazily
        self._yfinance_streamer = None

        # Track which symbols use which source
        self._dtc_symbols: set[str] = set()
        self._yfinance_symbols: set[str] = set()

    @property
    def _yfinance(self):
        """Lazy load yfinance streamer."""
        if self._yfinance_streamer is None:
            from data_streamer import DataStreamer

            self._yfinance_streamer = DataStreamer(
                self.server,
                poll_interval=60,
            )
        return self._yfinance_streamer

    async def start(self):
        """Start the hybrid streamer."""
        if self.use_dtc and self.dtc_streamer:
            await self.dtc_streamer.start()
        # yfinance streamer started on demand

    async def stop(self):
        """Stop all streamers."""
        if self.dtc_streamer:
            await self.dtc_streamer.stop()
        if self._yfinance_streamer:
            await self._yfinance_streamer.stop()

    async def subscribe(self, ticker: str) -> bool:
        """Subscribe to a ticker, choosing appropriate source."""
        # Check if this is a Sierra Chart compatible symbol
        is_dtc_symbol = (
            ticker in SYMBOL_MAP or ticker.startswith("NQ") or ticker.startswith("ES")
        )

        if self.use_dtc and self.dtc_streamer and is_dtc_symbol:
            success = await self.dtc_streamer.subscribe(ticker)
            if success:
                self._dtc_symbols.add(ticker)
                return True

        # Fall back to yfinance
        self._yfinance_symbols.add(ticker)
        if not self._yfinance_streamer:
            await self._yfinance.start()
        return True

    async def unsubscribe(self, ticker: str):
        """Unsubscribe from a ticker."""
        if ticker in self._dtc_symbols and self.dtc_streamer:
            await self.dtc_streamer.unsubscribe(ticker)
            self._dtc_symbols.discard(ticker)
        if ticker in self._yfinance_symbols:
            self._yfinance_symbols.discard(ticker)

    async def send_initial_data(self, client_id: str, ticker: str):
        """Send initial data to a client."""
        if ticker in self._dtc_symbols and self.dtc_streamer:
            await self.dtc_streamer.send_initial_data(client_id, ticker)
        elif self._yfinance_streamer:
            await self._yfinance_streamer.send_initial_data(client_id, ticker)

    def get_status(self) -> dict:
        """Get combined status."""
        status = {
            "mode": "dtc" if self.use_dtc else "yfinance",
            "dtc_symbols": list(self._dtc_symbols),
            "yfinance_symbols": list(self._yfinance_symbols),
        }
        if self.dtc_streamer:
            status["dtc"] = self.dtc_streamer.get_status()
        if self._yfinance_streamer:
            status["yfinance"] = self._yfinance_streamer.get_status()
        return status
