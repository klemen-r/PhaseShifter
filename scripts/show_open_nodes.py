import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

ROOT_DIR = Path(__file__).resolve().parent.parent
DEFAULT_NODES_PATH = ROOT_DIR / "data" / "node_events.jsonl"
DEFAULT_PHASES_PATH = ROOT_DIR / "data" / "phase_updates.jsonl"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Show open PhaseShifter nodes with projected prices using node_events.jsonl."
    )
    parser.add_argument(
        "--nodes",
        type=Path,
        default=DEFAULT_NODES_PATH,
        help="Path to node_events.jsonl (default: data/node_events.jsonl)",
    )
    parser.add_argument(
        "--phases",
        type=Path,
        default=DEFAULT_PHASES_PATH,
        help=(
            "Optional path to phase_updates.jsonl (default: data/phase_updates.jsonl); "
            "used to auto-detect the latest anchor when --anchor is not provided."
        ),
    )
    parser.add_argument("--symbol", help="Filter by symbol (optional)")
    parser.add_argument(
        "--anchor",
        type=float,
        help="Current anchor to project targets: bullish P=anchor*(1+dist), bearish P=anchor*(1-dist).",
    )
    parser.add_argument(
        "--side",
        choices=["bullish", "bearish"],
        help="Filter open nodes by direction.",
    )
    parser.add_argument(
        "--limit",
        type=int,
        help="Show only the N most recent open nodes.",
    )
    parser.add_argument(
        "--raw",
        action="store_true",
        help="Print raw JSON for open nodes instead of a pretty view.",
    )
    return parser.parse_args()


def warn(message: str) -> None:
    print(f"[warn] {message}", file=sys.stderr)


def load_open_nodes(
    path: Path, symbol: Optional[str]
) -> Tuple[List[Dict[str, Any]], List[str]]:
    nodes: Dict[Any, Dict[str, Any]] = {}
    warnings: List[str] = []

    try:
        with path.open("r", encoding="utf-8") as handle:
            for line_no, raw_line in enumerate(handle, 1):
                line = raw_line.strip()
                if not line:
                    continue

                try:
                    event = json.loads(line)
                except json.JSONDecodeError as exc:
                    warnings.append(f"{path}: line {line_no}: invalid JSON ({exc})")
                    continue

                if event.get("type") != "node_event":
                    continue
                if symbol and event.get("symbol") != symbol:
                    continue

                event_name = event.get("event")
                node_id = event.get("created_at")
                if node_id is None:
                    warnings.append(
                        f"{path}: line {line_no}: missing created_at; skipping"
                    )
                    continue

                if event_name == "created":
                    side = event.get("side")
                    distance_pct = event.get("distance_pct")
                    if side is None or distance_pct is None:
                        warnings.append(
                            f"{path}: line {line_no}: missing side/distance_pct; skipping"
                        )
                        continue

                    nodes[node_id] = {
                        "side": side,
                        "distance_pct": distance_pct,
                        "created_from_anchor": event.get("created_from_anchor"),
                        "created_from_extreme": event.get("created_from_extreme"),
                        "created_at": node_id,
                        "status": "open",
                        "symbol": event.get("symbol"),
                    }
                elif event_name == "closed":
                    if node_id not in nodes:
                        warnings.append(
                            f"{path}: line {line_no}: close for unknown node {node_id}; skipping"
                        )
                        continue
                    nodes[node_id].update(
                        {
                            "status": "closed",
                            "closed_at": event.get("closed_at"),
                            "last_anchor_used": event.get("last_anchor_used"),
                            "last_projected_target": event.get("last_projected_target"),
                        }
                    )
                else:
                    warnings.append(
                        f"{path}: line {line_no}: unexpected event '{event_name}'; skipping"
                    )
    except FileNotFoundError:
        print(f"[error] Nodes file not found: {path}", file=sys.stderr)
        sys.exit(1)
    except OSError as exc:
        print(f"[error] Could not read nodes file {path}: {exc}", file=sys.stderr)
        sys.exit(1)

    open_nodes = [n for n in nodes.values() if n.get("status") == "open"]
    return open_nodes, warnings


def load_latest_anchor(
    phases_path: Optional[Path], symbol: Optional[str]
) -> Tuple[Optional[float], List[str]]:
    if phases_path is None:
        return None, []

    warnings: List[str] = []
    latest_time: Optional[float] = None
    latest_anchor: Optional[float] = None

    if not phases_path.is_file():
        warnings.append(f"{phases_path}: phase_updates file not found")
        return None, warnings

    try:
        with phases_path.open("r", encoding="utf-8") as handle:
            for line_no, raw_line in enumerate(handle, 1):
                line = raw_line.strip()
                if not line:
                    continue

                try:
                    event = json.loads(line)
                except json.JSONDecodeError as exc:
                    warnings.append(
                        f"{phases_path}: line {line_no}: invalid JSON ({exc}); skipping"
                    )
                    continue

                if event.get("type") != "phase_update":
                    continue
                if symbol and event.get("symbol") != symbol:
                    continue

                event_time = event.get("time")
                anchor_value = event.get("anchor")
                if event_time is None or anchor_value is None:
                    warnings.append(
                        f"{phases_path}: line {line_no}: missing time/anchor; skipping"
                    )
                    continue

                try:
                    event_time_num = float(event_time)
                    anchor_num = float(anchor_value)
                except (TypeError, ValueError):
                    warnings.append(
                        f"{phases_path}: line {line_no}: non-numeric time/anchor; skipping"
                    )
                    continue

                if latest_time is None or event_time_num > latest_time:
                    latest_time = event_time_num
                    latest_anchor = anchor_num
    except OSError as exc:
        warnings.append(f"{phases_path}: could not read file ({exc})")

    return latest_anchor, warnings


