"""
Step 4l: Entry Confirmation + RR Filter
=======================================
Combine the best entry confirmations with favorable RR.
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def main():
    print("=" * 70)
    print("ENTRY CONFIRMATION + RR ANALYSIS")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")

    # Best confirmation: bars_aligned (most robust)
    # Also test: m5_aligned + vol_spike (good across splits)

    confirmations = {
        "bars_aligned": lambda df: df["bars_aligned"] == True,
        "m5 + vol_spike": lambda df: (df["m5_direction_aligned"] == True)
        & (df["volume_spike_ratio"] > 2),
        "momentum + vol_spike": lambda df: (df["momentum_aligned"] == True)
        & (df["volume_spike_ratio"] > 2),
        "bars + vol + mid": lambda df: (df["bars_aligned"] == True)
        & (df["volume_spike_ratio"] > 2)
        & (df["intraday_range_position"] > 0.3)
        & (df["intraday_range_position"] < 0.7),
    }

    rr_thresholds = [2, 3, 5, 7, 10]

    for conf_name, conf_fn in confirmations.items():
        print(f"\n{'=' * 60}")
        print(f"CONFIRMATION: {conf_name}")
        print(f"{'=' * 60}")

        print(f"\n{'RR >=':<8} | {'Train':^30} | {'Val':^30} | {'Test':^30}")
        print(
            f"{'':8} | {'n':>5} {'WR':>6} {'EV':>8} {'$PnL':>10} | "
            f"{'n':>5} {'WR':>6} {'EV':>8} {'$PnL':>10} | "
            f"{'n':>5} {'WR':>6} {'EV':>8} {'$PnL':>10}"
        )
        print("-" * 110)

        # First show without RR filter
        row = f"{'any':8} |"
        for df in [train, val, test]:
            mask = conf_fn(df)
            sub = df[mask]
            if len(sub) >= 10:
                wr = sub["anchor_hit"].mean()
                ev = sub["outcome_r"].mean()
                pnl = sub["net_pnl"].sum()
                row += f" {len(sub):>5} {wr * 100:>5.1f}% {ev:>+7.3f}R ${pnl:>9,.0f} |"
            else:
                row += f" {'n/a':>35} |"
        print(row)

        # With RR filters
        for rr_min in rr_thresholds:
            row = f"{rr_min:<8} |"
            for df in [train, val, test]:
                mask = conf_fn(df) & (df["rr_ratio"] >= rr_min)
                sub = df[mask]
                if len(sub) >= 10:
                    wr = sub["anchor_hit"].mean()
                    ev = sub["outcome_r"].mean()
                    pnl = sub["net_pnl"].sum()
                    row += (
                        f" {len(sub):>5} {wr * 100:>5.1f}% {ev:>+7.3f}R ${pnl:>9,.0f} |"
                    )
                elif len(sub) > 0:
                    row += f" {len(sub):>5} {'(few)':>20} |"
                else:
                    row += f" {'n/a':>35} |"
            print(row)

        # Check direction breakdown for best combo
        print(f"\nBest combo direction breakdown (RR >= 3):")
        for df, split in [(train, "Train"), (val, "Val"), (test, "Test")]:
            mask = conf_fn(df) & (df["rr_ratio"] >= 3)
            sub = df[mask]

            if len(sub) < 5:
                continue

            print(f"\n  {split}:")
            for d in ["long", "short"]:
                dsub = sub[sub["direction"] == d]
                if len(dsub) >= 3:
                    wr = dsub["anchor_hit"].mean()
                    ev = dsub["outcome_r"].mean()
                    pnl = dsub["net_pnl"].sum()
                    print(
                        f"    {d}: n={len(dsub)}, WR={wr * 100:.1f}%, EV={ev:+.3f}R, PnL=${pnl:,.0f}"
                    )

    # Final summary
    print("\n" + "=" * 70)
    print("SUMMARY: Best Entry Rules")
    print("=" * 70)

    print("""
    Entry Confirmation Options (pick one):

    1. bars_aligned (M1 + M5 bars confirm direction)
       - Most robust across splits
       - ~11% WR (baseline 7%)

    2. m5_aligned + vol_spike > 2
       - Good volume confirmation
       - ~12% WR

    3. momentum_aligned + vol_spike > 2
       - Momentum + volume
       - ~13% WR

    4. bars + vol + mid_session
       - Highest WR (~18%)
       - Fewer trades

    RR Filter:
    - RR >= 3 seems to be sweet spot (enough trades, better EV)
    - Higher RR = fewer trades but not necessarily better EV

    RECOMMENDED ENTRY RULE:
    - Wait for zone exit
    - Confirm: bars_aligned OR (m5_aligned + high volume)
    - Filter: RR >= 3 (zone far enough from anchor to be worth it)
    """)


if __name__ == "__main__":
    main()
