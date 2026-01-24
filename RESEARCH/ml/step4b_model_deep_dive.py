"""
Step 4b: Deep dive into what the model actually learned
=======================================================
The model found that LOW rr_ratio is better - let's understand why.
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def main():
    print("=" * 70)
    print("DEEP DIVE: Understanding the RR Ratio Pattern")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")

    # The model found rr_ratio <= 0.90 is good
    # rr_ratio = (zone_to_anchor_distance) / risk_amount
    # Low RR means: anchor is CLOSE relative to risk
    # This means: tighter targets, more likely to hit

    print("\n=== RR Ratio Distribution Analysis ===")
    print(f"\nTrain RR ratio stats:")
    print(f"  Mean: {train['rr_ratio'].mean():.2f}")
    print(f"  Median: {train['rr_ratio'].median():.2f}")
    print(f"  Min: {train['rr_ratio'].min():.2f}")
    print(f"  Max: {train['rr_ratio'].max():.2f}")
    print(f"  % with RR < 1: {(train['rr_ratio'] < 1).mean() * 100:.1f}%")

    # The key insight: LOW RR = anchor is close = easier target
    # HIGH RR = anchor is far = harder to reach before stop

    print("\n=== Win Rate by RR Ratio Bins ===")
    bins = [0, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 5.0, 100]
    labels = [
        "0-0.5",
        "0.5-0.75",
        "0.75-1.0",
        "1.0-1.5",
        "1.5-2.0",
        "2.0-3.0",
        "3.0-5.0",
        "5.0+",
    ]

    for df, name in [(train, "Train"), (val, "Val")]:
        print(f"\n{name}:")
        df["rr_bin"] = pd.cut(df["rr_ratio"], bins=bins, labels=labels)
        grouped = df.groupby("rr_bin", observed=True).agg(
            {"anchor_hit": ["mean", "count"], "outcome_r": "mean"}
        )
        grouped.columns = ["win_rate", "count", "mean_r"]
        for idx, row in grouped.iterrows():
            if row["count"] >= 10:
                print(
                    f"  RR {idx:<10}: n={int(row['count']):>5}, "
                    f"win rate={row['win_rate'] * 100:>5.1f}%, mean R={row['mean_r']:>6.2f}"
                )

    print("\n=== The Real Pattern ===")
    print("""
    LOW RR (< 1.0) means:
    - Zone is CLOSE to anchor
    - Target is nearby, easy to reach
    - BUT: risk is relatively large
    - Net effect: higher probability of hitting target

    HIGH RR (> 2.0) means:
    - Zone is FAR from anchor
    - Must travel far to reach target
    - Risk is small relative to reward
    - BUT: price rarely travels that far
    - Net effect: lower probability

    The EXPECTATION might be better with high RR (when it works, it's big),
    but the PROBABILITY is higher with low RR.
    """)

    print("\n=== Checking: Does htf_trend matter at different RR levels? ===")
    for df, name in [(train, "Train"), (val, "Val")]:
        print(f"\n{name}:")
        for rr_threshold in [0.9, 1.5, 3.0]:
            low_rr = df[df["rr_ratio"] <= rr_threshold]
            high_rr = df[df["rr_ratio"] > rr_threshold]

            print(f"\n  RR <= {rr_threshold}:")
            if len(low_rr) > 0:
                htf_yes = (
                    low_rr[low_rr["htf_trend_aligned"] == True]["anchor_hit"].mean()
                    if (low_rr["htf_trend_aligned"] == True).sum() > 10
                    else np.nan
                )
                htf_no = (
                    low_rr[low_rr["htf_trend_aligned"] == False]["anchor_hit"].mean()
                    if (low_rr["htf_trend_aligned"] == False).sum() > 10
                    else np.nan
                )
                print(
                    f"    HTF aligned TRUE:  {htf_yes * 100:.1f}%"
                    if not np.isnan(htf_yes)
                    else "    HTF aligned TRUE:  n/a"
                )
                print(
                    f"    HTF aligned FALSE: {htf_no * 100:.1f}%"
                    if not np.isnan(htf_no)
                    else "    HTF aligned FALSE: n/a"
                )

    print("\n=== Best Combination: Low RR + HTF Aligned ===")
    for df, name in [(train, "Train"), (val, "Val")]:
        mask = (df["rr_ratio"] <= 0.9) & (df["htf_trend_aligned"] == True)
        n = mask.sum()
        if n > 10:
            wr = df.loc[mask, "anchor_hit"].mean()
            mean_r = df.loc[mask, "outcome_r"].mean()
            # Also check mean outcome in dollars
            mean_pnl = df.loc[mask, "net_pnl"].mean()
            print(
                f"{name}: n={n:>4}, win rate={wr * 100:.1f}%, mean R={mean_r:.2f}, mean PnL=${mean_pnl:.2f}"
            )
        else:
            print(f"{name}: n={n} (too few)")

    print("\n=== Extended: Low RR + HTF + momentum_aligned ===")
    for df, name in [(train, "Train"), (val, "Val")]:
        mask = (
            (df["rr_ratio"] <= 0.9)
            & (df["htf_trend_aligned"] == True)
            & (df["momentum_aligned"] == True)
        )
        n = mask.sum()
        if n > 10:
            wr = df.loc[mask, "anchor_hit"].mean()
            mean_r = df.loc[mask, "outcome_r"].mean()
            mean_pnl = df.loc[mask, "net_pnl"].mean()
            print(
                f"{name}: n={n:>4}, win rate={wr * 100:.1f}%, mean R={mean_r:.2f}, mean PnL=${mean_pnl:.2f}"
            )
        else:
            print(f"{name}: n={n} (too few)")

    print("\n=== Trade-off Analysis: Frequency vs Quality ===")
    thresholds = [0.5, 0.75, 0.9, 1.0, 1.25, 1.5]
    print(
        f"\n{'RR <=':>8} | {'Train n':>8} {'WR%':>6} {'MeanR':>8} | {'Val n':>8} {'WR%':>6} {'MeanR':>8}"
    )
    print("-" * 70)

    for thresh in thresholds:
        train_mask = (train["rr_ratio"] <= thresh) & (
            train["htf_trend_aligned"] == True
        )
        val_mask = (val["rr_ratio"] <= thresh) & (val["htf_trend_aligned"] == True)

        train_n = train_mask.sum()
        val_n = val_mask.sum()

        if train_n > 10 and val_n > 10:
            train_wr = train.loc[train_mask, "anchor_hit"].mean()
            train_r = train.loc[train_mask, "outcome_r"].mean()
            val_wr = val.loc[val_mask, "anchor_hit"].mean()
            val_r = val.loc[val_mask, "outcome_r"].mean()
            print(
                f"{thresh:>8.2f} | {train_n:>8} {train_wr * 100:>5.1f}% {train_r:>8.2f} | "
                f"{val_n:>8} {val_wr * 100:>5.1f}% {val_r:>8.2f}"
            )


if __name__ == "__main__":
    main()
