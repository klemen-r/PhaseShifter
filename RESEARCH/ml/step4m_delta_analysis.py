"""
Step 4m: Delta / Order Flow Analysis
====================================
Analyzing existing delta features and what we can derive.

What we have:
- cumulative_delta_1m: Net delta over last minute
- cumulative_delta_5m: Net delta over 5 minutes
- delta_supports_trade: Delta agrees with trade direction
- delta_divergence: Delta disagrees with price

What we need for DOM/footprint style entries:
- Delta FLIP (sign change)
- Absorption (high vol, no price move)
- Imbalance
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def main():
    print("=" * 70)
    print("DELTA / ORDER FLOW ANALYSIS")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")
    all_data = pd.concat([train, val, test])

    print(f"\nTotal trades: {len(all_data)}")
    print(f"Baseline WR: {all_data['anchor_hit'].mean() * 100:.1f}%")

    # ==========================================
    # 1. DELTA DIRECTION ANALYSIS
    # ==========================================
    print("\n" + "=" * 60)
    print("1. DELTA DIRECTION AT ENTRY")
    print("=" * 60)

    # Delta supports trade (delta aligns with direction)
    print("\ndelta_supports_trade:")
    for val_flag in [True, False]:
        sub = all_data[all_data["delta_supports_trade"] == val_flag]
        print(
            f"  {val_flag}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
            f"EV={sub['outcome_r'].mean():+.3f}R"
        )
        # By direction
        for d in ["long", "short"]:
            dsub = sub[sub["direction"] == d]
            if len(dsub) > 100:
                print(
                    f"    {d}: n={len(dsub)}, WR={dsub['anchor_hit'].mean() * 100:.1f}%"
                )

    # Delta divergence (delta disagrees with price)
    print("\ndelta_divergence:")
    for val_flag in [True, False]:
        sub = all_data[all_data["delta_divergence"] == val_flag]
        print(
            f"  {val_flag}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
            f"EV={sub['outcome_r'].mean():+.3f}R"
        )

    # ==========================================
    # 2. DELTA MAGNITUDE ANALYSIS
    # ==========================================
    print("\n" + "=" * 60)
    print("2. DELTA MAGNITUDE (cumulative_delta_5m)")
    print("=" * 60)

    # Analyze by quintiles
    all_data["delta_5m_abs"] = all_data["cumulative_delta_5m"].abs()
    quintiles = pd.qcut(
        all_data["delta_5m_abs"],
        5,
        labels=["Q1 (low)", "Q2", "Q3", "Q4", "Q5 (high)"],
        duplicates="drop",
    )
    all_data["delta_quintile"] = quintiles

    print("\nBy delta magnitude (absolute value):")
    for q in ["Q1 (low)", "Q2", "Q3", "Q4", "Q5 (high)"]:
        sub = all_data[all_data["delta_quintile"] == q]
        if len(sub) > 100:
            print(
                f"  {q}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():+.3f}R"
            )

    # ==========================================
    # 3. DERIVE DELTA FLIP PROXY
    # ==========================================
    print("\n" + "=" * 60)
    print("3. DELTA FLIP PROXY")
    print("=" * 60)

    # We can't see the actual flip, but we can look at:
    # - Small delta magnitude = near zero = potential flip area
    # - Delta divergence = price moved but delta didn't follow

    # Near-zero delta (potential flip zone)
    all_data["delta_near_zero"] = all_data["cumulative_delta_5m"].abs() < 100

    print("\nNear-zero delta (|delta_5m| < 100) - potential flip zone:")
    for val_flag in [True, False]:
        sub = all_data[all_data["delta_near_zero"] == val_flag]
        print(
            f"  near_zero={val_flag}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
            f"EV={sub['outcome_r'].mean():+.3f}R"
        )

    # Check if delta_supports_trade + near_zero (fresh flip)
    print("\nDelta supports + near zero (fresh delta flip in our direction):")
    mask = (all_data["delta_supports_trade"] == True) & (
        all_data["delta_near_zero"] == True
    )
    sub = all_data[mask]
    print(
        f"  n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, EV={sub['outcome_r'].mean():+.3f}R"
    )

    # ==========================================
    # 4. DERIVE ABSORPTION PROXY
    # ==========================================
    print("\n" + "=" * 60)
    print("4. ABSORPTION PROXY")
    print("=" * 60)

    # Absorption = high volume but price didn't move much
    # We have volume_spike_ratio and we can look at ret_1m

    # High volume + small price move = absorption
    all_data["high_vol"] = all_data["volume_spike_ratio"] > 2
    all_data["small_move"] = all_data["ret_1m"].abs() < 0.0002  # Less than 0.02% move
    all_data["absorption_proxy"] = all_data["high_vol"] & all_data["small_move"]

    print("\nHigh volume + small price move (absorption proxy):")
    for val_flag in [True, False]:
        sub = all_data[all_data["absorption_proxy"] == val_flag]
        print(
            f"  absorption={val_flag}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
            f"EV={sub['outcome_r'].mean():+.3f}R"
        )

    # ==========================================
    # 5. COMBINED ORDER FLOW SIGNALS
    # ==========================================
    print("\n" + "=" * 60)
    print("5. COMBINED ORDER FLOW SIGNALS")
    print("=" * 60)

    signals = [
        ("delta_supports", lambda df: df["delta_supports_trade"] == True),
        ("delta_divergence", lambda df: df["delta_divergence"] == True),
        ("delta_near_zero", lambda df: df["delta_near_zero"] == True),
        ("absorption_proxy", lambda df: df["absorption_proxy"] == True),
        (
            "high_delta_supports",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["delta_5m_abs"] > 500),
        ),
        (
            "delta_sup + vol_spike",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["volume_spike_ratio"] > 2),
        ),
        (
            "delta_sup + absorption",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["absorption_proxy"] == True),
        ),
        (
            "delta_div + high_vol",
            lambda df: (df["delta_divergence"] == True)
            & (df["volume_spike_ratio"] > 2),
        ),
    ]

    print(f"\n{'Signal':<25} | {'All':^20} | {'Long':^15} | {'Short':^15}")
    print("-" * 80)

    for name, mask_fn in signals:
        mask = mask_fn(all_data)
        sub = all_data[mask]
        if len(sub) >= 50:
            wr = sub["anchor_hit"].mean()
            ev = sub["outcome_r"].mean()

            long_sub = sub[sub["direction"] == "long"]
            short_sub = sub[sub["direction"] == "short"]
            long_wr = long_sub["anchor_hit"].mean() if len(long_sub) > 20 else np.nan
            short_wr = short_sub["anchor_hit"].mean() if len(short_sub) > 20 else np.nan

            print(
                f"{name:<25} | {len(sub):>5} {wr * 100:>5.1f}% {ev:>+6.3f}R | "
                f"{long_wr * 100 if not np.isnan(long_wr) else 0:>5.1f}% | "
                f"{short_wr * 100 if not np.isnan(short_wr) else 0:>5.1f}%"
            )

    # ==========================================
    # 6. BEST COMBINATIONS WITH OTHER SIGNALS
    # ==========================================
    print("\n" + "=" * 60)
    print("6. DELTA + OTHER ENTRY SIGNALS")
    print("=" * 60)

    combos = [
        (
            "delta_sup + bars_aligned",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["bars_aligned"] == True),
        ),
        (
            "delta_sup + m5_aligned",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["m5_direction_aligned"] == True),
        ),
        (
            "delta_sup + momentum",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["momentum_aligned"] == True),
        ),
        (
            "delta_sup + htf",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["htf_trend_aligned"] == True),
        ),
        (
            "delta_sup + vol + bars",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["volume_spike_ratio"] > 2)
            & (df["bars_aligned"] == True),
        ),
        (
            "high_delta + bars",
            lambda df: (df["delta_supports_trade"] == True)
            & (df["delta_5m_abs"] > 500)
            & (df["bars_aligned"] == True),
        ),
    ]

    for name, mask_fn in combos:
        print(f"\n--- {name} ---")
        for df, split in [(train, "Train"), (val, "Val"), (test, "Test")]:
            # Need to recalculate derived columns for each split
            df = df.copy()
            df["delta_5m_abs"] = df["cumulative_delta_5m"].abs()

            mask = mask_fn(df)
            sub = df[mask]
            if len(sub) >= 10:
                wr = sub["anchor_hit"].mean()
                ev = sub["outcome_r"].mean()
                pnl = sub["net_pnl"].sum()
                print(
                    f"  {split}: n={len(sub)}, WR={wr * 100:.1f}%, EV={ev:+.3f}R, PnL=${pnl:,.0f}"
                )
            else:
                print(f"  {split}: n={len(sub)} (too few)")


if __name__ == "__main__":
    main()
