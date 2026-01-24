"""
Step 4k: Entry Confirmations - What actually helps?
===================================================
Key findings from previous analysis:
- bars_aligned: 11.1% WR vs 6.6% baseline
- momentum_aligned: 10.8% WR
- volume_spike > 3.3: 11.1% WR
- swing_range_pct > 0.03: 11.4% WR
- tick_velocity LOW (0-2): 10.5% WR
- intraday_range 0.4-0.7: ~10% WR

Let's combine these and check if they stack.
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def eval_filter(df, mask, name):
    """Evaluate a filter."""
    sub = df[mask]
    if len(sub) < 20:
        return None

    wr = sub["anchor_hit"].mean()
    ev = sub["outcome_r"].mean()

    # Check by direction
    long_sub = sub[sub["direction"] == "long"]
    short_sub = sub[sub["direction"] == "short"]

    long_wr = long_sub["anchor_hit"].mean() if len(long_sub) > 10 else np.nan
    short_wr = short_sub["anchor_hit"].mean() if len(short_sub) > 10 else np.nan
    long_ev = long_sub["outcome_r"].mean() if len(long_sub) > 10 else np.nan
    short_ev = short_sub["outcome_r"].mean() if len(short_sub) > 10 else np.nan

    return {
        "name": name,
        "n": len(sub),
        "wr": wr,
        "ev": ev,
        "long_n": len(long_sub),
        "long_wr": long_wr,
        "long_ev": long_ev,
        "short_n": len(short_sub),
        "short_wr": short_wr,
        "short_ev": short_ev,
    }


def main():
    print("=" * 70)
    print("ENTRY CONFIRMATION ANALYSIS")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")

    all_data = pd.concat([train, val, test])

    baseline_wr = all_data["anchor_hit"].mean()
    baseline_ev = all_data["outcome_r"].mean()
    print(
        f"\nBaseline: n={len(all_data)}, WR={baseline_wr * 100:.1f}%, EV={baseline_ev:.3f}R"
    )

    # Define confirmation signals
    confirmations = {
        # Bar alignment
        "bars_aligned": lambda df: df["bars_aligned"] == True,
        "m5_aligned": lambda df: df["m5_direction_aligned"] == True,
        "m1_aligned": lambda df: df["m1_direction_aligned"] == True,
        # Momentum
        "momentum_aligned": lambda df: df["momentum_aligned"] == True,
        "momentum_1m": lambda df: df["momentum_1m_aligned"] == True,
        # Volume
        "vol_spike_hi": lambda df: df["volume_spike_ratio"] > 3.0,
        "vol_spike_med": lambda df: df["volume_spike_ratio"] > 2.0,
        "delta_supports": lambda df: df["delta_supports_trade"] == True,
        # Price action
        "swing_big": lambda df: (
            df["swing_high_after_exit"] - df["swing_low_after_exit"]
        )
        / df["cluster_mid"]
        > 0.0003,
        "tick_vel_low": lambda df: df["tick_velocity"] < 5,
        # Session position
        "mid_session": lambda df: (df["intraday_range_position"] > 0.3)
        & (df["intraday_range_position"] < 0.7),
        # HTF
        "htf_aligned": lambda df: df["htf_trend_aligned"] == True,
    }

    print("\n" + "=" * 60)
    print("SINGLE CONFIRMATIONS")
    print("=" * 60)

    results = []
    for name, mask_fn in confirmations.items():
        res = eval_filter(all_data, mask_fn(all_data), name)
        if res:
            results.append(res)

    print(f"\n{'Confirmation':<20} | {'All':^25} | {'Long':^20} | {'Short':^20}")
    print(
        f"{'':20} | {'n':>6} {'WR':>7} {'EV':>9} | {'n':>5} {'WR':>6} {'EV':>7} | {'n':>5} {'WR':>6} {'EV':>7}"
    )
    print("-" * 100)

    for r in sorted(results, key=lambda x: -x["wr"]):
        print(
            f"{r['name']:<20} | {r['n']:>6} {r['wr'] * 100:>6.1f}% {r['ev']:>+8.3f}R | "
            f"{r['long_n']:>5} {r['long_wr'] * 100:>5.1f}% {r['long_ev']:>+6.3f}R | "
            f"{r['short_n']:>5} {r['short_wr'] * 100:>5.1f}% {r['short_ev']:>+6.3f}R"
        )

    # Test combinations
    print("\n" + "=" * 60)
    print("COMBINED CONFIRMATIONS (stacking)")
    print("=" * 60)

    combos = [
        (
            "bars + vol_spike",
            lambda df: (df["bars_aligned"] == True) & (df["volume_spike_ratio"] > 2),
        ),
        (
            "bars + momentum",
            lambda df: (df["bars_aligned"] == True) & (df["momentum_aligned"] == True),
        ),
        (
            "bars + delta",
            lambda df: (df["bars_aligned"] == True)
            & (df["delta_supports_trade"] == True),
        ),
        (
            "momentum + vol_spike",
            lambda df: (df["momentum_aligned"] == True)
            & (df["volume_spike_ratio"] > 2),
        ),
        (
            "momentum + delta",
            lambda df: (df["momentum_aligned"] == True)
            & (df["delta_supports_trade"] == True),
        ),
        (
            "m5 + vol_spike",
            lambda df: (df["m5_direction_aligned"] == True)
            & (df["volume_spike_ratio"] > 2),
        ),
        (
            "m5 + delta",
            lambda df: (df["m5_direction_aligned"] == True)
            & (df["delta_supports_trade"] == True),
        ),
        (
            "htf + momentum",
            lambda df: (df["htf_trend_aligned"] == True)
            & (df["momentum_aligned"] == True),
        ),
        (
            "htf + delta",
            lambda df: (df["htf_trend_aligned"] == True)
            & (df["delta_supports_trade"] == True),
        ),
        (
            "htf + vol_spike",
            lambda df: (df["htf_trend_aligned"] == True)
            & (df["volume_spike_ratio"] > 2),
        ),
        (
            "htf + m5",
            lambda df: (df["htf_trend_aligned"] == True)
            & (df["m5_direction_aligned"] == True),
        ),
        (
            "bars + vol + mid",
            lambda df: (df["bars_aligned"] == True)
            & (df["volume_spike_ratio"] > 2)
            & (df["intraday_range_position"] > 0.3)
            & (df["intraday_range_position"] < 0.7),
        ),
        (
            "m5 + delta + htf",
            lambda df: (df["m5_direction_aligned"] == True)
            & (df["delta_supports_trade"] == True)
            & (df["htf_trend_aligned"] == True),
        ),
    ]

    combo_results = []
    for name, mask_fn in combos:
        res = eval_filter(all_data, mask_fn(all_data), name)
        if res:
            combo_results.append(res)

    print(f"\n{'Combination':<25} | {'All':^25} | {'Long':^20} | {'Short':^20}")
    print("-" * 100)

    for r in sorted(combo_results, key=lambda x: -x["wr"]):
        long_str = (
            f"{r['long_n']:>5} {r['long_wr'] * 100:>5.1f}% {r['long_ev']:>+6.3f}R"
            if not np.isnan(r["long_wr"])
            else "n/a"
        )
        short_str = (
            f"{r['short_n']:>5} {r['short_wr'] * 100:>5.1f}% {r['short_ev']:>+6.3f}R"
            if not np.isnan(r["short_wr"])
            else "n/a"
        )
        print(
            f"{r['name']:<25} | {r['n']:>6} {r['wr'] * 100:>6.1f}% {r['ev']:>+8.3f}R | {long_str} | {short_str}"
        )

    # Check on splits
    print("\n" + "=" * 60)
    print("BEST COMBINATIONS - VALIDATION ACROSS SPLITS")
    print("=" * 60)

    best_combos = [
        ("bars_aligned", lambda df: df["bars_aligned"] == True),
        (
            "m5 + vol_spike",
            lambda df: (df["m5_direction_aligned"] == True)
            & (df["volume_spike_ratio"] > 2),
        ),
        (
            "m5 + delta",
            lambda df: (df["m5_direction_aligned"] == True)
            & (df["delta_supports_trade"] == True),
        ),
        (
            "htf + momentum",
            lambda df: (df["htf_trend_aligned"] == True)
            & (df["momentum_aligned"] == True),
        ),
        (
            "momentum + vol_spike",
            lambda df: (df["momentum_aligned"] == True)
            & (df["volume_spike_ratio"] > 2),
        ),
    ]

    for name, mask_fn in best_combos:
        print(f"\n--- {name} ---")
        print(f"{'Split':<8} | {'All':^22} | {'Long':^18} | {'Short':^18}")

        for df, split in [(train, "Train"), (val, "Val"), (test, "Test")]:
            mask = mask_fn(df)
            sub = df[mask]

            if len(sub) < 10:
                print(f"{split:<8} | n={len(sub)} (too few)")
                continue

            wr = sub["anchor_hit"].mean()
            ev = sub["outcome_r"].mean()

            long_sub = sub[sub["direction"] == "long"]
            short_sub = sub[sub["direction"] == "short"]

            long_str = (
                f"{len(long_sub):>4} {long_sub['anchor_hit'].mean() * 100:>5.1f}%"
                if len(long_sub) > 5
                else "n/a"
            )
            short_str = (
                f"{len(short_sub):>4} {short_sub['anchor_hit'].mean() * 100:>5.1f}%"
                if len(short_sub) > 5
                else "n/a"
            )

            print(
                f"{split:<8} | {len(sub):>5} {wr * 100:>5.1f}% {ev:>+7.3f}R | {long_str:>18} | {short_str:>18}"
            )


if __name__ == "__main__":
    main()
