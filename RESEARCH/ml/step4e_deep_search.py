"""
Step 4e: Deep search for profitable filters
============================================
"""

from itertools import combinations
from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def eval_filter(train, val, mask_fn, min_trades=30):
    """Evaluate a filter on train and val."""
    train_mask = mask_fn(train)
    val_mask = mask_fn(val)

    train_sub = train[train_mask]
    val_sub = val[val_mask]

    if len(train_sub) < min_trades or len(val_sub) < 10:
        return None

    return {
        "train_n": len(train_sub),
        "train_wr": train_sub["anchor_hit"].mean(),
        "train_ev": train_sub["outcome_r"].mean(),
        "val_n": len(val_sub),
        "val_wr": val_sub["anchor_hit"].mean(),
        "val_ev": val_sub["outcome_r"].mean(),
    }


def main():
    print("=" * 70)
    print("DEEP SEARCH: Finding Profitable Entry Filters")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")

    # Let's examine delta_supports_trade more closely
    print("\n=== Investigating delta_supports_trade ===")
    for df, name in [(train, "Train"), (val, "Val")]:
        mask = df["delta_supports_trade"] == True
        sub = df[mask]
        print(f"\n{name} (delta_supports_trade=True):")
        print(f"  n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%")
        print(f"  Mean RR: {sub['rr_ratio'].mean():.1f}")
        print(f"  Net EV: {sub['outcome_r'].mean():.3f}R")

        # Check by direction
        for d in ["long", "short"]:
            dsub = sub[sub["direction"] == d]
            if len(dsub) > 10:
                print(
                    f"    {d}: n={len(dsub)}, WR={dsub['anchor_hit'].mean() * 100:.1f}%, EV={dsub['outcome_r'].mean():.3f}R"
                )

    # Define atomic filters
    atomic_filters = {
        "delta_supports": lambda df: df["delta_supports_trade"] == True,
        "momentum_aligned": lambda df: df["momentum_aligned"] == True,
        "bars_aligned": lambda df: df["bars_aligned"] == True,
        "htf_aligned": lambda df: df["htf_trend_aligned"] == True,
        "m5_aligned": lambda df: df["m5_direction_aligned"] == True,
        "high_vol": lambda df: df["high_vol_environment"] == True,
        "vol_spike_hi": lambda df: df["volume_spike_ratio"] > 2.0,
        "vol_expanding": lambda df: df["vol_expanding"] == True,
        "is_rth": lambda df: df["is_rth"] == True,
        "is_eth": lambda df: df["is_rth"] == False,
        "align_5plus": lambda df: df["alignment_score"] >= 5,
        "flags_low": lambda df: df["red_flag_count"] <= 2,
        "rr_10plus": lambda df: df["rr_ratio"] >= 10,
        "rr_15plus": lambda df: df["rr_ratio"] >= 15,
        "rr_20plus": lambda df: df["rr_ratio"] >= 20,
        "cluster_4plus": lambda df: df["cluster_count"] >= 4,
        "session_mid": lambda df: df["session_mid_zone"] == True,
        "not_first_hr": lambda df: df["is_first_hour"] == False,
        "not_last_hr": lambda df: df["is_last_hour"] == False,
    }

    # Try all pairs
    print("\n" + "=" * 70)
    print("TESTING ALL PAIRS OF FILTERS")
    print("=" * 70)

    results = []

    filter_names = list(atomic_filters.keys())
    for f1, f2 in combinations(filter_names, 2):
        mask_fn = lambda df, a=f1, b=f2: atomic_filters[a](df) & atomic_filters[b](df)
        res = eval_filter(train, val, mask_fn, min_trades=30)
        if res:
            results.append({"filter": f"{f1} + {f2}", **res})

    # Also try triples with delta_supports (since it looked promising)
    print("\n=== Testing triples with delta_supports ===")
    for f1, f2 in combinations([f for f in filter_names if f != "delta_supports"], 2):
        mask_fn = lambda df, a=f1, b=f2: (
            atomic_filters["delta_supports"](df)
            & atomic_filters[a](df)
            & atomic_filters[b](df)
        )
        res = eval_filter(train, val, mask_fn, min_trades=20)
        if res:
            results.append({"filter": f"delta + {f1} + {f2}", **res})

    # Sort by val EV
    results_df = pd.DataFrame(results)
    if len(results_df) > 0:
        results_df = results_df.sort_values("val_ev", ascending=False)

        print("\nTop 20 filters by VALIDATION EV:")
        print(f"{'Filter':<40} | {'Train':^25} | {'Val':^25}")
        print(
            f"{'':40} | {'n':>6} {'WR%':>6} {'EV':>8} | {'n':>6} {'WR%':>6} {'EV':>8}"
        )
        print("-" * 95)

        for _, row in results_df.head(20).iterrows():
            print(
                f"{row['filter']:<40} | {row['train_n']:>6} {row['train_wr'] * 100:>5.1f}% {row['train_ev']:>8.3f} | "
                f"{row['val_n']:>6} {row['val_wr'] * 100:>5.1f}% {row['val_ev']:>8.3f}"
            )

        # Find filters positive on BOTH
        positive_both = results_df[
            (results_df["train_ev"] > 0) & (results_df["val_ev"] > 0)
        ]

        print(f"\n\nFilters POSITIVE on BOTH train and val: {len(positive_both)}")
        if len(positive_both) > 0:
            for _, row in positive_both.iterrows():
                print(
                    f"  {row['filter']:<40} train: {row['train_ev']:.3f}R, val: {row['val_ev']:.3f}R"
                )

        # Find filters positive on val with reasonable train
        reasonable = results_df[
            (results_df["train_ev"] > -0.3) & (results_df["val_ev"] > 0.1)
        ]
        print(f"\nFilters with val EV > 0.1R and train EV > -0.3R: {len(reasonable)}")
        if len(reasonable) > 0:
            for _, row in reasonable.head(10).iterrows():
                print(
                    f"  {row['filter']:<40} train: n={row['train_n']}, EV={row['train_ev']:.3f}R | "
                    f"val: n={row['val_n']}, EV={row['val_ev']:.3f}R"
                )

    # Check what's special about validation period
    print("\n" + "=" * 70)
    print("TRAIN vs VAL PERIOD COMPARISON")
    print("=" * 70)

    print("\nTime periods:")
    train["dt"] = pd.to_datetime(train["trade_entry_time"], unit="ms")
    val["dt"] = pd.to_datetime(val["trade_entry_time"], unit="ms")
    print(f"  Train: {train['dt'].min()} to {train['dt'].max()}")
    print(f"  Val: {val['dt'].min()} to {val['dt'].max()}")

    print("\nKey stats comparison:")
    for col in [
        "rr_ratio",
        "volume_spike_ratio",
        "vol_percentile_session",
        "alignment_score",
    ]:
        print(f"  {col}:")
        print(f"    Train mean: {train[col].mean():.2f}")
        print(f"    Val mean: {val[col].mean():.2f}")


if __name__ == "__main__":
    main()
