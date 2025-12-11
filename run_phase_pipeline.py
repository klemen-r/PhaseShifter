#!/usr/bin/env python3
"""
Fetch price data for a ticker across multiple timeframes, run the PhaseShifter
core for each scenario, and append the OPEN NODES section from show_open_nodes
into nodes.txt. Existing node/phase log files are deleted before every run to
avoid stale data.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import List, Tuple

ROOT_DIR = Path(__file__).resolve().parent
DATA_DIR = ROOT_DIR / "BACKEND" / "data"
CORE_DIR = ROOT_DIR / "BACKEND" / "phaseshifter-core"
SCRIPTS_DIR = ROOT_DIR / "BACKEND" / "scripts"
NODES_OUTPUT = ROOT_DIR / "nodes.txt"

RUNS = [
    {"interval": "1m", "days": 8, "phase_window": 50, "depth_days": 8},
    {"interval": "5m", "days": 50, "phase_window": 7, "depth_days": 50},
    {"interval": "5m", "days": 50, "phase_window": 20, "depth_days": 50},
    {"interval": "15m", "days": 60, "phase_window": 10, "depth_days": 60},
]


def load_fetch_module():
    spec = importlib.util.spec_from_file_location(
        "fetch_price_data", DATA_DIR / "fetch_price_data.py"
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("Could not load fetch_price_data.py")

    module = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(module)
    except SystemExit as exc:
        raise RuntimeError(
            "Loading fetch_price_data.py failed (missing dependency like yfinance?)."
        ) from exc
    return module


def fetch_price_data(module, ticker: str, interval: str, days: int) -> Path:
    ticker_clean = ticker.strip().upper()
    if not ticker_clean:
        raise ValueError("Ticker cannot be empty.")
    return module.fetch_data(ticker_clean, interval, days)


def purge_logs() -> None:
    for path in [
        DATA_DIR / "node_events.json",
        DATA_DIR / "node_events.jsonl",
        DATA_DIR / "phase_updates.json",
        DATA_DIR / "phase_updates.jsonl",
    ]:
        try:
            path.unlink()
        except FileNotFoundError:
            pass


def run_core(
    csv_path: Path, phase_window: int, depth_days: int, timeframe: str
) -> None:
    cmd = [
        "cargo",
        "run",
        "--",
        "--csv",
        str(csv_path),
        "--phase-window",
        str(phase_window),
        "--depth-days",
        str(depth_days),
        "--timeframe",
        timeframe,
        "--out-json",
        str(DATA_DIR / "phase_updates.jsonl"),
        "--node-events-log",
        str(DATA_DIR / "node_events.jsonl"),
    ]
    subprocess.run(cmd, cwd=CORE_DIR, check=True)


def run_show_open_nodes(ticker: str) -> List[dict]:
    cmd = [
        sys.executable,
        "show_open_nodes.py",
        "--nodes",
        str(DATA_DIR / "node_events.jsonl"),
        "--phases",
        str(DATA_DIR / "phase_updates.jsonl"),
        "--symbol",
        ticker,
        "--raw",
    ]
    result = subprocess.run(
        cmd, cwd=SCRIPTS_DIR, text=True, capture_output=True, check=True
    )
    return json.loads(result.stdout or "[]")


def format_run_section(
    timestamp: str,
    header: str,
    projections_by_side: dict[str, List[float]],
) -> str:
    lines: List[str] = [f"=== {timestamp} | {header} ==="]
    for side in ("bullish", "bearish"):
        values = sorted(projections_by_side.get(side, []))
        if not values:
            lines.append(f"{side}: none")
        else:
            lines.append(f"{side}:")
            lines.extend(f"  {value:.2f}" for value in values)
    lines.append("")
    return "\n".join(lines)


def format_aggregate_section(timestamp: str, all_projections: List[dict]) -> str:
    lines = ["", f"=== {timestamp} | All projections (ascending) ==="]
    if not all_projections:
        lines.append("none")
    else:
        for item in sorted(all_projections, key=lambda row: row["value"]):
            lines.append(
                f"{item['value']:.2f} ({item['side']}, {item['interval']}, "
                f"phase_window={item['phase_window']}, depth_days={item['depth_days']})"
            )
    lines.append("")
    lines.append("")
    return "\n".join(lines)


def write_nodes_file(sections: List[Tuple[str, str]], aggregate: str) -> None:
    timestamp = datetime.now().isoformat(timespec="seconds")
    with NODES_OUTPUT.open("a", encoding="utf-8") as handle:
        for header, content in sections:
            handle.write(format_run_section(timestamp, header, content))
        handle.write(aggregate)


def main() -> None:
    ticker = sys.argv[1] if len(sys.argv) > 1 else "NQ=F"
    fetch_module = load_fetch_module()

    sections: List[Tuple[str, dict[str, List[float]]]] = []
    all_projections: List[dict] = []
    timestamp = datetime.now().isoformat(timespec="seconds")
    for run in RUNS:
        interval = run["interval"]
        days = run["days"]
        phase_window = run["phase_window"]
        depth_days = run["depth_days"]

        csv_path = fetch_price_data(fetch_module, ticker, interval, days)
        print(f"[fetch] {ticker} {interval} {days}d -> {csv_path}")

        purge_logs()
        print("[cleanup] cleared node_events/phase_updates logs")

        run_core(csv_path, phase_window, depth_days, interval)
        print(f"[core] completed run for {interval}, phase_window={phase_window}")

        nodes = run_show_open_nodes(ticker)
        projections_by_side: dict[str, List[float]] = {"bullish": [], "bearish": []}
        for node in nodes:
            value = node.get("projected_price_now")
            side = (node.get("side") or "").lower()
            if value is None or side not in projections_by_side:
                continue
            try:
                value_num = float(value)
            except (TypeError, ValueError):
                continue
            projections_by_side[side].append(value_num)
            all_projections.append(
                {
                    "value": value_num,
                    "side": side,
                    "interval": interval,
                    "phase_window": phase_window,
                    "depth_days": depth_days,
                }
            )

        header = (
            f"{ticker.upper()} {interval} {days}d "
            f"phase_window={phase_window} depth_days={depth_days}"
        )
        sections.append((header, projections_by_side))
        print(f"[nodes] captured projected values for {header}")

    aggregate = format_aggregate_section(timestamp, all_projections)
    write_nodes_file(sections, aggregate)
    print(f"[done] appended results to {NODES_OUTPUT}")


if __name__ == "__main__":
    main()
