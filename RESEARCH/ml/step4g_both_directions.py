"""
Step 4g: Find filters that work for BOTH longs and shorts
=========================================================
A robust filter should work regardless of direction.
"""

from itertools import combinations
from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def eval_filter_by_direction(train, val, test, mask_fn, name):
    """Evaluate filter separately for longs and shorts."""
    results = []

    for df, split in [(train, "train"), (val, "val"), (test, "test")]:
        mask = mask_fn(df)
        sub = df[mask]

        for direction in ["long", "short"]:
            dsub = sub[sub["direction"] == direction]
            if len(dsub) >= 5:
                results.append(
                    {
                        "split": split,
                        "direction": direction,
                        "n": len(dsub),
                        "wr": dsub["anchor_hit"].mean(),
                        "ev": dsub["outcome_r"].mean(),
                    }
                )

    return pd.DataFrame(results)


def main():
    print("=" * 70)
    print("FINDING FILTERS THAT WORK FOR BOTH DIRECTIONS")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")

    # Atomic filters
    filters = {
        "delta": lambda df: df["delta_supports_trade"] == True,
        "momentum": lambda df: df["momentum_aligned"] == True,
        "bars": lambda df: df["bars_aligned"] == True,
        "htf": lambda df: df["htf_trend_aligned"] == True,
        "m5": lambda df: df["m5_direction_aligned"] == True,
        "high_vol": lambda df: df["high_vol_environment"] == True,
        "vol_spike": lambda df: df["volume_spike_ratio"] > 1.5,
        "vol_expand": lambda df: df["vol_expanding"] == True,
        "session_mid": lambda df: df["session_mid_zone"] == True,
        "align_5": lambda df: df["alignment_score"] >= 5,
        "rr_10": lambda df: df["rr_ratio"] >= 10,
        "rr_15": lambda df: df["rr_ratio"] >= 15,
        "cluster_4": lambda df: df["cluster_count"] >= 4,
        "not_first": lambda df: df["is_first_hour"] == False,
        "eth": lambda df: df["is_rth"] == False,
        "rth": lambda df: df["is_rth"] == True,
    }

    # Test single filters first
    print("\n=== SINGLE FILTERS ===")
    print(
        f"{'Filter':<20} | {'Train L':>12} {'Train S':>12} | {'Val L':>12} {'Val S':>12} | {'Test L':>12} {'Test S':>12}"
    )
    print("-" * 100)

    single_results = []
    for name, filt in filters.items():
        row = f"{name:<20} |"
        long_evs = []
        short_evs = []

        for df, split in [(train, "train"), (val, "val"), (test, "test")]:
            mask = filt(df)
            sub = df[mask]

            for direction in ["long", "short"]:
                dsub = sub[sub["direction"] == direction]
                if len(dsub) >= 10:
                    ev = dsub["outcome_r"].mean()
                    row += f" {len(dsub):>4}:{ev:>+.2f}R"
                    if direction == "long":
                        long_evs.append(ev)
                    else:
                        short_evs.append(ev)
                else:
                    row += f" {'n/a':>10}"

        print(row)

        # Check if positive for both directions across splits
        if len(long_evs) >= 2 and len(short_evs) >= 2:
            avg_long = np.mean(long_evs)
            avg_short = np.mean(short_evs)
            if avg_long > -0.5 and avg_short > -0.5:  # Both reasonable
                single_results.append((name, avg_long, avg_short, avg_long + avg_short))

    print("\nSingle filters with decent performance in both directions:")
    for name, l, s, total in sorted(single_results, key=lambda x: -x[3]):
        print(
            f"  {name:<20} long={l:>+.3f}R, short={s:>+.3f}R, combined={total:>+.3f}R"
        )

    # Test pairs
    print("\n\n=== FILTER PAIRS ===")

    pair_results = []
    filter_names = list(filters.keys())

    for f1, f2 in combinations(filter_names, 2):
        combined = lambda df, a=f1, b=f2: filters[a](df) & filters[b](df)

        long_evs = []
        short_evs = []
        long_ns = []
        short_ns = []

        for df in [train, val, test]:
            mask = combined(df)
            sub = df[mask]

            for direction in ["long", "short"]:
                dsub = sub[sub["direction"] == direction]
                if len(dsub) >= 5:
                    if direction == "long":
                        long_evs.append(dsub["outcome_r"].mean())
                        long_ns.append(len(dsub))
                    else:
                        short_evs.append(dsub["outcome_r"].mean())
                        short_ns.append(len(dsub))

        # Need data in all splits for both directions
        if len(long_evs) >= 2 and len(short_evs) >= 2:
            avg_long = np.mean(long_evs)
            avg_short = np.mean(short_evs)
            total_n = sum(long_ns) + sum(short_ns)

            # Both directions should be at least not terrible
            if avg_long > -0.3 and avg_short > -0.3:
                pair_results.append(
                    {
                        "filter": f"{f1} + {f2}",
                        "long_ev": avg_long,
                        "short_ev": avg_short,
                        "combined_ev": (avg_long + avg_short) / 2,
                        "total_n": total_n,
                        "long_n": sum(long_ns),
                        "short_n": sum(short_ns),
                    }
                )

    pair_df = pd.DataFrame(pair_results)
    if len(pair_df) > 0:
        pair_df = pair_df.sort_values("combined_ev", ascending=False)

        print(f"\nTop 15 pairs by combined EV (both directions > -0.3R):")
        print(
            f"{'Filter':<30} | {'Long EV':>8} {'Long n':>7} | {'Short EV':>8} {'Short n':>7} | {'Combined':>8}"
        )
        print("-" * 85)

        for _, row in pair_df.head(15).iterrows():
            print(
                f"{row['filter']:<30} | {row['long_ev']:>+7.3f}R {row['long_n']:>7} | "
                f"{row['short_ev']:>+7.3f}R {row['short_n']:>7} | {row['combined_ev']:>+7.3f}R"
            )

        # Find filters positive in BOTH
        positive_both = pair_df[(pair_df["long_ev"] > 0) & (pair_df["short_ev"] > 0)]
        print(f"\n\nFilters POSITIVE in BOTH directions: {len(positive_both)}")
        for _, row in positive_both.iterrows():
            print(
                f"  {row['filter']:<30} long={row['long_ev']:>+.3f}R, short={row['short_ev']:>+.3f}R"
            )

    # Deep dive on most promising
    print("\n" + "=" * 70)
    print("DEEP DIVE: Best candidates")
    print("=" * 70)

    candidates = [
        ("momentum", lambda df: df["momentum_aligned"] == True),
        (
            "momentum + vol_spike",
            lambda df: (df["momentum_aligned"] == True)
            & (df["volume_spike_ratio"] > 1.5),
        ),
        (
            "momentum + high_vol",
            lambda df: (df["momentum_aligned"] == True)
            & (df["high_vol_environment"] == True),
        ),
        (
            "bars + vol_spike",
            lambda df: (df["bars_aligned"] == True) & (df["volume_spike_ratio"] > 1.5),
        ),
        (
            "delta + momentum",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["momentum_aligned"] == True),
        ),
        ("align_5", lambda df: df["alignment_score"] >= 5),
        (
            "align_5 + vol_spike",
            lambda df: (df["alignment_score"] >= 5) & (df["volume_spike_ratio"] > 1.5),
        ),
    ]

    for name, filt in candidates:
        print(f"\n--- {name} ---")
        print(
            f"{'Split':<8} | {'Long n':>7} {'Long WR':>8} {'Long EV':>9} | {'Short n':>7} {'Short WR':>8} {'Short EV':>9}"
        )
        print("-" * 75)

        for df, split in [(train, "Train"), (val, "Val"), (test, "Test")]:
            mask = filt(df)
            sub = df[mask]

            row = f"{split:<8} |"
            for direction in ["long", "short"]:
                dsub = sub[sub["direction"] == direction]
                if len(dsub) >= 3:
                    wr = dsub["anchor_hit"].mean()
                    ev = dsub["outcome_r"].mean()
                    row += f" {len(dsub):>7} {wr * 100:>7.1f}% {ev:>+8.3f}R |"
                else:
                    row += f" {len(dsub):>7} {'n/a':>8} {'n/a':>9} |"
            print(row)


if __name__ == "__main__":
    main()
