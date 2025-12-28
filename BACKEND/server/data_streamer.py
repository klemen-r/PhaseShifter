"""
yfinance data streaming - polls for 1m candles and pushes to subscribers.
"""

import asyncio
import logging
import re
import time
from dataclasses import dataclass
from datetime import datetime, timedelta
from typing import TYPE_CHECKING, Optional

import pandas as pd
import yfinance as yf

if TYPE_CHECKING:
    from ws_server import WebSocketServer

logger = logging.getLogger(__name__)

# Valid ticker pattern: alphanumeric, dashes, dots, equals, carets (e.g., BTC-USD, AAPL, ES=F, ^GSPC)
VALID_TICKER_PATTERN = re.compile(r'^[A-Za-z0-9\-\.=\^]{1,20}$')


@dataclass
class CachedCandles:
    """Cached candle data with timestamp for TTL checking."""
    candles: list[dict]
    fetched_at: float  # time.time()
    interval: str      # "1m" or "5m"


@dataclass
class TickerError:
    """Tracks errors for a ticker to implement backoff."""
    error_count: int
    last_error: float  # time.time()
    last_message: str


class DataStreamer:
    PRIMARY_INTERVAL = "1m"
    FALLBACK_INTERVAL = "5m"
    FLAT_RATIO_THRESHOLD = 0.9
    FLAT_MIN_SAMPLES = 30

    # Error backoff settings
    MAX_CONSECUTIVE_ERRORS = 5  # After this many errors, skip ticker for a while
    ERROR_BACKOFF_SECONDS = 300  # Skip erroring ticker for 5 minutes

    def __init__(self, server: "WebSocketServer", poll_interval: int = 60, cache_ttl: int = 30):
        self.server = server
        self.poll_interval = poll_interval  # seconds
        self._running = False
        self._task: Optional[asyncio.Task] = None

        # Track last candle time per ticker to avoid duplicates
        self._last_candle_time: dict[str, int] = {}
        self._preferred_interval: dict[str, str] = {}

        # Candle cache to avoid redundant yfinance calls
        self._candle_cache: dict[str, CachedCandles] = {}
        self._cache_ttl = cache_ttl  # seconds

        # Track errors per ticker for backoff
        self._ticker_errors: dict[str, TickerError] = {}

    def _is_valid_ticker(self, ticker: str) -> bool:
        """Check if ticker format is valid."""
        if not ticker or not isinstance(ticker, str):
            return False
        return VALID_TICKER_PATTERN.match(ticker.strip()) is not None

    def _should_skip_ticker(self, ticker: str) -> bool:
        """Check if ticker should be skipped due to repeated errors."""
        error_info = self._ticker_errors.get(ticker)
        if error_info is None:
            return False

        # If too many errors and not enough time passed, skip
        if error_info.error_count >= self.MAX_CONSECUTIVE_ERRORS:
            elapsed = time.time() - error_info.last_error
            if elapsed < self.ERROR_BACKOFF_SECONDS:
                return True
            # Backoff period passed, reset error count
            self._ticker_errors[ticker] = TickerError(0, 0, "")
        return False

    def _record_error(self, ticker: str, message: str):
        """Record an error for a ticker."""
        error_info = self._ticker_errors.get(ticker)
        if error_info:
            self._ticker_errors[ticker] = TickerError(
                error_count=error_info.error_count + 1,
                last_error=time.time(),
                last_message=message,
            )
        else:
            self._ticker_errors[ticker] = TickerError(
                error_count=1,
                last_error=time.time(),
                last_message=message,
            )

    def _clear_error(self, ticker: str):
        """Clear error tracking for a ticker on success."""
        if ticker in self._ticker_errors:
            del self._ticker_errors[ticker]

    async def start(self):
        """Start the data streaming loop."""
        self._running = True
        self._task = asyncio.create_task(self._poll_loop())
        print(f"Data streamer started (polling every {self.poll_interval}s)")

    async def stop(self):
        """Stop the data streaming loop."""
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
        print("Data streamer stopped")

    async def _poll_loop(self):
        """Main polling loop."""
        while self._running:
            try:
                tickers = self.server.get_all_subscribed_tickers()
                if tickers:
                    await self._fetch_and_broadcast(tickers)
            except Exception as e:
                print(f"[Streamer] Error in poll loop: {e}")

            await asyncio.sleep(self.poll_interval)

    async def _fetch_and_broadcast(self, tickers: set[str]):
        """Fetch latest candles for tickers and broadcast to subscribers."""
        # Run yfinance in executor to not block event loop
        loop = asyncio.get_event_loop()

        for ticker in tickers:
            # Skip invalid tickers
            if not self._is_valid_ticker(ticker):
                logger.warning(f"Skipping invalid ticker format: {ticker}")
                continue

            # Skip tickers in error backoff
            if self._should_skip_ticker(ticker):
                continue

            try:
                candles = await loop.run_in_executor(
                    None, lambda t=ticker: self._fetch_latest_candles(t)
                )

                if candles:
                    self._clear_error(ticker)  # Success, clear any error tracking
                    for candle in candles:
                        await self.server.broadcast_to_ticker(
                            ticker,
                            {
                                "type": "candle",
                                "ticker": ticker,
                                "data": candle,
                            },
                        )
            except Exception as e:
                error_msg = str(e)
                self._record_error(ticker, error_msg)
                error_info = self._ticker_errors.get(ticker)
                if error_info and error_info.error_count >= self.MAX_CONSECUTIVE_ERRORS:
                    logger.error(f"[Streamer] {ticker}: Too many errors, skipping for {self.ERROR_BACKOFF_SECONDS}s")
                else:
                    logger.warning(f"[Streamer] Error fetching {ticker}: {e}")

    def _fetch_latest_candles(self, ticker: str) -> list[dict]:
        """Fetch latest 1m candles from yfinance and update cache."""
        try:
            interval = self._preferred_interval.get(ticker, self.PRIMARY_INTERVAL)
            data = self._download_data(ticker, period="1d", interval=interval)
            data, interval = self._apply_fallback_if_needed(
                ticker, data, interval, period="1d"
            )
            if data.empty:
                return []

            all_candles = []
            new_candles = []
            last_time = self._last_candle_time.get(ticker, 0)

            for idx, row in data.iterrows():
                # Convert timestamp to milliseconds
                ts = int(idx.timestamp() * 1000)

                candle = {
                    "time": ts,
                    "open": float(row["Open"]),
                    "high": float(row["High"]),
                    "low": float(row["Low"]),
                    "close": float(row["Close"]),
                    "volume": int(row["Volume"]) if pd.notna(row["Volume"]) else 0,
                }
                all_candles.append(candle)

                # Only add to new_candles if after last sent time
                if ts > last_time:
                    new_candles.append(candle)
                    self._last_candle_time[ticker] = ts

            # Update cache with all candles (keeps cache fresh for new subscribers)
            if all_candles:
                self._set_cached_candles(ticker, all_candles, interval)

            return new_candles

        except Exception as e:
            print(f"[Streamer] yfinance error for {ticker}: {e}")
            return []

    def get_status(self) -> dict:
        """Get streamer status."""
        cache_info = {}
        now = time.time()
        for ticker, cached in self._candle_cache.items():
            age = now - cached.fetched_at
            cache_info[ticker] = {
                "candles": len(cached.candles),
                "interval": cached.interval,
                "age_seconds": round(age, 1),
                "valid": age <= self._cache_ttl,
            }

        # Error tracking info
        error_info = {}
        for ticker, err in self._ticker_errors.items():
            error_info[ticker] = {
                "error_count": err.error_count,
                "last_error_age": round(now - err.last_error, 1) if err.last_error else None,
                "last_message": err.last_message[:100] if err.last_message else None,
                "in_backoff": self._should_skip_ticker(ticker),
            }

        return {
            "running": self._running,
            "poll_interval": self.poll_interval,
            "cache_ttl": self._cache_ttl,
            "active_tickers": list(self.server.get_all_subscribed_tickers()),
            "tracked_tickers": list(self._last_candle_time.keys()),
            "cache": cache_info,
            "errors": error_info,
        }

    def clear_cache(self, ticker: str | None = None):
        """Clear candle cache. If ticker is None, clears all."""
        if ticker is None:
            self._candle_cache.clear()
        elif ticker in self._candle_cache:
            del self._candle_cache[ticker]

    def reset_ticker(self, ticker: str):
        """Reset tracking for a ticker (will resend all candles)."""
        if ticker in self._last_candle_time:
            del self._last_candle_time[ticker]
        if ticker in self._candle_cache:
            del self._candle_cache[ticker]

    def _get_cached_candles(self, ticker: str) -> list[dict] | None:
        """Return cached candles if valid, None if expired/missing."""
        cached = self._candle_cache.get(ticker)
        if cached is None:
            return None
        age = time.time() - cached.fetched_at
        if age > self._cache_ttl:
            return None
        return cached.candles

    def _set_cached_candles(self, ticker: str, candles: list[dict], interval: str):
        """Store candles in cache with current timestamp."""
        self._candle_cache[ticker] = CachedCandles(
            candles=candles,
            fetched_at=time.time(),
            interval=interval,
        )

    async def send_initial_data(self, client_id: str, ticker: str):
        """Fetch and send historical candles to a newly subscribed client."""
        # Validate ticker format
        if not self._is_valid_ticker(ticker):
            logger.warning(f"Invalid ticker format: {ticker}")
            await self.server.send_to_client(
                client_id,
                {
                    "type": "error",
                    "message": f"Invalid ticker format: {ticker}",
                    "ticker": ticker,
                },
            )
            return

        loop = asyncio.get_event_loop()

        try:
            # Check if we'll hit cache before fetching
            from_cache = self._get_cached_candles(ticker) is not None

            candles = await loop.run_in_executor(
                None, lambda: self._fetch_all_candles(ticker)
            )

            if candles:
                self._clear_error(ticker)  # Success
                source = "cached" if from_cache else "fetched"
                print(f"    → Sending {len(candles)} initial candles to {client_id} ({source})")
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
                # No data but not an error - ticker might exist but have no recent data
                print(f"    → No candle data available for {ticker}")
                await self.server.send_to_client(
                    client_id,
                    {
                        "type": "info",
                        "message": f"No data available for {ticker}. The symbol may be invalid or have no recent trading activity.",
                        "ticker": ticker,
                    },
                )

        except Exception as e:
            error_msg = str(e)
            self._record_error(ticker, error_msg)
            logger.error(f"[Streamer] Error sending initial data for {ticker}: {e}")
            await self.server.send_to_client(
                client_id,
                {
                    "type": "error",
                    "message": f"Failed to fetch data for {ticker}: {error_msg}",
                    "ticker": ticker,
                },
            )

    def _fetch_all_candles(self, ticker: str) -> list[dict]:
        """Fetch all available 1m candles from yfinance (1 day), using cache if valid."""
        # Check cache first
        cached = self._get_cached_candles(ticker)
        if cached is not None:
            return cached

        try:
            interval = self._preferred_interval.get(ticker, self.PRIMARY_INTERVAL)
            data = self._download_data(ticker, period="1d", interval=interval)
            data, interval = self._apply_fallback_if_needed(
                ticker, data, interval, period="1d"
            )
            if data.empty:
                return []

            candles = []
            for idx, row in data.iterrows():
                ts = int(idx.timestamp() * 1000)
                candle = {
                    "time": ts,
                    "open": float(row["Open"]),
                    "high": float(row["High"]),
                    "low": float(row["Low"]),
                    "close": float(row["Close"]),
                    "volume": int(row["Volume"]) if pd.notna(row["Volume"]) else 0,
                }
                candles.append(candle)

                # Update last candle time for this ticker
                if (
                    ticker not in self._last_candle_time
                    or ts > self._last_candle_time[ticker]
                ):
                    self._last_candle_time[ticker] = ts

            # Cache the fetched candles
            if candles:
                self._set_cached_candles(ticker, candles, interval)

            return candles

        except Exception as e:
            print(f"[Streamer] yfinance error for {ticker}: {e}")
            return []

    def _download_data(self, ticker: str, period: str, interval: str) -> pd.DataFrame:
        data = yf.download(
            ticker,
            period=period,
            interval=interval,
            progress=False,
            auto_adjust=False,
        )
        if data.empty:
            return data
        if isinstance(data.columns, pd.MultiIndex):
            data.columns = data.columns.get_level_values(0)
        return data

    def _apply_fallback_if_needed(
        self,
        ticker: str,
        data: pd.DataFrame,
        interval: str,
        period: str,
    ) -> tuple[pd.DataFrame, str]:
        if interval != self.PRIMARY_INTERVAL or data.empty:
            return data, interval

        if not self._should_fallback(data):
            return data, interval

        fallback = self._download_data(
            ticker, period=period, interval=self.FALLBACK_INTERVAL
        )
        if fallback.empty:
            return data, interval

        if self._preferred_interval.get(ticker) != self.FALLBACK_INTERVAL:
            print(f"[Streamer] {ticker} 1m data looks flat; switching to 5m candles.")
        self._preferred_interval[ticker] = self.FALLBACK_INTERVAL
        return fallback, self.FALLBACK_INTERVAL

    def _should_fallback(self, data: pd.DataFrame) -> bool:
        ohlc = data[["Open", "High", "Low", "Close"]].dropna(how="any")
        if len(ohlc) < self.FLAT_MIN_SAMPLES:
            return False
        flat = (ohlc["High"] == ohlc["Low"]).mean()
        return float(flat) >= self.FLAT_RATIO_THRESHOLD
