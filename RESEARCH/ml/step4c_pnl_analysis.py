"""
Step 4c: P&L Analysis - Does high win rate translate to profit?
===============================================================
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def main():
    print("=" * 70)
    print("P&L ANALYSIS: Win Rate vs Actual Profit")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")

    # The key question: does the high win rate filter produce positive expectancy?

    print("\n=== Baseline (All Trades) ===")
    for df, name in [(train, "Train"), (val, "Val")]:
        print(f"\n{name}:")
        print(f"  Total trades: {len(df):,}")
        print(f"  Win rate: {df['anchor_hit'].mean() * 100:.1f}%")
        print(f"  Mean R: {df['outcome_r'].mean():.3f}")
        print(f"  Sum PnL: ${df['net_pnl'].sum():,.2f}")
        print(f"  Mean PnL/trade: ${df['net_pnl'].mean():.2f}")

    print("\n=== Filter: RR <= 3 (captures ~5% of trades) ===")
    for df, name in [(train, "Train"), (val, "Val")]:
        mask = df["rr_ratio"] <= 3.0
        sub = df[mask]
        print(f"\n{name}:")
        print(f"  Trades: {len(sub):,} ({len(sub) / len(df) * 100:.1f}%)")
        print(f"  Win rate: {sub['anchor_hit'].mean() * 100:.1f}%")
        print(f"  Mean R: {sub['outcome_r'].mean():.3f}")
        print(f"  Sum PnL: ${sub['net_pnl'].sum():,.2f}")
        print(f"  Mean PnL/trade: ${sub['net_pnl'].mean():.2f}")

    print("\n=== Filter: RR <= 2 ===")
    for df, name in [(train, "Train"), (val, "Val")]:
        mask = df["rr_ratio"] <= 2.0
        sub = df[mask]
        print(f"\n{name}:")
        print(f"  Trades: {len(sub):,} ({len(sub) / len(df) * 100:.1f}%)")
        print(f"  Win rate: {sub['anchor_hit'].mean() * 100:.1f}%")
        print(f"  Mean R: {sub['outcome_r'].mean():.3f}")
        print(f"  Sum PnL: ${sub['net_pnl'].sum():,.2f}")
        print(f"  Mean PnL/trade: ${sub['net_pnl'].mean():.2f}")

    print("\n=== Profit Factor Analysis ===")
    print("(Gross Profit / Gross Loss)")

    for thresh in [1.0, 1.5, 2.0, 3.0, 5.0]:
        print(f"\nRR <= {thresh}:")
        for df, name in [(train, "Train"), (val, "Val")]:
            mask = df["rr_ratio"] <= thresh
            sub = df[mask]
            if len(sub) < 10:
                print(f"  {name}: n={len(sub)} (too few)")
                continue

            wins = sub[sub["anchor_hit"] == True]
            losses = sub[sub["anchor_hit"] == False]

            gross_profit = wins["net_pnl"].sum() if len(wins) > 0 else 0
            gross_loss = abs(losses["net_pnl"].sum()) if len(losses) > 0 else 0

            pf = gross_profit / gross_loss if gross_loss > 0 else float("inf")

            print(
                f"  {name}: n={len(sub):>4}, WR={sub['anchor_hit'].mean() * 100:>5.1f}%, "
                f"GP=${gross_profit:>8,.0f}, GL=${gross_loss:>8,.0f}, PF={pf:.2f}"
            )

    print("\n=== The Real Issue: Risk vs Reward ===")
    print("\nWhen RR < 1, reward < risk. So even 70% win rate may not be profitable.")
    print("Let's check the math:")

    for df, name in [(train, "Train"), (val, "Val")]:
        mask = df["rr_ratio"] <= 1.5
        sub = df[mask]
        if len(sub) < 10:
            continue

        print(f"\n{name} (RR <= 1.5, n={len(sub)}):")
        print(f"  Mean RR ratio: {sub['rr_ratio'].mean():.2f}")
        print(f"  Win rate: {sub['anchor_hit'].mean() * 100:.1f}%")

        # Expected value calculation
        avg_rr = sub["rr_ratio"].mean()
        wr = sub["anchor_hit"].mean()

        # If win: gain avg_rr * R
        # If lose: lose 1 * R
        ev = (wr * avg_rr) - ((1 - wr) * 1)
        print(f"  Theoretical EV (ignoring costs): {ev:.3f}R")
        print(f"  Actual mean R: {sub['outcome_r'].mean():.3f}R")

    print("\n=== What RR threshold gives positive expectancy? ===")

    for df, name in [(train, "Train"), (val, "Val")]:
        print(f"\n{name}:")
        print(
            f"{'RR <=':>8} | {'n':>6} | {'WR%':>6} | {'MeanRR':>6} | {'Theory EV':>9} | {'Actual R':>9} | {'Sum PnL':>10}"
        )
        print("-" * 75)

        for thresh in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 10.0]:
            mask = df["rr_ratio"] <= thresh
            sub = df[mask]

            if len(sub) < 5:
                continue

            avg_rr = sub["rr_ratio"].mean()
            wr = sub["anchor_hit"].mean()
            ev = (wr * avg_rr) - ((1 - wr) * 1)
            actual_r = sub["outcome_r"].mean()
            sum_pnl = sub["net_pnl"].sum()

            print(
                f"{thresh:>8.1f} | {len(sub):>6} | {wr * 100:>5.1f}% | {avg_rr:>6.2f} | {ev:>9.3f} | "
                f"{actual_r:>9.3f} | ${sum_pnl:>9,.0f}"
            )


if __name__ == "__main__":
    main()
