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
from statistics import median
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

CLUSTER_WIDTH_RATIO = 0.0006
CORE_GAP_RATIO = 0.0002
NEAR_ZERO_WIDTH = 1e-6


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


def find_clusters(
    all_projections: List[dict],
    ratio: float = CLUSTER_WIDTH_RATIO,
) -> List[dict]:
    """Identify tight price clusters per side using gap-based growth and outlier pruning."""

    def process_cluster(side: str, cluster: List[dict]) -> List[dict]:
        values = [row["value"] for row in cluster]
        center_val = float(median(values))
        max_width = center_val * ratio
        filtered = [
            row for row in cluster if abs(row["value"] - center_val) <= max_width * 0.3
        ]
        if len(filtered) < 2:
            return []

        filtered = sorted(filtered, key=lambda row: row["value"])
        center_filtered = float(median([row["value"] for row in filtered]))
        core_gap = center_filtered * CORE_GAP_RATIO
        sub_clusters: List[List[dict]] = []
        current_group: List[dict] = []
        for row in filtered:
            if not current_group:
                current_group = [row]
                continue
            gap = row["value"] - current_group[-1]["value"]
            if gap > core_gap:
                sub_clusters.append(current_group)
                current_group = [row]
            else:
                current_group.append(row)
        if current_group:
            sub_clusters.append(current_group)

        results: List[dict] = []
        for sub in sub_clusters:
            count = len(sub)
            unique_intervals = len({row["interval"] for row in sub})
            if count < 2:
                continue
            if unique_intervals < 2 and count < 3:
                continue
            sub_values = [row["value"] for row in sub]
            low = min(sub_values)
            high = max(sub_values)
            center_sub = float(median(sub_values))
            results.append(
                {
                    "side": side,
                    "center": center_sub,
                    "low": low,
                    "high": high,
                    "width": high - low,
                    "count": count,
                    "unique_intervals": unique_intervals,
                    "projections": sub,
                }
            )
        return results

    clusters: List[dict] = []
    grouped: dict[str, List[dict]] = {"bullish": [], "bearish": []}
    for projection in all_projections:
        side = projection.get("side")
        if side in grouped:
            grouped[side].append(projection)

    for side, projections in grouped.items():
        sorted_projections = sorted(projections, key=lambda row: row["value"])
        if not sorted_projections:
            continue

        seeds: List[List[dict]] = []
        for proj in sorted_projections:
            if not seeds:
                seeds.append([proj])
                continue
            current = seeds[-1]
            current_values = [row["value"] for row in current]
            current_center = float(median(current_values))
            current_core_gap = current_center * CORE_GAP_RATIO
            current_max_width = current_center * ratio
            last_val = current_values[-1]
            if (
                abs(proj["value"] - last_val) <= current_core_gap
                or abs(proj["value"] - current_center) <= current_max_width
            ):
                current.append(proj)
            else:
                seeds.append([proj])

        for seed in seeds:
            clusters.extend(process_cluster(side, seed))

    return clusters


def dedupe_overlapping_clusters(clusters: List[dict]) -> List[dict]:
    """Keep the strongest cluster per overlapping price region, per side."""

    def better(a: dict, b: dict) -> bool:
        return (
            a["width"],
            -a["count"],
            -a["unique_intervals"],
        ) < (
            b["width"],
            -b["count"],
            -b["unique_intervals"],
        )

    result: List[dict] = []
    for side in ("bullish", "bearish"):
        clusters_side = [c for c in clusters if c["side"] == side]
        clusters_sorted = sorted(clusters_side, key=lambda c: (c["low"], c["high"]))
        group: List[dict] = []
        current_high: float | None = None

        for cluster in clusters_sorted:
            if current_high is None or cluster["low"] > current_high:
                if group:
                    best = group[0]
                    for candidate in group[1:]:
                        if better(candidate, best):
                            best = candidate
                    result.append(best)
                group = [cluster]
                current_high = cluster["high"]
            else:
                group.append(cluster)
                current_high = max(current_high, cluster["high"])

        if group:
            best = group[0]
            for candidate in group[1:]:
                if better(candidate, best):
                    best = candidate
            result.append(best)

    return sorted(result, key=lambda c: c["low"])


def format_clusters_section(timestamp: str, clusters: List[dict]) -> str:
    lines = [
        "",
        f"=== {timestamp} | Experimental clusters (width <= price * {CLUSTER_WIDTH_RATIO}) ===",
    ]
    if not clusters:
        lines.append("none")
    else:
        for idx, cluster in enumerate(clusters, start=1):
            tf_phase = sorted(
                {
                    (row["interval"], row["phase_window"])
                    for row in cluster["projections"]
                }
            )
            tf_phase_str = "; ".join(
                f"{interval} pw={phase_window}" for interval, phase_window in tf_phase
            )
            lines.append(
                f"{idx}. {cluster['low']:.2f} - {cluster['high']:.2f} "
                f"({tf_phase_str}) [{cluster['side']}]"
            )
    lines.append("")
    lines.append("")
    lines.append("")
    return "\n".join(lines)


def write_nodes_file(
    timestamp: str,
    sections: List[Tuple[str, dict[str, List[float]]]],
    aggregate: str,
    clusters: str,
) -> None:
    with NODES_OUTPUT.open("a", encoding="utf-8") as handle:
        for header, content in sections:
            handle.write(format_run_section(timestamp, header, content))
        handle.write(aggregate)
        handle.write(clusters)


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
    raw_clusters = find_clusters(all_projections, CLUSTER_WIDTH_RATIO)
    clusters = dedupe_overlapping_clusters(raw_clusters)
    clusters_section = format_clusters_section(timestamp, clusters)
    write_nodes_file(timestamp, sections, aggregate, clusters_section)
    print(f"[done] appended results to {NODES_OUTPUT}")


if __name__ == "__main__":
    main()
