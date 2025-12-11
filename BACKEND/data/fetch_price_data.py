#!/usr/bin/env python3
"""
Interactive helper to download OHLCV data and save it next to the existing CSVs.

It asks for:
- ticker (e.g., QQQ)
- timeframe/interval (e.g., 1m, 5m, 15m, 1h, 1d)
- number of calendar days to fetch

Output file will be named like <TICKER>_<INTERVAL>_<DAYS>d.csv with columns:
Datetime,Open,High,Low,Close,Volume
"""

import sys
from pathlib import Path

import pandas as pd

try:
    import yfinance as yf
except ImportError:
    print(
        "Missing dependency: yfinance. Install with `pip install yfinance`.",
        file=sys.stderr,
    )
    sys.exit(1)


SCRIPT_DIR = Path(__file__).resolve().parent
VALID_INTERVALS = {
    "1m",
    "2m",
    "5m",
    "15m",
    "30m",
    "60m",
    "90m",
    "1h",
    "1d",
    "5d",
    "1wk",
    "1mo",
    "3mo",
}


def prompt_user() -> tuple[str, str, int]:
    ticker = input("Enter ticker (e.g., QQQ): ").strip().upper()
    if not ticker:
        sys.exit("Ticker is required.")

    interval = input("Enter timeframe/interval (e.g., 1m, 5m, 15m, 1h, 1d): ").strip()
    if interval not in VALID_INTERVALS:
        print(
            f"[warn] Interval '{interval}' is not in the known set {sorted(VALID_INTERVALS)}; "
            "continuing anyway.",
            file=sys.stderr,
        )

    days_raw = input("Enter number of calendar days to fetch (e.g., 50): ").strip()
    try:
        days = int(days_raw)
    except ValueError:
        sys.exit("Days must be an integer.")
    if days <= 0:
        sys.exit("Days must be greater than zero.")

    return ticker, interval, days


def format_datetime_index(index) -> list[str]:
    """
    Format a pandas DateTimeIndex to match existing CSVs: YYYY-MM-DD HH:MM:SS+00:00
    """
    try:
        if index.tz is None:
            index = index.tz_localize("UTC")
        else:
            index = index.tz_convert("UTC")
    except Exception:
        # As a fallback, just coerce to UTC without TZ info.
        index = index.tz_localize(None).tz_localize("UTC")

    formatted = []
    for ts in index:
        raw = ts.strftime("%Y-%m-%d %H:%M:%S%z")  # -> 2025-01-01 00:00:00+0000
        formatted.append(f"{raw[:-2]}:{raw[-2:]}")  # -> 2025-01-01 00:00:00+00:00
    return formatted


def fetch_data(ticker: str, interval: str, days: int) -> Path:
    period = f"{days}d"
    df = yf.download(
        tickers=ticker,
        interval=interval,
        period=period,
        progress=False,
        auto_adjust=False,
        group_by="column",  # keep columns flat to avoid multi-row CSV headers
    )

    if df.empty:
        sys.exit("No data returned. Try a shorter period or different interval.")

    # Flatten any multi-index columns, strip ticker prefixes, and normalize names.
    if isinstance(df.columns, pd.MultiIndex):
        level_1_values = set(str(col[1]) for col in df.columns if len(col) > 1)
        if len(level_1_values) == 1:
            # Typical yfinance shape: first level is OHLCV, second level is ticker.
            df.columns = [str(col[0]) for col in df.columns]
        else:
            df.columns = [
                "_".join([str(part) for part in col if part]) for col in df.columns
            ]
    df = df.rename(
        columns={
            name: name[len(f"{ticker}_") :]
            if isinstance(name, str) and name.startswith(f"{ticker}_")
            else name
            for name in df.columns
        }
    )
    if "Adj Close" in df.columns and "Close" not in df.columns:
        df["Close"] = df["Adj Close"]

    # Keep only the columns we need and order them to match existing files.
    try:
        df = df[["Open", "High", "Low", "Close", "Volume"]]
    except KeyError as exc:
        sys.exit(f"Expected OHLCV columns missing from download: {exc}")

    df.index = format_datetime_index(df.index)
    df.index.name = "Datetime"

    output_path = SCRIPT_DIR / f"{ticker}_{interval}_{days}d.csv"
    df.to_csv(output_path)
    return output_path


def main() -> None:
    ticker, interval, days = prompt_user()
    output_path = fetch_data(ticker, interval, days)
    print(f"Wrote {output_path}")


if __name__ == "__main__":
    main()
