#!/usr/bin/env python3
"""
Fetch price data for a ticker across multiple timeframes, run the PhaseShifter
core for each scenario, and append the OPEN NODES section from show_open_nodes
into nodes.txt. Existing node/phase log files are deleted before every run to
avoid stale data.
"""

from __future__ import annotations

import importlib.util
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


def run_core(csv_path: Path, phase_window: int, depth_days: int, timeframe: str) -> None:
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


def run_show_open_nodes(ticker: str) -> str:
    cmd = [
        sys.executable,
        "show_open_nodes.py",
        "--nodes",
        str(DATA_DIR / "node_events.jsonl"),
        "--phases",
        str(DATA_DIR / "phase_updates.jsonl"),
        "--symbol",
        ticker,
    ]
    result = subprocess.run(
        cmd, cwd=SCRIPTS_DIR, text=True, capture_output=True, check=True
    )
    return result.stdout


def extract_open_nodes(stdout: str) -> str:
    lines = stdout.splitlines()
    start = next((idx for idx, line in enumerate(lines) if line.startswith("OPEN NODES")), None)
    if start is None:
        return ""
    return "\n".join(lines[start:]).strip()


def write_nodes_file(sections: List[Tuple[str, str]]) -> None:
    timestamp = datetime.now().isoformat(timespec="seconds")
    with NODES_OUTPUT.open("a", encoding="utf-8") as handle:
        for header, content in sections:
            handle.write(f"=== {timestamp} | {header} ===\n")
            handle.write(content.rstrip() or "OPEN NODES block not found.")
            handle.write("\n\n")


def main() -> None:
    ticker = sys.argv[1] if len(sys.argv) > 1 else "NQ=F"
    fetch_module = load_fetch_module()

    sections: List[Tuple[str, str]] = []
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

        stdout = run_show_open_nodes(ticker)
        open_nodes_block = extract_open_nodes(stdout)
        header = (
            f"{ticker.upper()} {interval} {days}d "
            f"phase_window={phase_window} depth_days={depth_days}"
        )
        sections.append((header, open_nodes_block))
        print(f"[nodes] captured OPEN NODES for {header}")

    write_nodes_file(sections)
    print(f"[done] appended results to {NODES_OUTPUT}")


if __name__ == "__main__":
    main()
