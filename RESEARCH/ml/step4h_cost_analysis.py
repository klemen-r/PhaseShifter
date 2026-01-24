"""
Step 4h: Cost Analysis - Is there edge before costs?
====================================================
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent

def calculate_gross_ev(df):
    """Calculate EV before costs."""
    # Win: get rr_ratio as profit
    # Lose: lose 1R
    wins = df[df["anchor_hit"] == True]
    losses = df[df["anchor_hit"] == False]

    if len(df) == 0:
        return np.nan, np.nan

    win_rate = len(wins) / len(df)
    avg_win_r = wins["rr_ratio"].mean() if len(wins) > 0 else 0

    gross_ev = (win_rate * avg_win_r) - ((1 - win_rate) * 1.0)
    net_ev = df["outcome_r"].mean()

    return gross_ev, net_ev


def main():
    print("=" * 70)
    print("COST ANALYSIS: Gross vs Net Expectancy")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")

    all_data = pd.concat([train, val, test])

    print("\n=== BASELINE (ALL TRADES) ===")
    for df, name in [(train, "Train"), (val, "Val"), (test, "Test"), (all_data, "ALL")]:
        gross, net = calculate_gross_ev(df)
        cost = gross - net
        print(
            f"{name:>6}: gross={gross:>+.3f}R, net={net:>+.3f}R, cost={cost:>.3f}R, n={len(df)}"
        )

    print("\n=== BY DIRECTION ===")
    for direction in ["long", "short"]:
        print(f"\n{direction.upper()}:")
        for df, name in [(train, "Train"), (val, "Val"), (test, "Test")]:
            sub = df[df["direction"] == direction]
            gross, net = calculate_gross_ev(sub)
            cost = gross - net if not np.isnan(gross) else 0
            print(
                f"  {name:>6}: gross={gross:>+.3f}R, net={net:>+.3f}R, cost={cost:>.3f}R, n={len(sub)}"
            )

    print("\n=== FILTERS WITH POSITIVE GROSS EV ===")

    filters = [
        ("momentum", lambda df: df["momentum_aligned"] == True),
        ("bars_aligned", lambda df: df["bars_aligned"] == True),
        ("delta", lambda df: df["delta_supports_trade"] == True),
        ("htf", lambda df: df["htf_trend_aligned"] == True),
        ("high_vol", lambda df: df["high_vol_environment"] == True),
        ("vol_spike>2", lambda df: df["volume_spike_ratio"] > 2.0),
        ("align>=5", lambda df: df["alignment_score"] >= 5),
        ("align>=6", lambda df: df["alignment_score"] >= 6),
        ("rr>=10", lambda df: df["rr_ratio"] >= 10),
        ("rr>=15", lambda df: df["rr_ratio"] >= 15),
        ("rr>=20", lambda df: df["rr_ratio"] >= 20),
        (
            "momentum+htf",
            lambda df: (df["momentum_aligned"]) & (df["htf_trend_aligned"]),
        ),
        (
            "momentum+vol_spike",
            lambda df: (df["momentum_aligned"]) & (df["volume_spike_ratio"] > 1.5),
        ),
        (
            "delta+htf",
            lambda df: (df["delta_supports_trade"]) & (df["htf_trend_aligned"]),
        ),
    ]

    print(
        f"\n{'Filter':<25} | {'Train Gross':>12} {'Net':>8} | {'Val Gross':>12} {'Net':>8} | {'Test Gross':>12} {'Net':>8}"
    )
    print("-" * 100)

    for name, filt in filters:
        row = f"{name:<25} |"
        for df in [train, val, test]:
            sub = df[filt(df)]
            if len(sub) >= 20:
                gross, net = calculate_gross_ev(sub)
                row += f" {gross:>+11.3f}R {net:>+7.3f}R |"
            else:
                row += f" {'n/a':>12} {'':>8} |"
        print(row)

    print("\n=== KEY INSIGHT ===")
    print("""
    Costs are ~0.4-0.5R per trade (slippage + commission).

    To be profitable after costs, need gross EV > 0.5R

    Gross EV = (win_rate * avg_win_R) - (loss_rate * 1)

    With typical RR of 20-40:
    - Need ~3-4% win rate to break even on gross
    - Need ~5-6% win rate to cover costs

    Current baseline: ~7% win rate, ~33 avg RR
    Gross EV = 0.07 * 33 - 0.93 * 1 = 2.31 - 0.93 = +1.38R (should be positive!)

    But the data shows negative... let me verify.
    """)

    # Verify the math
    print("\n=== VERIFYING BASELINE MATH ===")
    for df, name in [(train, "Train"), (val, "Val"), (test, "Test")]:
        wr = df["anchor_hit"].mean()
        avg_rr_winners = df[df["anchor_hit"] == True]["rr_ratio"].mean()
        expected_gross = (wr * avg_rr_winners) - ((1 - wr) * 1)
        actual_gross, actual_net = calculate_gross_ev(df)

        # What's the actual average R when winning?
        actual_avg_win_r = df[df["anchor_hit"] == True]["outcome_r"].mean()
        actual_avg_loss_r = df[df["anchor_hit"] == False]["outcome_r"].mean()

        print(f"\n{name}:")
        print(f"  Win rate: {wr * 100:.1f}%")
        print(f"  Avg RR of winners: {avg_rr_winners:.1f}")
        print(f"  Expected gross from RR: {expected_gross:.3f}R")
        print(f"  Actual gross (from formula): {actual_gross:.3f}R")
        print(f"  Actual net: {actual_net:.3f}R")
        print(f"  Actual avg win R (after costs): {actual_avg_win_r:.3f}R")
        print(f"  Actual avg loss R (after costs): {actual_avg_loss_r:.3f}R")

        # Check a few individual winning trades
        print(f"\n  Sample winning trades:")
        winners = df[df["anchor_hit"] == True].head(5)
        for _, w in winners.iterrows():
            print(
                f"    RR={w['rr_ratio']:.1f}, outcome_r={w['outcome_r']:.3f}, net_pnl=${w['net_pnl']:.2f}"
            )

            print(f"    RR={w['rr_ratio']:.1f}, outcome_r={w['outcome_r']:.3f}, net_pnl=${w['net_pnl']:.2f}")

if __name__ == "__main__":
    main()
