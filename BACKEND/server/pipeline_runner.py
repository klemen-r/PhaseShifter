"""
Pipeline runner - runs Rust core periodically and caches cluster/node results.
"""

import asyncio
import importlib.util
import json
import math
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from ws_server import WebSocketServer

# Path setup relative to server directory
SERVER_DIR = Path(__file__).resolve().parent
BACKEND_DIR = SERVER_DIR.parent
ROOT_DIR = BACKEND_DIR.parent
DATA_DIR = BACKEND_DIR / "data"
CORE_DIR = BACKEND_DIR / "phaseshifter-core"
SCRIPTS_DIR = BACKEND_DIR / "scripts"

# Default run configurations
DEFAULT_RUNS = [
    {"interval": "1m", "days": 8, "phase_window": 50, "depth_days": 8},
    {"interval": "5m", "days": 50, "phase_window": 7, "depth_days": 50},
    {"interval": "5m", "days": 50, "phase_window": 20, "depth_days": 50},
    {"interval": "15m", "days": 60, "phase_window": 10, "depth_days": 60},
]
MIN_UNIQUE_SCENARIOS = 1


class PipelineRunner:
    def __init__(
        self,
        server: "WebSocketServer",
        run_interval: int = 300,  # 5 minutes
    ):
        self.server = server
        self.run_interval = run_interval
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._cache: dict[str, dict] = {}
        self._fetch_module = None

    async def start(self):
        """Start the pipeline scheduler."""
        self._running = True
        self._load_fetch_module()
        self._task = asyncio.create_task(self._schedule_loop())
        print(f"Pipeline runner started (runs every {self.run_interval}s)")

    async def stop(self):
        """Stop the pipeline scheduler."""
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
        print("Pipeline runner stopped")

    def _load_fetch_module(self):
        """Load the fetch_price_data module."""
        fetch_path = DATA_DIR / "fetch_price_data.py"
        if not fetch_path.exists():
            print(f"[Pipeline] Warning: {fetch_path} not found")
            return

        spec = importlib.util.spec_from_file_location("fetch_price_data", fetch_path)
        if spec is None or spec.loader is None:
            print("[Pipeline] Warning: Could not load fetch_price_data.py")
            return

        module = importlib.util.module_from_spec(spec)
        try:
            spec.loader.exec_module(module)
            self._fetch_module = module
        except Exception as e:
            print(f"[Pipeline] Warning: Failed to load fetch module: {e}")

    async def _schedule_loop(self):
        """Main scheduler loop - runs pipeline for subscribed tickers."""
        while self._running:
            try:
                tickers = self.server.get_all_subscribed_tickers()
                if tickers:
                    for ticker in tickers:
                        await self.run_for_ticker(ticker)
            except Exception as e:
                print(f"[Pipeline] Error in schedule loop: {e}")

            await asyncio.sleep(self.run_interval)

    async def run_for_ticker(self, ticker: str) -> Optional[dict]:
        """Run the pipeline for a specific ticker."""
        loop = asyncio.get_event_loop()

        try:
            result = await loop.run_in_executor(
                None, lambda: self._run_pipeline_sync(ticker)
            )

            if result:
                self._cache[ticker] = result

                # Only broadcast to clients with auto-clusters enabled
                auto_subscribers = self.server.get_auto_cluster_subscribers(ticker)
                if auto_subscribers:
                    await self.server.broadcast_to_auto_cluster_clients(
                        ticker,
                        {
                            "type": "clusters",
                            "ticker": ticker,
                            "data": result,
                        },
                    )
                    print(f"[Pipeline] Broadcast clusters to {len(auto_subscribers)} auto-cluster clients for {ticker}")

            return result

        except Exception as e:
            print(f"[Pipeline] Error running for {ticker}: {e}")
            return None

    def _run_pipeline_sync(self, ticker: str) -> Optional[dict]:
        """Synchronous pipeline execution."""
        if not self._fetch_module:
            print(f"[Pipeline] Fetch module not loaded, skipping {ticker}")
            return None

        all_projections = []
        timestamp = datetime.utcnow().isoformat(timespec="seconds")

        for run in DEFAULT_RUNS:
            interval = run["interval"]
            days = run["days"]
            phase_window = run["phase_window"]
            depth_days = run["depth_days"]

            try:
                # Fetch data
                csv_path = self._fetch_module.fetch_data(ticker, interval, days)
                print(f"[Pipeline] Fetched {ticker} {interval} {days}d -> {csv_path}")

                # Purge old logs
                self._purge_logs()

                # Run Rust core
                self._run_core(csv_path, phase_window, depth_days, interval)

                # Get open nodes
                nodes = self._run_show_open_nodes(ticker)

                # Extract projections
                for node in nodes:
                    value = node.get("projected_price_now")
                    side = (node.get("side") or "").lower()
                    if value is None or side not in ("bullish", "bearish"):
                        continue

                    try:
                        value_num = float(value)
                    except (TypeError, ValueError):
                        continue

                    distance_pct = node.get("distance_pct")
                    pct_from_anchor = None
                    try:
                        pct_from_anchor = float(distance_pct) * 100.0
                    except (TypeError, ValueError):
                        pass

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

            except Exception as e:
                print(f"[Pipeline] Error in run {interval}/{phase_window}: {e}")
                continue

        anchor, raw_clusters = self._find_clusters(
            all_projections,
            min_unique_scenarios=MIN_UNIQUE_SCENARIOS,
        )
        clusters = self._dedupe_overlapping_clusters(raw_clusters)

        return {
            "ticker": ticker,
            "anchor": anchor,
            "clusters": clusters,
            "nodes": all_projections,
            "generated_at": timestamp,
        }

    def _purge_logs(self):
        """Remove old log files."""
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

    def _run_core(
        self,
        csv_path: Path,
        phase_window: int,
        depth_days: int,
        timeframe: str,
    ):
        """Run the Rust core."""
        cmd = [
            "cargo",
            "run",
            "--release",
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
        subprocess.run(cmd, cwd=CORE_DIR, check=True, capture_output=True)

    def _run_show_open_nodes(self, ticker: str) -> list[dict]:
        """Run show_open_nodes.py and get results."""
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

    def _find_clusters(
        self,
        all_projections: list[dict],
        min_unique_scenarios: int = 2,
    ) -> tuple[Optional[float], list[dict]]:
        """
        Anchor-normalized, adaptive-gap clustering for mixed-price assets.

        Steps:
        1) Compute per-node implied anchor and take the median across all nodes.
        2) Work in deviation space dev=(value-A)/A per side.
        3) Seed groups with a percentile-based gap and refine with adaptive scales.
        """
        from statistics import median

        ZERO_GAP_EPS = 1e-12

        def safe_float(value: object) -> Optional[float]:
            try:
                return float(value)
            except (TypeError, ValueError):
                return None

        def quantile(values: list[float], q: float) -> float:
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

        def compute_scale(gaps: list[float]) -> float:
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

        def compute_anchor(nodes: list[dict]) -> Optional[float]:
            anchors: list[float] = []
            for node in nodes:
                value = safe_float(node.get("value"))
                side = node.get("side")
                pct_raw = node.get("pct_from_anchor")
                if (
                    value is None
                    or side not in ("bullish", "bearish")
                    or pct_raw is None
                ):
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

        def make_gaps(sorted_nodes: list[dict]) -> list[float]:
            return [
                sorted_nodes[i + 1]["dev"] - sorted_nodes[i]["dev"]
                for i in range(len(sorted_nodes) - 1)
            ]

        anchor = compute_anchor(all_projections)
        if anchor is None or abs(anchor) < ZERO_GAP_EPS:
            return None, []

        enriched: list[dict] = []
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

        clusters: list[dict] = []
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

            seeds: list[list[dict]] = []
            current_seed: list[dict] = [side_nodes[0]]
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

                pruned = [n for n in seed if abs(n["dev"] - center) <= out_cut]
                if len(pruned) < 2:
                    continue

                pruned.sort(key=lambda n: n["dev"])
                pruned_gaps = make_gaps(pruned)
                s_split = compute_scale(pruned_gaps)
                core_gap = 2.0 * s_split
                max_width = 5.0 * s_split

                subcluster: list[dict] = [pruned[0]]
                subs: list[list[dict]] = []
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

                    scenarios = {
                        (n.get("interval"), n.get("phase_window")) for n in sub
                    }
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

    def _dedupe_overlapping_clusters(self, clusters: list[dict]) -> list[dict]:
        """Keep the strongest cluster per overlapping price region, per side."""

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

        result: list[dict] = []
        for side in ("bullish", "bearish"):
            clusters_side = [c for c in clusters if c["side"] == side]
            clusters_sorted = sorted(clusters_side, key=lambda c: (c["low"], c["high"]))
            group: list[dict] = []
            current_high: Optional[float] = None

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

    async def get_cached_data(self, ticker: str) -> Optional[dict]:
        """Get cached data for a ticker, or run pipeline if not cached."""
        if ticker in self._cache:
            return self._cache[ticker]

        # Run pipeline if not cached
        return await self.run_for_ticker(ticker)

    def get_cache_status(self) -> dict[str, dict]:
        """Get cache status for all tickers."""
        status = {}
        now = datetime.utcnow()

        for ticker, data in self._cache.items():
            generated_at = data.get("generated_at", "")
            age_seconds = 0
            try:
                gen_time = datetime.fromisoformat(generated_at)
                age_seconds = (now - gen_time).total_seconds()
            except (ValueError, TypeError):
                pass

            status[ticker] = {
                "generated_at": generated_at,
                "cluster_count": len(data.get("clusters", [])),
                "node_count": len(data.get("nodes", [])),
                "age_seconds": age_seconds,
            }

        return status

    def clear_cache(self):
        """Clear all cached data."""
        self._cache.clear()

    def clear_cache_for_ticker(self, ticker: str) -> bool:
        """Clear cache for a specific ticker. Returns True if ticker was in cache."""
        if ticker in self._cache:
            del self._cache[ticker]
            return True
        return False
