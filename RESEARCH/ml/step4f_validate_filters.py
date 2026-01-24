"""
Step 4f: Validate promising filters on TEST set
===============================================
Only now do we touch the test set - to validate our findings.
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def analyze_filter(train, val, test, mask_fn, name):
    """Full analysis of a filter across all splits."""
    print(f"\n{'=' * 60}")
    print(f"FILTER: {name}")
    print(f"{'=' * 60}")

    for df, split_name in [(train, "TRAIN"), (val, "VAL"), (test, "TEST")]:
        mask = mask_fn(df)
        sub = df[mask]

        if len(sub) < 5:
            print(f"\n{split_name}: n={len(sub)} (too few)")
            continue

        wr = sub["anchor_hit"].mean()
        avg_rr = sub["rr_ratio"].mean()
        net_ev = sub["outcome_r"].mean()
        sum_pnl = sub["net_pnl"].sum()

        # Win/loss breakdown
        wins = sub[sub["anchor_hit"] == True]
        losses = sub[sub["anchor_hit"] == False]

        avg_win = wins["outcome_r"].mean() if len(wins) > 0 else 0
        avg_loss = losses["outcome_r"].mean() if len(losses) > 0 else 0

        print(f"\n{split_name}: n={len(sub)} trades")
        print(f"  Win rate: {wr * 100:.1f}%")
        print(f"  Avg RR ratio: {avg_rr:.1f}")
        print(f"  Net EV: {net_ev:.3f}R")
        print(f"  Sum PnL: ${sum_pnl:,.2f}")
        print(f"  Avg win: {avg_win:.2f}R, Avg loss: {avg_loss:.2f}R")

        # By direction
        print(f"  By direction:")
        for d in ["long", "short"]:
            dsub = sub[sub["direction"] == d]
            if len(dsub) >= 3:
                print(
                    f"    {d}: n={len(dsub)}, WR={dsub['anchor_hit'].mean() * 100:.1f}%, EV={dsub['outcome_r'].mean():.3f}R"
                )


def main():
    print("=" * 70)
    print("STEP 4f: FINAL VALIDATION ON TEST SET")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")

    print(f"\nDataset sizes: Train={len(train)}, Val={len(val)}, Test={len(test)}")

    # Filter 1: delta + htf_aligned + session_mid
    filter1 = lambda df: (
        (df["delta_supports_trade"] == True)
        & (df["htf_trend_aligned"] == True)
        & (df["session_mid_zone"] == True)
    )
    analyze_filter(train, val, test, filter1, "delta + htf_aligned + session_mid")

    # Filter 2: delta + rr_20plus + session_mid
    filter2 = lambda df: (
        (df["delta_supports_trade"] == True)
        & (df["rr_ratio"] >= 20)
        & (df["session_mid_zone"] == True)
    )
    analyze_filter(train, val, test, filter2, "delta + rr>=20 + session_mid")

    # Filter 3: delta + rr_15plus + session_mid (more trades)
    filter3 = lambda df: (
        (df["delta_supports_trade"] == True)
        & (df["rr_ratio"] >= 15)
        & (df["session_mid_zone"] == True)
    )
    analyze_filter(train, val, test, filter3, "delta + rr>=15 + session_mid")

    # Filter 4: delta + rr_10plus + session_mid (even more trades)
    filter4 = lambda df: (
        (df["delta_supports_trade"] == True)
        & (df["rr_ratio"] >= 10)
        & (df["session_mid_zone"] == True)
    )
    analyze_filter(train, val, test, filter4, "delta + rr>=10 + session_mid")

    # Let's also try without session_mid to get more trades
    filter5 = lambda df: (
        (df["delta_supports_trade"] == True) & (df["htf_trend_aligned"] == True)
    )
    analyze_filter(train, val, test, filter5, "delta + htf_aligned (no session_mid)")

    # Check just delta on ETH with high RR
    filter6 = lambda df: (
        (df["delta_supports_trade"] == True)
        & (df["is_rth"] == False)
        & (df["rr_ratio"] >= 15)
    )
    analyze_filter(train, val, test, filter6, "delta + ETH + rr>=15")

    print("\n" + "=" * 70)
    print("INTERPRETATION")
    print("=" * 70)
    print("""
    The filters use:
    - delta_supports_trade: Order flow (cumulative delta) agrees with trade direction
    - htf_trend_aligned: Higher timeframe trend agrees with trade direction
    - session_mid_zone: Price is in middle 40-60% of session range
    - rr_ratio >= X: Minimum reward-to-risk ratio (anchor distance / stop distance)
    - is_rth=False (ETH): Extended trading hours

    These make fundamental sense:
    - Delta alignment = institutional flow supporting the trade
    - HTF alignment = trading with the trend
    - Session mid = not at extreme (more room to move)
    - High RR = favorable risk/reward
    """)


if __name__ == "__main__":
    main()
