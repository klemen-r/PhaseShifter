"""
Step 4d: Filter Analysis - Find entry filters that produce positive expectancy
==============================================================================
The backtest collected ALL zone interactions. Now we find which filters
select profitable subsets.

Key insight: We need filters that BOTH:
1. Increase win rate
2. Maintain favorable RR (or accept lower RR with much higher win rate)

Breakeven math (with ~0.4R costs):
- RR=0.5, need ~80% win rate to break even
- RR=1.0, need ~60% win rate
- RR=2.0, need ~45% win rate
- RR=3.0, need ~40% win rate
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def calculate_expectancy(df, costs_r=0.4):
    """Calculate expected R per trade."""
    if len(df) == 0:
        return np.nan

    # For each trade: if win, get rr_ratio - costs; if lose, get -1 - costs
    # But outcome_r already has costs baked in
    return df["outcome_r"].mean()


def calculate_gross_expectancy(df):
    """Calculate expected R before costs."""
    if len(df) == 0:
        return np.nan

    wins = df[df["anchor_hit"] == True]
    losses = df[df["anchor_hit"] == False]

    if len(wins) > 0:
        avg_win_r = wins["rr_ratio"].mean()  # Win gets RR
    else:
        avg_win_r = 0

    wr = df["anchor_hit"].mean()

    # Gross EV = (win_rate * avg_win_R) - (loss_rate * 1R)
    return (wr * avg_win_r) - ((1 - wr) * 1.0)


def main():
    print("=" * 70)
    print("FILTER ANALYSIS: Finding Positive Expectancy Subsets")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")

    print(f"\nBaseline (ALL trades):")
    for df, name in [(train, "Train"), (val, "Val")]:
        wr = df["anchor_hit"].mean()
        avg_rr = df["rr_ratio"].mean()
        gross_ev = calculate_gross_expectancy(df)
        net_ev = df["outcome_r"].mean()
        print(
            f"  {name}: n={len(df):>5}, WR={wr * 100:>5.1f}%, avgRR={avg_rr:>5.1f}, "
            f"grossEV={gross_ev:>6.3f}R, netEV={net_ev:>6.3f}R"
        )

    # ========================================
    # Strategy 1: RR-based filtering
    # Higher RR = harder to hit but better payout
    # ========================================
    print("\n" + "=" * 70)
    print("STRATEGY 1: RR-based filtering (need higher WR for lower RR)")
    print("=" * 70)

    print(f"\n{'Filter':<25} | {'Train':^40} | {'Val':^40}")
    print(
        f"{'':25} | {'n':>6} {'WR%':>6} {'avgRR':>6} {'grossEV':>8} {'netEV':>8} | "
        f"{'n':>6} {'WR%':>6} {'avgRR':>6} {'grossEV':>8} {'netEV':>8}"
    )
    print("-" * 115)

    rr_filters = [
        ("RR >= 3", lambda df: df["rr_ratio"] >= 3),
        ("RR >= 5", lambda df: df["rr_ratio"] >= 5),
        ("RR >= 7", lambda df: df["rr_ratio"] >= 7),
        ("RR >= 10", lambda df: df["rr_ratio"] >= 10),
        ("RR >= 15", lambda df: df["rr_ratio"] >= 15),
        ("RR >= 20", lambda df: df["rr_ratio"] >= 20),
    ]

    for name, filt in rr_filters:
        row = f"{name:<25} |"
        for df in [train, val]:
            sub = df[filt(df)]
            if len(sub) >= 10:
                wr = sub["anchor_hit"].mean()
                avg_rr = sub["rr_ratio"].mean()
                gross_ev = calculate_gross_expectancy(sub)
                net_ev = sub["outcome_r"].mean()
                row += f" {len(sub):>6} {wr * 100:>5.1f}% {avg_rr:>6.1f} {gross_ev:>8.3f} {net_ev:>8.3f} |"
            else:
                row += f" {len(sub):>6} {'n/a':>5} {'':>6} {'':>8} {'':>8} |"
        print(row)

    # ========================================
    # Strategy 2: Alignment-based filtering
    # ========================================
    print("\n" + "=" * 70)
    print("STRATEGY 2: Alignment filters (momentum, HTF, bars)")
    print("=" * 70)

    alignment_filters = [
        ("momentum_aligned", lambda df: df["momentum_aligned"] == True),
        ("bars_aligned", lambda df: df["bars_aligned"] == True),
        ("htf_trend_aligned", lambda df: df["htf_trend_aligned"] == True),
        ("delta_supports_trade", lambda df: df["delta_supports_trade"] == True),
        ("alignment_score >= 5", lambda df: df["alignment_score"] >= 5),
        ("alignment_score >= 6", lambda df: df["alignment_score"] >= 6),
        ("red_flag_count <= 2", lambda df: df["red_flag_count"] <= 2),
    ]

    print(f"\n{'Filter':<25} | {'Train':^40} | {'Val':^40}")
    print("-" * 115)

    for name, filt in alignment_filters:
        row = f"{name:<25} |"
        for df in [train, val]:
            sub = df[filt(df)]
            if len(sub) >= 10:
                wr = sub["anchor_hit"].mean()
                avg_rr = sub["rr_ratio"].mean()
                gross_ev = calculate_gross_expectancy(sub)
                net_ev = sub["outcome_r"].mean()
                row += f" {len(sub):>6} {wr * 100:>5.1f}% {avg_rr:>6.1f} {gross_ev:>8.3f} {net_ev:>8.3f} |"
            else:
                row += f" {len(sub):>6} {'n/a':>5} {'':>6} {'':>8} {'':>8} |"
        print(row)

    # ========================================
    # Strategy 3: Combined filters
    # ========================================
    print("\n" + "=" * 70)
    print("STRATEGY 3: Combined filters (RR + Alignment)")
    print("=" * 70)

    combined_filters = [
        (
            "RR>=5 + momentum",
            lambda df: (df["rr_ratio"] >= 5) & (df["momentum_aligned"] == True),
        ),
        (
            "RR>=5 + bars",
            lambda df: (df["rr_ratio"] >= 5) & (df["bars_aligned"] == True),
        ),
        (
            "RR>=5 + htf",
            lambda df: (df["rr_ratio"] >= 5) & (df["htf_trend_aligned"] == True),
        ),
        (
            "RR>=5 + align>=5",
            lambda df: (df["rr_ratio"] >= 5) & (df["alignment_score"] >= 5),
        ),
        (
            "RR>=7 + momentum",
            lambda df: (df["rr_ratio"] >= 7) & (df["momentum_aligned"] == True),
        ),
        (
            "RR>=7 + bars",
            lambda df: (df["rr_ratio"] >= 7) & (df["bars_aligned"] == True),
        ),
        (
            "RR>=7 + htf",
            lambda df: (df["rr_ratio"] >= 7) & (df["htf_trend_aligned"] == True),
        ),
        (
            "RR>=10 + htf",
            lambda df: (df["rr_ratio"] >= 10) & (df["htf_trend_aligned"] == True),
        ),
        (
            "RR>=10 + momentum+htf",
            lambda df: (df["rr_ratio"] >= 10)
            & (df["momentum_aligned"] == True)
            & (df["htf_trend_aligned"] == True),
        ),
        (
            "RR>=15 + htf",
            lambda df: (df["rr_ratio"] >= 15) & (df["htf_trend_aligned"] == True),
        ),
    ]

    print(f"\n{'Filter':<25} | {'Train':^40} | {'Val':^40}")
    print("-" * 115)

    for name, filt in combined_filters:
        row = f"{name:<25} |"
        for df in [train, val]:
            sub = df[filt(df)]
            if len(sub) >= 10:
                wr = sub["anchor_hit"].mean()
                avg_rr = sub["rr_ratio"].mean()
                gross_ev = calculate_gross_expectancy(sub)
                net_ev = sub["outcome_r"].mean()
                row += f" {len(sub):>6} {wr * 100:>5.1f}% {avg_rr:>6.1f} {gross_ev:>8.3f} {net_ev:>8.3f} |"
            else:
                row += f" {len(sub):>6} {'n/a':>5} {'':>6} {'':>8} {'':>8} |"
        print(row)

    # ========================================
    # Strategy 4: Volume/Volatility filters
    # ========================================
    print("\n" + "=" * 70)
    print("STRATEGY 4: Volume/Volatility filters")
    print("=" * 70)

    vol_filters = [
        ("high_vol_env", lambda df: df["high_vol_environment"] == True),
        ("vol_spike > 1.5", lambda df: df["volume_spike_ratio"] > 1.5),
        ("vol_spike > 2.0", lambda df: df["volume_spike_ratio"] > 2.0),
        ("vol_expanding", lambda df: df["vol_expanding"] == True),
        (
            "RR>=10 + vol_spike>1.5",
            lambda df: (df["rr_ratio"] >= 10) & (df["volume_spike_ratio"] > 1.5),
        ),
        (
            "RR>=10 + high_vol",
            lambda df: (df["rr_ratio"] >= 10) & (df["high_vol_environment"] == True),
        ),
    ]

    print(f"\n{'Filter':<25} | {'Train':^40} | {'Val':^40}")
    print("-" * 115)

    for name, filt in vol_filters:
        row = f"{name:<25} |"
        for df in [train, val]:
            sub = df[filt(df)]
            if len(sub) >= 10:
                wr = sub["anchor_hit"].mean()
                avg_rr = sub["rr_ratio"].mean()
                gross_ev = calculate_gross_expectancy(sub)
                net_ev = sub["outcome_r"].mean()
                row += f" {len(sub):>6} {wr * 100:>5.1f}% {avg_rr:>6.1f} {gross_ev:>8.3f} {net_ev:>8.3f} |"
            else:
                row += f" {len(sub):>6} {'n/a':>5} {'':>6} {'':>8} {'':>8} |"
        print(row)

    # ========================================
    # Find any positive expectancy filters
    # ========================================
    print("\n" + "=" * 70)
    print("POSITIVE EXPECTANCY SEARCH")
    print("=" * 70)

    all_filters = rr_filters + alignment_filters + combined_filters + vol_filters

    positive_train = []
    positive_both = []

    for name, filt in all_filters:
        train_sub = train[filt(train)]
        val_sub = val[filt(val)]

        if len(train_sub) >= 20:
            train_ev = train_sub["outcome_r"].mean()
            if train_ev > 0:
                positive_train.append((name, len(train_sub), train_ev))

                if len(val_sub) >= 10:
                    val_ev = val_sub["outcome_r"].mean()
                    if val_ev > 0:
                        positive_both.append(
                            (name, len(train_sub), train_ev, len(val_sub), val_ev)
                        )

    print("\nFilters with POSITIVE net EV on TRAIN:")
    if positive_train:
        for name, n, ev in sorted(positive_train, key=lambda x: -x[2]):
            print(f"  {name:<30} n={n:>5}, EV={ev:>6.3f}R")
    else:
        print("  None found")

    print("\nFilters with POSITIVE net EV on BOTH train AND val:")
    if positive_both:
        for name, nt, evt, nv, evv in sorted(positive_both, key=lambda x: -x[2]):
            print(
                f"  {name:<30} train: n={nt:>5}, EV={evt:>6.3f}R | val: n={nv:>5}, EV={evv:>6.3f}R"
            )
    else:
        print("  None found - need to search more combinations")


if __name__ == "__main__":
    main()