def project_nodes(open_nodes: List[Dict[str, Any]], anchor: Optional[float]) -> None:
    if anchor is None:
        return
    for node in open_nodes:
        node["current_anchor"] = anchor
        projected: Optional[float] = None
        raw_distance = node.get("distance_pct")
        if raw_distance is None:
            node["projected_price_now"] = None
            continue
        try:
            distance = float(raw_distance)
        except (TypeError, ValueError):
            node["projected_price_now"] = None
            continue

        side = node.get("side")
        if side == "bullish":
            projected = anchor * (1.0 + distance)
        elif side == "bearish":
            projected = anchor * (1.0 - distance)
        node["projected_price_now"] = projected


def format_timestamp(ms_value: Any) -> str:
    if ms_value is None:
        return "n/a"
    try:
        dt = datetime.fromtimestamp(float(ms_value) / 1000.0, tz=timezone.utc)
        return dt.strftime("%Y-%m-%d %H:%M:%S %Z")
    except Exception:
        return "n/a"


def format_age(ms_value: Any) -> str:
    if ms_value is None:
        return "unknown"
    try:
        created = datetime.fromtimestamp(float(ms_value) / 1000.0, tz=timezone.utc)
    except Exception:
        return "unknown"

    delta = datetime.now(timezone.utc) - created
    total_seconds = int(delta.total_seconds())
    days, remainder = divmod(total_seconds, 86400)
    hours, remainder = divmod(remainder, 3600)
    minutes, _ = divmod(remainder, 60)

    parts = []
    if days:
        parts.append(f"{days}d")
    if hours or days:
        parts.append(f"{hours}h")
    parts.append(f"{minutes}m")
    return " ".join(parts)


def format_pct(value: Any) -> str:
    try:
        return f"{float(value) * 100:.1f}%"
    except Exception:
        return "n/a"


def pretty_print_nodes(
    nodes: List[Dict[str, Any]],
    anchor: Optional[float],
    symbol: Optional[str],
    total_open: int,
) -> None:
    shown = len(nodes)
    anchor_display = f"{anchor:.2f}" if anchor is not None else "n/a"
    header_extra = "" if total_open == shown else f", showing {shown}"
    print(
        f"OPEN NODES ({total_open} total{header_extra}) - symbol: {symbol or 'n/a'}, anchor: {anchor_display}\n"
    )

    if not nodes:
        print("No open nodes found.")
        return

    for idx, node in enumerate(nodes, 1):
        created_at_ms = node.get("created_at")
        print(f"[{idx}] {node.get('side', 'n/a')}")
        print(f"    created_at: {format_timestamp(created_at_ms)}  ({created_at_ms})")
        print(f"    distance_pct: {format_pct(node.get('distance_pct'))}")
        print(f"    created_from_anchor: {node.get('created_from_anchor', 'n/a')}")
        print(f"    created_from_extreme: {node.get('created_from_extreme', 'n/a')}")
        print(f"    open_for: {format_age(created_at_ms)}")

        if anchor is not None:
            projected = node.get("projected_price_now")
            projected_display = f"{projected:.2f}" if projected is not None else "n/a"
            print(f"    current_anchor: {anchor_display}")
            print(f"    projected_price_now: {projected_display}")

        print()


def main() -> None:
    args = parse_args()

    nodes_path = args.nodes
    if not nodes_path.is_file():
        print(f"[error] Nodes file not found: {nodes_path}", file=sys.stderr)
        sys.exit(1)

    open_nodes, node_warnings = load_open_nodes(nodes_path, args.symbol)
    anchor = args.anchor
    anchor_warnings: List[str] = []
    if anchor is None:
        phases_path = args.phases if args.phases else None
        anchor, anchor_warnings = load_latest_anchor(phases_path, args.symbol)

    if args.side:
        open_nodes = [n for n in open_nodes if n.get("side") == args.side]

    def sort_key(node: Dict[str, Any]) -> float:
        raw_created = node.get("created_at", 0)
        if raw_created is None:
            return 0.0
        try:
            return float(raw_created or 0)
        except (TypeError, ValueError):
            return 0.0

    open_nodes.sort(key=sort_key, reverse=True)
    total_open = len(open_nodes)

    limit = args.limit if args.limit and args.limit > 0 else None
    if limit:
        open_nodes = open_nodes[:limit]

    project_nodes(open_nodes, anchor)

    if args.raw:
        json.dump(open_nodes, sys.stdout, indent=2)
        sys.stdout.write("\n")
    else:
        pretty_print_nodes(open_nodes, anchor, args.symbol, total_open)

    for message in [*node_warnings, *anchor_warnings]:
        warn(message)


if __name__ == "__main__":
    main()
