"""
Multi-timeframe bar builder.

Constructs OHLCV bars from tick data for multiple timeframes simultaneously.
Emits events when bars close, enabling timeframe-specific pipeline triggers.
"""

import asyncio
import logging
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Callable, Optional

from dtc_client import Tick

logger = logging.getLogger(__name__)


@dataclass
class Bar:
    """OHLCV bar."""

    symbol: str
    timeframe: str
    timestamp_ms: int  # Bar open time in milliseconds
    open: float
    high: float
    low: float
    close: float
    volume: float
    tick_count: int = 0
    is_closed: bool = False

    def to_dict(self) -> dict:
        return {
            "time": self.timestamp_ms,
            "open": self.open,
            "high": self.high,
            "low": self.low,
            "close": self.close,
            "volume": int(self.volume),
        }


@dataclass
class BarState:
    """Mutable state for a bar being built."""

    symbol: str
    timeframe: str
    bar_open_time_ms: int
    open: float
    high: float
    low: float
    close: float
    volume: float = 0.0
    tick_count: int = 0

    def update(self, price: float, volume: float):
        """Update bar with new tick."""
        if self.tick_count == 0:
            self.open = price
            self.high = price
            self.low = price
        else:
            self.high = max(self.high, price)
            self.low = min(self.low, price)
        self.close = price
        self.volume += volume
        self.tick_count += 1

    def to_bar(self, is_closed: bool = False) -> Bar:
        """Convert to immutable Bar."""
        return Bar(
            symbol=self.symbol,
            timeframe=self.timeframe,
            timestamp_ms=self.bar_open_time_ms,
            open=self.open,
            high=self.high,
            low=self.low,
            close=self.close,
            volume=self.volume,
            tick_count=self.tick_count,
            is_closed=is_closed,
        )


class Timeframe(Enum):
    """Supported timeframes."""

    M1 = "1m"
    M5 = "5m"
    M15 = "15m"
    M30 = "30m"
    H1 = "1h"
    H4 = "4h"
    D1 = "1d"

    @property
    def seconds(self) -> int:
        """Duration in seconds."""
        mapping = {
            "1m": 60,
            "5m": 300,
            "15m": 900,
            "30m": 1800,
            "1h": 3600,
            "4h": 14400,
            "1d": 86400,
        }
        return mapping[self.value]

    @property
    def milliseconds(self) -> int:
        """Duration in milliseconds."""
        return self.seconds * 1000


# Callback types
BarClosedCallback = Callable[[Bar], None]
BarUpdateCallback = Callable[[Bar], None]


