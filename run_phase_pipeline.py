#!/usr/bin/env python3
"""
Fetch price data for a ticker across multiple timeframes, run the PhaseShifter
core for each scenario, and append the OPEN NODES section from show_open_nodes
into nodes.txt. Existing node/phase log files are deleted before every run to
avoid stale data.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from statistics import median
from typing import List, Optional, Tuple

ROOT_DIR = Path(__file__).resolve().parent
DATA_DIR = ROOT_DIR / "BACKEND" / "data"
CORE_DIR = ROOT_DIR / "BACKEND" / "phaseshifter-core"
SCRIPTS_DIR = ROOT_DIR / "BACKEND" / "scripts"
NODES_OUTPUT = ROOT_DIR / "nodes.txt"
PINESCRIPT_CACHE = ROOT_DIR / "pinescript_clusters.json"

RUNS = [
    {"interval": "1m", "days": 8, "phase_window": 50, "depth_days": 8},
    {"interval": "5m", "days": 50, "phase_window": 7, "depth_days": 50},
    {"interval": "5m", "days": 50, "phase_window": 20, "depth_days": 50},
    {"interval": "15m", "days": 60, "phase_window": 10, "depth_days": 60},
]

ZERO_GAP_EPS = 1e-12
DEFAULT_MIN_UNIQUE_SCENARIOS = 2


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


def format_distance(distance_pct: float | None) -> str:
    try:
        return f"{float(distance_pct) * 100:.2f}%"
    except Exception:
        return "n/a"


def format_run_section(
    timestamp: str,
    header: str,
    projections_by_side: dict[str, List[dict]],
) -> str:
    lines: List[str] = [f"=== {timestamp} | {header} ==="]
    for side in ("bullish", "bearish"):
        projections = sorted(
            projections_by_side.get(side, []), key=lambda p: p["value"]
        )
        if not projections:
            lines.append(f"{side}: none")
        else:
            lines.append(f"{side}:")
            for proj in projections:
                lines.append(
                    f"  {proj['value']:.2f}    ({format_distance(proj.get('distance_pct'))})"
                )
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


def load_cluster_cache() -> dict:
    if not PINESCRIPT_CACHE.exists():
        return {}
    try:
        with PINESCRIPT_CACHE.open("r", encoding="utf-8") as handle:
            return json.load(handle)
    except Exception:
        return {}


def persist_cluster_cache(cache: dict) -> None:
    with PINESCRIPT_CACHE.open("w", encoding="utf-8") as handle:
        json.dump(cache, handle, ensure_ascii=False, indent=2)


def find_clusters(
    all_projections: List[dict],
    min_unique_scenarios: int = DEFAULT_MIN_UNIQUE_SCENARIOS,
) -> Tuple[Optional[float], List[dict]]:
    """
    Anchor-normalized, adaptive-gap clustering for mixed-price assets.

    Steps:
    1) Compute per-node implied anchor and take the median across all nodes.
    2) Work in deviation space dev=(value-A)/A per side.
    3) Seed groups with a percentile-based gap and refine with adaptive scales.
    """

    def safe_float(value: object) -> Optional[float]:
        try:
            return float(value)
        except (TypeError, ValueError):
            return None

    def quantile(values: List[float], q: float) -> float:
        if not values:
            raise ValueError("quantile requires non-empty list")
        if q <= 0.0:
            return min(values)
        if q >= 1.0:
            return max(values)
        sorted_vals = sorted(values)
        pos = (len(sorted_vals) - 1) * q
        lower = int(pos)
        upper = min(lower + 1, len(sorted_vals) - 1)
        if lower == upper:
            return sorted_vals[lower]
        fraction = pos - lower
        return sorted_vals[lower] * (1.0 - fraction) + sorted_vals[upper] * fraction

    def compute_scale(gaps: List[float]) -> float:
        gg = [abs(g) for g in gaps if abs(g) > ZERO_GAP_EPS]
        if not gg:
            return 0.0
        if len(gg) <= 3:
            gmin = min(gg)
            gmax = max(gg)
            if gmin > 0 and gmax / gmin > 3.5:
                return float(median(gg))
            return gmin
        q25 = quantile(gg, 0.25)
        filtered = [g for g in gg if g <= q25]
        base = filtered if filtered else gg
        return float(median(base))

    def compute_anchor(nodes: List[dict]) -> Optional[float]:
        anchors: List[float] = []
        for node in nodes:
            value = safe_float(node.get("value"))
            side = node.get("side")
            pct_raw = node.get("pct_from_anchor")
            if value is None or side not in ("bullish", "bearish") or pct_raw is None:
                continue
            try:
                pct = abs(float(pct_raw)) / 100.0
            except (TypeError, ValueError):
                continue
            sign = 1.0 if side == "bullish" else -1.0
            denom = 1.0 + sign * pct
            if abs(denom) < ZERO_GAP_EPS:
                continue
            anchors.append(value / denom)
        if not anchors:
            return None
        return float(median(anchors))

    def make_gaps(sorted_nodes: List[dict]) -> List[float]:
        return [
            sorted_nodes[i + 1]["dev"] - sorted_nodes[i]["dev"]
            for i in range(len(sorted_nodes) - 1)
        ]

    anchor = compute_anchor(all_projections)
    if anchor is None or abs(anchor) < ZERO_GAP_EPS:
        return None, []

    enriched: List[dict] = []
    for row in all_projections:
        value = safe_float(row.get("value"))
        if value is None:
            continue
        side = row.get("side")
        if side not in ("bullish", "bearish"):
            continue
        dev = (value - anchor) / anchor
        enriched.append(
            {
                **row,
                "dev": dev,
                "anchor": anchor,
                "scenario": (row.get("interval"), row.get("phase_window")),
            }
        )

    clusters: List[dict] = []
    for side in ("bullish", "bearish"):
        side_nodes = [n for n in enriched if n["side"] == side]
        if len(side_nodes) < 2:
            continue
        side_nodes.sort(key=lambda n: n["dev"])

        gaps = make_gaps(side_nodes)
        nonzero_gaps = [g for g in gaps if abs(g) > ZERO_GAP_EPS]
        if not nonzero_gaps:
            continue

        sorted_gaps = sorted(nonzero_gaps)
        k = len(sorted_gaps)
        start_idx = max(0, int(math.floor(k * 0.2)))
        end_idx = max(start_idx + 1, int(math.ceil(k * 0.5)))
        end_idx = min(end_idx, k)
        seed_slice = sorted_gaps[start_idx:end_idx]
        s_seed = float(median(seed_slice if seed_slice else sorted_gaps))
        seed_gap = s_seed

        seeds: List[List[dict]] = []
        current_seed: List[dict] = [side_nodes[0]]
        for i in range(1, len(side_nodes)):
            gap = side_nodes[i]["dev"] - side_nodes[i - 1]["dev"]
            if gap <= seed_gap:
                current_seed.append(side_nodes[i])
            else:
                seeds.append(current_seed)
                current_seed = [side_nodes[i]]
        if current_seed:
            seeds.append(current_seed)

        for seed in seeds:
            if len(seed) < 2:
                continue

            seed_gaps = make_gaps(seed)
            s_core = compute_scale(seed_gaps)
            center = float(median([n["dev"] for n in seed]))
            out_cut = 2.2 * s_core

            # Keep points inside the outlier cut
            pruned = [n for n in seed if abs(n["dev"] - center) <= out_cut]
            if len(pruned) < 2:
                continue

            pruned.sort(key=lambda n: n["dev"])
            pruned_gaps = make_gaps(pruned)
            s_split = compute_scale(pruned_gaps)
            core_gap = 2.0 * s_split
            max_width = 5.0 * s_split

            subcluster: List[dict] = [pruned[0]]
            subs: List[List[dict]] = []
            for j in range(1, len(pruned)):
                gap = pruned[j]["dev"] - pruned[j - 1]["dev"]
                if gap <= core_gap:
                    subcluster.append(pruned[j])
                else:
                    subs.append(subcluster)
                    subcluster = [pruned[j]]
            if subcluster:
                subs.append(subcluster)

            for sub in subs:
                if len(sub) < 2:
                    continue

                scenarios = {(n.get("interval"), n.get("phase_window")) for n in sub}
                unique_scenarios = len(scenarios)
                if unique_scenarios < min_unique_scenarios:
                    continue

                devs = [n["dev"] for n in sub]
                width_dev = max(devs) - min(devs)
                if width_dev > max_width:
                    continue

                low = anchor * (1.0 + min(devs))
                high = anchor * (1.0 + max(devs))
                clusters.append(
                    {
                        "side": side,
                        "low": low,
                        "high": high,
                        "width": high - low,
                        "count": len(sub),
                        "unique_scenarios": unique_scenarios,
                        "projections": sub,
                        "anchor": anchor,
                        "center_dev": center,
                        "width_dev": width_dev,
                    }
                )

    return anchor, clusters


def dedupe_overlapping_clusters(clusters: List[dict]) -> List[dict]:
    """Keep the strongest cluster per overlapping price region, per side (narrowest width, then scenarios, then count)."""

    def better(a: dict, b: dict) -> bool:
        return (
            a["width"],
            -a["unique_scenarios"],
            -a["count"],
        ) < (
            b["width"],
            -b["unique_scenarios"],
            -b["count"],
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


def format_clusters_section(
    timestamp: str, anchor: Optional[float], clusters: List[dict]
) -> str:
    anchor_display = f"{anchor:.2f}" if anchor is not None else "n/a"
    lines = [
        "",
        (
            f"=== {timestamp} | Anchor-normalized clusters "
            f"(anchor median={anchor_display}) ==="
        ),
    ]
    if not clusters:
        lines.append("none")
    else:
        for idx, cluster in enumerate(clusters, start=1):
            members = sorted(cluster["projections"], key=lambda r: r["value"])
            scenario_set = sorted(
                {(m.get("interval"), m.get("phase_window")) for m in members}
            )
            scenario_str = "; ".join(
                f"{interval} pw={phase_window}"
                for interval, phase_window in scenario_set
            )
            lines.append(
                f"{idx}. {cluster['low']:.2f} - {cluster['high']:.2f} "
                f"(w={cluster['width']:.2f}, dev_w={cluster.get('width_dev', 0):.6f}) "
                f"[{cluster['side']}] count={cluster['count']} "
                f"scenarios={cluster['unique_scenarios']} ({scenario_str})"
            )
            for m in members:
                pct = m.get("pct_from_anchor")
                pct_display = f"{pct:.2f}%" if isinstance(pct, (int, float)) else "n/a"
                lines.append(
                    f"    - {m['value']:.2f} dev={m['dev']:+.6f} "
                    f"({m.get('interval')}, pw={m.get('phase_window')}) "
                    f"dist_from_anchor={pct_display}"
                )
    lines.append("")
    lines.append("")
    lines.append("")
    return "\n".join(lines)


def write_nodes_file(
    timestamp: str,
    sections: List[Tuple[str, dict[str, List[dict]]]],
    aggregate: str,
    clusters: str,
    pinescript: str,
) -> None:
    with NODES_OUTPUT.open("a", encoding="utf-8") as handle:
        for header, content in sections:
            handle.write(format_run_section(timestamp, header, content))
        handle.write(aggregate)
        handle.write(clusters)
        handle.write(pinescript)


def parse_timestamp_parts(
    ts: Optional[str],
) -> Optional[Tuple[int, int, int, int, int, int]]:
    if not ts:
        return None
    try:
        dt = datetime.fromisoformat(ts)
    except ValueError:
        return None
    return (dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second)


def format_pinescript_indicator(cache: dict, generated_at: str) -> str:
    zones: List[dict] = []
    for symbol, payload in sorted(cache.items()):
        clusters = payload.get("clusters") or []
        anchor = payload.get("anchor")
        ts_parts = parse_timestamp_parts(payload.get("timestamp"))
        if ts_parts is None:
            continue
        for cluster in clusters:
            low = cluster.get("low")
            high = cluster.get("high")
            side = cluster.get("side")
            if low is None or high is None or side not in ("bullish", "bearish"):
                continue
            zones.append(
                {
                    "symbol": symbol,
                    "low": float(low),
                    "high": float(high),
                    "side": side,
                    "year": ts_parts[0],
                    "month": ts_parts[1],
                    "day": ts_parts[2],
                    "hour": ts_parts[3],
                    "minute": ts_parts[4],
                    "second": ts_parts[5],
                    "anchor": anchor,
                }
            )

    header = f"=== {generated_at} | PineScript v6 indicator (anchor clusters) ==="
    if not zones:
        return f"{header}\nno clusters available for indicator\n\n"

    lines: List[str] = []
    lines.append(header)
    lines.append("```pine")
    lines.append(
        f"// Auto-generated at {generated_at}. Paste into TradingView Pine Editor."
    )
    lines.append("//@version=5")
    lines.append(
        'indicator("PhaseShifter Anchor Clusters", overlay=true, max_boxes_count=500)'
    )
    lines.append("type Zone")
    lines.append("    string symbol")
    lines.append("    float low")
    lines.append("    float high")
    lines.append("    int year")
    lines.append("    int month")
    lines.append("    int day")
    lines.append("    int hour")
    lines.append("    int minute")
    lines.append("    int second")
    lines.append("    string side")
    lines.append("    float anchor")
    lines.append("")
    lines.append("var zones = array.new<Zone>()")
    lines.append("if barstate.isfirst")
    lines.append("    array.clear(zones)")
    for zone in zones:
        lines.append(
            "    array.push(zones, Zone.new("
            f'"{zone["symbol"]}", '
            f"{zone['low']:.4f}, {zone['high']:.4f}, "
            f"{zone['year']}, {zone['month']}, {zone['day']}, "
            f"{zone['hour']}, {zone['minute']}, {zone['second']}, "
            f'"{zone["side"]}", {0.0 if zone["anchor"] is None else float(zone["anchor"]):.4f}))'
        )
    lines.append("")
    lines.append('tz = input.string("Europe/Ljubljana", "Timestamp timezone")')
    symbols = sorted({z["symbol"] for z in zones})
    input_map = []
    for sym in symbols:
        var_name = f"show_{sym.replace('-', '_').replace('/', '_').replace(':', '_')}"
        input_map.append((sym, var_name))
        lines.append(f'{var_name} = input.bool(true, "Show {sym} zones")')
    lines.append("")
    lines.append("border_col = color.new(color.gray, 60)")
    lines.append("fill_col   = color.new(color.gray, 60)")
    lines.append("")
    lines.append("var boxes = array.new_box()")
    lines.append("var bool built = false")
    lines.append("zone_count = array.size(zones)")
    lines.append("if not built and barstate.islast and zone_count > 0")
    lines.append("    for i = 0 to zone_count - 1")
    lines.append("        z = array.get(zones, i)")
    lines.append("        show = true")
    for sym, var_name in input_map:
        lines.append(f'        show := show and (z.symbol != "{sym}" or {var_name})')
    lines.append(
        "        start_ts = timestamp(tz, z.year, z.month, z.day, z.hour, z.minute, z.second)"
    )
    lines.append("        if show")
    lines.append("            left_ts  = math.min(start_ts, time)")
    lines.append("            right_ts = time")
    lines.append(
        "            b = box.new(left_ts, z.high, right_ts, z.low, xloc=xloc.bar_time, "
        "extend=extend.right, border_color=border_col, bgcolor=fill_col)"
    )
    lines.append(
        '            box.set_text(b, str.format("{0} {1} [{2,number,#.####}-{3,number,#.####}] anchor={4,number,#.####}", '
        "str.upper(z.side), z.symbol, z.low, z.high, z.anchor))"
    )
    lines.append("            array.push(boxes, b)")
    lines.append("            built := true")
    lines.append("")
    lines.append("// Keep boxes extended to the latest bar")
    lines.append("box_count = array.size(boxes)")
    lines.append("if box_count > 0")
    lines.append("    for i = 0 to box_count - 1")
    lines.append("        box.set_right(array.get(boxes, i), time)")
    lines.append("```")
    lines.append("")
    lines.append("")
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Fetch price data, run PhaseShifter core, and cluster open nodes."
    )
    parser.add_argument(
        "ticker",
        nargs="?",
        default="NQ=F",
        help="Symbol/ticker to process (default: NQ=F)",
    )
    parser.add_argument(
        "--allow-single-scenario",
        action="store_true",
        help="Disable cross-scenario validation; allow clusters with a single timeframe/phase_window.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    ticker = args.ticker
    min_unique_scenarios = (
        1 if args.allow_single_scenario else DEFAULT_MIN_UNIQUE_SCENARIOS
    )
    fetch_module = load_fetch_module()

    sections: List[Tuple[str, dict[str, List[dict]]]] = []
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
        projections_by_side: dict[str, List[dict]] = {"bullish": [], "bearish": []}
        for node in nodes:
            value = node.get("projected_price_now")
            side = (node.get("side") or "").lower()
            if value is None or side not in projections_by_side:
                continue
            try:
                value_num = float(value)
            except (TypeError, ValueError):
                continue
            distance_pct = node.get("distance_pct")
            pct_from_anchor: Optional[float] = None
            try:
                pct_from_anchor = float(distance_pct) * 100.0
            except (TypeError, ValueError):
                pass
            projections_by_side[side].append(
                {"value": value_num, "distance_pct": distance_pct}
            )
            all_projections.append(
                {
                    "value": value_num,
                    "side": side,
                    "interval": interval,
                    "phase_window": phase_window,
                    "depth_days": depth_days,
                    "pct_from_anchor": pct_from_anchor,
                }
            )

        header = (
            f"{ticker.upper()} {interval} {days}d "
            f"phase_window={phase_window} depth_days={depth_days}"
        )
        sections.append((header, projections_by_side))
        print(f"[nodes] captured projected values for {header}")

    aggregate = format_aggregate_section(timestamp, all_projections)
    anchor, raw_clusters = find_clusters(
        all_projections, min_unique_scenarios=min_unique_scenarios
    )
    clusters = dedupe_overlapping_clusters(raw_clusters)
    clusters_section = format_clusters_section(timestamp, anchor, clusters)
    cache = load_cluster_cache()
    cache[ticker.upper()] = {
        "timestamp": timestamp,
        "anchor": anchor,
        "clusters": [
            {"side": c["side"], "low": c["low"], "high": c["high"]} for c in clusters
        ],
    }
    persist_cluster_cache(cache)
    pinescript_block = format_pinescript_indicator(cache, generated_at=timestamp)
    write_nodes_file(timestamp, sections, aggregate, clusters_section, pinescript_block)
    print(f"[done] appended results to {NODES_OUTPUT}")


if __name__ == "__main__":
    main()
