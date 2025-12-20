"""
Pipeline runner - runs Rust core periodically and caches cluster/node results.
"""

import asyncio
import importlib.util
import json
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
                None,
                lambda: self._run_pipeline_sync(ticker)
            )

            if result:
                self._cache[ticker] = result

                # Broadcast to subscribers
                await self.server.broadcast_to_ticker(ticker, {
                    "type": "clusters",
                    "ticker": ticker,
                    "data": result,
                })

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

                    all_projections.append({
                        "value": value_num,
                        "side": side,
                        "interval": interval,
                        "phase_window": phase_window,
                        "depth_days": depth_days,
                        "pct_from_anchor": pct_from_anchor,
                    })

            except Exception as e:
                print(f"[Pipeline] Error in run {interval}/{phase_window}: {e}")
                continue

        # Find clusters
        anchor, clusters = self._find_clusters(all_projections)

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
        Simplified cluster finding - group nearby projections.
        Uses anchor-normalized deviation space.
        """
        from statistics import median

        ZERO_GAP_EPS = 1e-12

        def safe_float(value) -> Optional[float]:
            try:
                return float(value)
            except (TypeError, ValueError):
                return None

        # Compute anchor from projections
        anchors = []
        for node in all_projections:
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
            return None, []

        anchor = float(median(anchors))

        # Group projections by side and find clusters
        clusters = []
        for side in ("bullish", "bearish"):
            side_nodes = [
                n for n in all_projections
                if n.get("side") == side and safe_float(n.get("value")) is not None
            ]
            if len(side_nodes) < 2:
                continue

            # Sort by value
            side_nodes.sort(key=lambda n: n["value"])

            # Simple gap-based clustering
            current_cluster = [side_nodes[0]]
            gap_threshold = anchor * 0.005  # 0.5% of anchor

            for i in range(1, len(side_nodes)):
                gap = side_nodes[i]["value"] - side_nodes[i - 1]["value"]
                if gap <= gap_threshold:
                    current_cluster.append(side_nodes[i])
                else:
                    if len(current_cluster) >= 2:
                        scenarios = {
                            (n.get("interval"), n.get("phase_window"))
                            for n in current_cluster
                        }
                        if len(scenarios) >= min_unique_scenarios:
                            values = [n["value"] for n in current_cluster]
                            clusters.append({
                                "side": side,
                                "low": min(values),
                                "high": max(values),
                                "count": len(current_cluster),
                                "unique_scenarios": len(scenarios),
                            })
                    current_cluster = [side_nodes[i]]

            # Don't forget the last cluster
            if len(current_cluster) >= 2:
                scenarios = {
                    (n.get("interval"), n.get("phase_window"))
                    for n in current_cluster
                }
                if len(scenarios) >= min_unique_scenarios:
                    values = [n["value"] for n in current_cluster]
                    clusters.append({
                        "side": side,
                        "low": min(values),
                        "high": max(values),
                        "count": len(current_cluster),
                        "unique_scenarios": len(scenarios),
                    })

        return anchor, sorted(clusters, key=lambda c: c["low"])

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

    def clear_cache(self, ticker: Optional[str] = None):
        """Clear cache for a ticker or all tickers."""
        if ticker:
            self._cache.pop(ticker, None)
        else:
            self._cache.clear()
