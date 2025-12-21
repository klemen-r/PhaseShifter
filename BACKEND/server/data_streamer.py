"""
yfinance data streaming - polls for 1m candles and pushes to subscribers.
"""

import asyncio
from datetime import datetime, timedelta
from typing import TYPE_CHECKING, Optional

import pandas as pd
import yfinance as yf

if TYPE_CHECKING:
    from ws_server import WebSocketServer


class DataStreamer:
    PRIMARY_INTERVAL = "1m"
    FALLBACK_INTERVAL = "5m"
    FLAT_RATIO_THRESHOLD = 0.9
    FLAT_MIN_SAMPLES = 30

    def __init__(self, server: "WebSocketServer", poll_interval: int = 60):
        self.server = server
        self.poll_interval = poll_interval  # seconds
        self._running = False
        self._task: Optional[asyncio.Task] = None

        # Track last candle time per ticker to avoid duplicates
        self._last_candle_time: dict[str, int] = {}
        self._preferred_interval: dict[str, str] = {}

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
            try:
                candles = await loop.run_in_executor(
                    None, lambda t=ticker: self._fetch_latest_candles(t)
                )

                if candles:
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
                print(f"[Streamer] Error fetching {ticker}: {e}")

    def _fetch_latest_candles(self, ticker: str) -> list[dict]:
        """Fetch latest 1m candles from yfinance."""
        try:
            interval = self._preferred_interval.get(ticker, self.PRIMARY_INTERVAL)
            data = self._download_data(ticker, period="1d", interval=interval)
            data, interval = self._apply_fallback_if_needed(
                ticker, data, interval, period="1d"
            )
            if data.empty:
                return []

            candles = []
            last_time = self._last_candle_time.get(ticker, 0)

            for idx, row in data.iterrows():
                # Convert timestamp to milliseconds
                ts = int(idx.timestamp() * 1000)

                # Skip if we've already sent this candle
                if ts <= last_time:
                    continue

                candle = {
                    "time": ts,
                    "open": float(row["Open"]),
                    "high": float(row["High"]),
                    "low": float(row["Low"]),
                    "close": float(row["Close"]),
                    "volume": int(row["Volume"]) if pd.notna(row["Volume"]) else 0,
                }
                candles.append(candle)

                # Update last candle time
                if ts > last_time:
                    self._last_candle_time[ticker] = ts

            return candles

        except Exception as e:
            print(f"[Streamer] yfinance error for {ticker}: {e}")
            return []

    def get_status(self) -> dict:
        """Get streamer status."""
        return {
            "running": self._running,
            "poll_interval": self.poll_interval,
            "active_tickers": list(self.server.get_all_subscribed_tickers()),
            "tracked_tickers": list(self._last_candle_time.keys()),
        }

    def reset_ticker(self, ticker: str):
        """Reset tracking for a ticker (will resend all candles)."""
        if ticker in self._last_candle_time:
            del self._last_candle_time[ticker]

    async def send_initial_data(self, client_id: str, ticker: str):
        """Fetch and send historical candles to a newly subscribed client."""
        loop = asyncio.get_event_loop()

        try:
            candles = await loop.run_in_executor(
                None, lambda: self._fetch_all_candles(ticker)
            )

            if candles:
                print(f"    → Sending {len(candles)} initial candles to {client_id}")
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
                print(f"    → No candle data available for {ticker}")

        except Exception as e:
            print(f"[Streamer] Error sending initial data for {ticker}: {e}")

    def _fetch_all_candles(self, ticker: str) -> list[dict]:
        """Fetch all available 1m candles from yfinance (1 day)."""
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