def get_bar_open_time_ms(timestamp_ms: int, timeframe: Timeframe) -> int:
    """
    Calculate the bar open time for a given timestamp and timeframe.

    Aligns to clean boundaries (e.g., 1m bars start at :00, 5m at :00/:05/:10, etc.)
    """
    tf_ms = timeframe.milliseconds
    return (timestamp_ms // tf_ms) * tf_ms


class BarBuilder:
    """
    Builds OHLCV bars from tick data for multiple symbols and timeframes.

    Usage:
        builder = BarBuilder(["1m", "5m", "15m"])
        builder.on_bar_closed = my_bar_handler
        builder.on_bar_update = my_update_handler

        # Feed ticks
        builder.on_tick(tick)
    """

    def __init__(
        self,
        timeframes: list[str] = None,
        emit_updates: bool = True,
        update_throttle_ms: int = 100,
    ):
        """
        Initialize bar builder.

        Args:
            timeframes: List of timeframe strings (e.g., ["1m", "5m", "15m"])
            emit_updates: Whether to emit bar updates (live candle updates)
            update_throttle_ms: Minimum ms between update emissions per bar
        """
        if timeframes is None:
            timeframes = ["1m", "5m", "15m"]

        self.timeframes = [Timeframe(tf) for tf in timeframes]
        self.emit_updates = emit_updates
        self.update_throttle_ms = update_throttle_ms

        # State: symbol -> timeframe -> BarState
        self._bars: dict[str, dict[Timeframe, BarState]] = {}

        # History: symbol -> timeframe -> list of closed bars
        self._history: dict[str, dict[Timeframe, list[Bar]]] = {}
        self._max_history = 500  # Max bars to keep per symbol/timeframe

        # Throttling: symbol -> timeframe -> last update emit time
        self._last_update_time: dict[str, dict[Timeframe, float]] = {}

        # Callbacks
        self.on_bar_closed: Optional[BarClosedCallback] = None
        self.on_bar_update: Optional[BarUpdateCallback] = None

        # Async callbacks (for integration with asyncio event loop)
        self._async_bar_closed: Optional[Callable[[Bar], asyncio.Future]] = None
        self._async_bar_update: Optional[Callable[[Bar], asyncio.Future]] = None

        # Stats
        self._tick_count = 0
        self._bars_closed = 0

    def on_tick(self, tick: Tick):
        """
        Process a tick and update all timeframe bars.

        Args:
            tick: Tick data from DTC client
        """
        self._tick_count += 1
        symbol = tick.symbol

        # Ensure symbol tracking exists
        if symbol not in self._bars:
            self._bars[symbol] = {}
            self._history[symbol] = {}
            self._last_update_time[symbol] = {}
            for tf in self.timeframes:
                self._history[symbol][tf] = []
                self._last_update_time[symbol][tf] = 0

        # Convert tick timestamp to milliseconds
        tick_time_ms = (
            int(tick.timestamp * 1000)
            if tick.timestamp > 0
            else int(time.time() * 1000)
        )

        # Update each timeframe
        for tf in self.timeframes:
            self._update_bar(symbol, tf, tick.price, tick.volume, tick_time_ms)

    def _update_bar(
        self,
        symbol: str,
        timeframe: Timeframe,
        price: float,
        volume: float,
        tick_time_ms: int,
    ):
        """Update a specific timeframe bar with tick data."""
        bar_open_time = get_bar_open_time_ms(tick_time_ms, timeframe)

        current_bar = self._bars[symbol].get(timeframe)

        # Check if we need to close the current bar and start a new one
        if current_bar is not None and current_bar.bar_open_time_ms < bar_open_time:
            # Close the current bar
            closed_bar = current_bar.to_bar(is_closed=True)
            self._emit_bar_closed(closed_bar)

            # Add to history
            self._history[symbol][timeframe].append(closed_bar)
            if len(self._history[symbol][timeframe]) > self._max_history:
                self._history[symbol][timeframe].pop(0)

            current_bar = None

        # Create new bar if needed
        if current_bar is None:
            current_bar = BarState(
                symbol=symbol,
                timeframe=timeframe.value,
                bar_open_time_ms=bar_open_time,
                open=price,
                high=price,
                low=price,
                close=price,
            )
            self._bars[symbol][timeframe] = current_bar

        # Update the bar
        current_bar.update(price, volume)

        # Emit update if enabled and not throttled
        if self.emit_updates:
            now = time.time() * 1000
            last_emit = self._last_update_time[symbol][timeframe]
            if now - last_emit >= self.update_throttle_ms:
                self._emit_bar_update(current_bar.to_bar(is_closed=False))
                self._last_update_time[symbol][timeframe] = now

    def _emit_bar_closed(self, bar: Bar):
        """Emit bar closed event."""
        self._bars_closed += 1
        logger.debug(f"Bar closed: {bar.symbol} {bar.timeframe} @ {bar.close:.2f}")

        if self.on_bar_closed:
            try:
                self.on_bar_closed(bar)
            except Exception as e:
                logger.error(f"Error in bar_closed callback: {e}")

        if self._async_bar_closed:
            try:
                asyncio.create_task(self._async_bar_closed(bar))
            except RuntimeError:
                # No event loop running
                pass

    def _emit_bar_update(self, bar: Bar):
        """Emit bar update event."""
        if self.on_bar_update:
            try:
                self.on_bar_update(bar)
            except Exception as e:
                logger.error(f"Error in bar_update callback: {e}")

        if self._async_bar_update:
            try:
                asyncio.create_task(self._async_bar_update(bar))
            except RuntimeError:
                pass

    def set_async_callbacks(
        self,
        on_bar_closed: Optional[Callable[[Bar], asyncio.Future]] = None,
        on_bar_update: Optional[Callable[[Bar], asyncio.Future]] = None,
    ):
        """Set async callbacks for use with asyncio."""
        self._async_bar_closed = on_bar_closed
        self._async_bar_update = on_bar_update

    def get_current_bar(self, symbol: str, timeframe: str) -> Optional[Bar]:
        """Get the current (incomplete) bar for a symbol/timeframe."""
        tf = Timeframe(timeframe)
        if symbol in self._bars and tf in self._bars[symbol]:
            return self._bars[symbol][tf].to_bar(is_closed=False)
        return None

    def get_history(
        self,
        symbol: str,
        timeframe: str,
        count: Optional[int] = None,
    ) -> list[Bar]:
        """
        Get historical closed bars for a symbol/timeframe.

        Args:
            symbol: Symbol to get history for
            timeframe: Timeframe string (e.g., "1m")
            count: Optional limit on number of bars (most recent)

        Returns:
            List of closed bars, oldest first
        """
        tf = Timeframe(timeframe)
        if symbol not in self._history or tf not in self._history[symbol]:
            return []

        bars = self._history[symbol][tf]
        if count is not None and count < len(bars):
            return bars[-count:]
        return list(bars)

    def get_all_bars(self, symbol: str, timeframe: str) -> list[Bar]:
        """
        Get all bars (history + current) for a symbol/timeframe.

        Returns:
            List of bars including current incomplete bar
        """
        bars = self.get_history(symbol, timeframe)
        current = self.get_current_bar(symbol, timeframe)
        if current:
            bars = list(bars) + [current]
        return bars

    def get_candles_dict(self, symbol: str, timeframe: str) -> list[dict]:
        """Get all bars as dictionaries (for WebSocket transmission)."""
        return [bar.to_dict() for bar in self.get_all_bars(symbol, timeframe)]

    def get_symbols(self) -> list[str]:
        """Get list of symbols being tracked."""
        return list(self._bars.keys())

    def get_stats(self) -> dict:
        """Get builder statistics."""
        return {
            "tick_count": self._tick_count,
            "bars_closed": self._bars_closed,
            "symbols": self.get_symbols(),
            "timeframes": [tf.value for tf in self.timeframes],
            "history_sizes": {
                symbol: {
                    tf.value: len(self._history[symbol][tf]) for tf in self.timeframes
                }
                for symbol in self._history
            },
        }

    def clear_symbol(self, symbol: str):
        """Clear all data for a symbol."""
        self._bars.pop(symbol, None)
        self._history.pop(symbol, None)
        self._last_update_time.pop(symbol, None)

    def clear_all(self):
        """Clear all data."""
        self._bars.clear()
        self._history.clear()
        self._last_update_time.clear()
        self._tick_count = 0
        self._bars_closed = 0


class AsyncBarBuilder(BarBuilder):
    """
    Async-aware bar builder that integrates with asyncio event loops.

    Provides async methods for handling bar events and integrates with
    the WebSocket server's event-driven architecture.
    """

    def __init__(
        self,
        timeframes: list[str] = None,
        emit_updates: bool = True,
        update_throttle_ms: int = 100,
    ):
        super().__init__(timeframes, emit_updates, update_throttle_ms)

        # Event queue for bar closed events
        self._bar_closed_queue: asyncio.Queue[Bar] = asyncio.Queue()
        self._bar_update_queue: asyncio.Queue[Bar] = asyncio.Queue()

    def _emit_bar_closed(self, bar: Bar):
        """Override to also queue for async processing."""
        super()._emit_bar_closed(bar)
        try:
            self._bar_closed_queue.put_nowait(bar)
        except asyncio.QueueFull:
            logger.warning("Bar closed queue full, dropping event")

    def _emit_bar_update(self, bar: Bar):
        """Override to also queue for async processing."""
        super()._emit_bar_update(bar)
        try:
            self._bar_update_queue.put_nowait(bar)
        except asyncio.QueueFull:
            pass  # Updates can be dropped silently

    async def get_next_closed_bar(
        self, timeout: Optional[float] = None
    ) -> Optional[Bar]:
        """
        Wait for the next bar to close.

        Args:
            timeout: Optional timeout in seconds

        Returns:
            Closed bar, or None on timeout
        """
        try:
            if timeout:
                return await asyncio.wait_for(
                    self._bar_closed_queue.get(),
                    timeout=timeout,
                )
            return await self._bar_closed_queue.get()
        except asyncio.TimeoutError:
            return None

    async def get_next_bar_update(
        self, timeout: Optional[float] = None
    ) -> Optional[Bar]:
        """Wait for the next bar update."""
        try:
            if timeout:
                return await asyncio.wait_for(
                    self._bar_update_queue.get(),
                    timeout=timeout,
                )
            return await self._bar_update_queue.get()
        except asyncio.TimeoutError:
            return None

    async def process_bar_closed_events(
        self,
        handler: Callable[[Bar], asyncio.Future],
    ):
        """
        Continuously process bar closed events.

        Args:
            handler: Async function to call for each closed bar
        """
        while True:
            bar = await self._bar_closed_queue.get()
            try:
                await handler(bar)
            except Exception as e:
                logger.error(f"Error in bar closed handler: {e}")
            finally:
                self._bar_closed_queue.task_done()
