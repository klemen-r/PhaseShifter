"""
Step 4i: RR Analysis - Why don't winners achieve theoretical RR?
================================================================
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def main():
    print("=" * 70)
    print("RR ANALYSIS: Theoretical vs Actual")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")

    winners = train[train["anchor_hit"] == True].copy()
    losers = train[train["anchor_hit"] == False].copy()

    print(f"\nWinners: {len(winners)}")
    print(f"Losers: {len(losers)}")

    # RR ratio is zone_to_anchor_distance / risk_amount
    # When you win, you should get rr_ratio as your R multiple
    # Let's check if outcome_r matches rr_ratio for winners

    print("\n=== WINNERS: Theoretical vs Actual R ===")
    print(f"Mean theoretical RR (rr_ratio): {winners['rr_ratio'].mean():.2f}")
    print(f"Mean actual R (outcome_r): {winners['outcome_r'].mean():.2f}")
    print(
        f"Gap: {winners['rr_ratio'].mean() - winners['outcome_r'].mean():.2f}R (costs)"
    )

    # Distribution of winner RR
    print("\n=== WINNER RR DISTRIBUTION ===")
    print(winners["rr_ratio"].describe())

    # Most winners are low RR
    print("\n=== WINNERS BY RR BUCKET ===")
    bins = [0, 2, 5, 10, 20, 50, 1000]
    labels = ["RR<2", "RR 2-5", "RR 5-10", "RR 10-20", "RR 20-50", "RR>50"]
    winners["rr_bucket"] = pd.cut(winners["rr_ratio"], bins=bins, labels=labels)

    bucket_stats = winners.groupby("rr_bucket", observed=True).agg(
        {"rr_ratio": ["count", "mean"], "outcome_r": "mean"}
    )
    bucket_stats.columns = ["count", "avg_rr", "avg_outcome_r"]

    print(f"\n{'Bucket':<12} | {'Count':>6} | {'Avg RR':>8} | {'Avg Outcome R':>12}")
    print("-" * 50)
    for idx, row in bucket_stats.iterrows():
        print(
            f"{idx:<12} | {int(row['count']):>6} | {row['avg_rr']:>8.1f} | {row['avg_outcome_r']:>12.2f}"
        )

    # What percentage of winners come from each bucket?
    print("\n=== WHERE DO WINS COME FROM? ===")
    total_wins = len(winners)
    for idx, row in bucket_stats.iterrows():
        pct = row["count"] / total_wins * 100
        print(f"  {idx}: {pct:.1f}% of wins")

    # Now check win RATE by RR bucket (using all trades)
    print("\n=== WIN RATE BY RR BUCKET ===")
    train["rr_bucket"] = pd.cut(train["rr_ratio"], bins=bins, labels=labels)

    for bucket in labels:
        sub = train[train["rr_bucket"] == bucket]
        if len(sub) > 0:
            wr = sub["anchor_hit"].mean()
            n = len(sub)
            gross_ev = (
                wr * sub[sub["anchor_hit"] == True]["rr_ratio"].mean() if wr > 0 else 0
            ) - (1 - wr)
            print(
                f"  {bucket:<12}: n={n:>5}, WR={wr * 100:>5.1f}%, gross EV={gross_ev:>+.3f}R"
            )

    # The key insight: low RR has high win rate, high RR has low win rate
    # But which bucket has positive expectancy?

    print("\n=== EXPECTANCY BY RR BUCKET ===")
    for bucket in labels:
        sub = train[train["rr_bucket"] == bucket]
        if len(sub) >= 20:
            net_ev = sub["outcome_r"].mean()
            sum_pnl = sub["net_pnl"].sum()
            print(f"  {bucket:<12}: net EV={net_ev:>+.3f}R, total PnL=${sum_pnl:>,.0f}")


if __name__ == "__main__":
    main()
