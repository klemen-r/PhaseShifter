"""
Step 5: Market Regime Testing + Summary
=======================================
Test the discovered filter under different market conditions.
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def main():
    print("=" * 70)
    print("STEP 5: MARKET REGIME TESTING")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")

    # Combine all for regime analysis
    all_data = pd.concat([train, val, test], ignore_index=True)
    all_data["dt"] = pd.to_datetime(all_data["trade_entry_time"], unit="ms")

    # The winning filter: delta + htf + session_mid + SHORT only
    def best_filter(df):
        return (
            (df["delta_supports_trade"] == True)
            & (df["htf_trend_aligned"] == True)
            & (df["session_mid_zone"] == True)
            & (df["direction"] == "short")
        )

    filtered = all_data[best_filter(all_data)].copy()

    print(f"\nBest Filter (shorts only): delta + htf + session_mid")
    print(f"Total trades: {len(filtered)}")
    print(f"Win rate: {filtered['anchor_hit'].mean() * 100:.1f}%")
    print(f"Net EV: {filtered['outcome_r'].mean():.3f}R")
    print(f"Total PnL: ${filtered['net_pnl'].sum():,.2f}")

    # By volatility regime
    print("\n" + "=" * 60)
    print("BY VOLATILITY REGIME")
    print("=" * 60)

    filtered["vol_regime"] = pd.cut(
        filtered["vol_percentile_session"],
        bins=[0, 30, 70, 100],
        labels=["Low Vol", "Med Vol", "High Vol"],
    )

    for regime in ["Low Vol", "Med Vol", "High Vol"]:
        sub = filtered[filtered["vol_regime"] == regime]
        if len(sub) >= 5:
            print(
                f"\n{regime}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():.3f}R, PnL=${sub['net_pnl'].sum():,.2f}"
            )

    # By time of day (session)
    print("\n" + "=" * 60)
    print("BY SESSION (RTH vs ETH)")
    print("=" * 60)

    for is_rth, name in [(True, "RTH"), (False, "ETH")]:
        sub = filtered[filtered["is_rth"] == is_rth]
        if len(sub) >= 5:
            print(
                f"\n{name}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():.3f}R, PnL=${sub['net_pnl'].sum():,.2f}"
            )

    # By day of week
    print("\n" + "=" * 60)
    print("BY DAY OF WEEK")
    print("=" * 60)

    days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    for dow in range(7):
        sub = filtered[filtered["day_of_week"] == dow]
        if len(sub) >= 3:
            print(
                f"{days[dow]}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():.3f}R"
            )

    # By RR ratio
    print("\n" + "=" * 60)
    print("BY RR RATIO")
    print("=" * 60)

    filtered["rr_bin"] = pd.cut(
        filtered["rr_ratio"],
        bins=[0, 10, 20, 30, 50, 200],
        labels=["RR<10", "RR 10-20", "RR 20-30", "RR 30-50", "RR>50"],
    )

    for rr_bin in ["RR<10", "RR 10-20", "RR 20-30", "RR 30-50", "RR>50"]:
        sub = filtered[filtered["rr_bin"] == rr_bin]
        if len(sub) >= 3:
            print(
                f"{rr_bin}: n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():.3f}R"
            )

    # Monthly breakdown
    print("\n" + "=" * 60)
    print("MONTHLY PERFORMANCE")
    print("=" * 60)

    filtered["month"] = filtered["dt"].dt.to_period("M")
    monthly = filtered.groupby("month").agg(
        {"anchor_hit": ["count", "mean"], "outcome_r": "mean", "net_pnl": "sum"}
    )
    monthly.columns = ["trades", "win_rate", "ev_r", "pnl"]

    print(f"\n{'Month':<10} {'Trades':>6} {'WR%':>6} {'EV':>8} {'PnL':>10}")
    print("-" * 45)
    for idx, row in monthly.iterrows():
        if row["trades"] >= 1:
            print(
                f"{str(idx):<10} {int(row['trades']):>6} {row['win_rate'] * 100:>5.1f}% "
                f"{row['ev_r']:>8.2f} ${row['pnl']:>9,.0f}"
            )

    # Summary
    print("\n" + "=" * 70)
    print("FINAL SUMMARY")
    print("=" * 70)

    print("""
    DISCOVERED ENTRY FILTER:
    ========================
    Take SHORT trades only when:
    1. delta_supports_trade = True (order flow confirms short)
    2. htf_trend_aligned = True (HTF trend is bearish)
    3. session_mid_zone = True (price in middle 40-60% of session range)

    PERFORMANCE ACROSS ALL DATA (Train + Val + Test):
    """)

    print(f"    Total SHORT trades matching filter: {len(filtered)}")
    print(f"    Win rate: {filtered['anchor_hit'].mean() * 100:.1f}%")
    print(
        f"    Average win: {filtered[filtered['anchor_hit'] == True]['outcome_r'].mean():.2f}R"
    )
    print(
        f"    Average loss: {filtered[filtered['anchor_hit'] == False]['outcome_r'].mean():.2f}R"
    )
    print(f"    Net expectancy: {filtered['outcome_r'].mean():.3f}R per trade")
    print(f"    Total P&L: ${filtered['net_pnl'].sum():,.2f}")

    # Compare to baseline
    baseline_shorts = all_data[all_data["direction"] == "short"]
    print(
        f"\n    Baseline (all shorts): {len(baseline_shorts)} trades, "
        f"WR={baseline_shorts['anchor_hit'].mean() * 100:.1f}%, "
        f"EV={baseline_shorts['outcome_r'].mean():.3f}R"
    )

    print(
        f"\n    IMPROVEMENT: {filtered['outcome_r'].mean() - baseline_shorts['outcome_r'].mean():.3f}R per trade"
    )

    # Trade frequency
    date_range = (filtered["dt"].max() - filtered["dt"].min()).days
    trades_per_month = len(filtered) / (date_range / 30) if date_range > 0 else 0
    print(
        f"\n    Date range: {filtered['dt'].min().date()} to {filtered['dt'].max().date()} ({date_range} days)"
    )
    print(f"    Average trades per month: {trades_per_month:.1f}")


if __name__ == "__main__":
    main()
