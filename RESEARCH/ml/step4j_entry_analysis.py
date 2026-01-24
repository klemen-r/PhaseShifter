"""
Step 4j: Entry Analysis - What predicts success at entry moment?
================================================================
Looking at:
- Price action leading into entry
- How price exited the zone
- Bar characteristics at entry
- Momentum/velocity at entry
"""

from pathlib import Path

import numpy as np
import pandas as pd

DATA_DIR = Path(__file__).parent


def analyze_feature(df, feature, target="anchor_hit", bins=5):
    """Analyze a numeric feature's relationship to winning."""
    if feature not in df.columns:
        return

    valid = df[df[feature].notna()]
    if len(valid) < 100:
        return

    try:
        valid["bin"] = pd.qcut(valid[feature], bins, duplicates="drop")
        grouped = valid.groupby("bin", observed=True).agg(
            {target: ["mean", "count"], "outcome_r": "mean"}
        )
        grouped.columns = ["win_rate", "count", "ev"]

        print(f"\n{feature}:")
        for idx, row in grouped.iterrows():
            if row["count"] >= 20:
                print(
                    f"  {str(idx):<30}: WR={row['win_rate'] * 100:>5.1f}%, EV={row['ev']:>+.3f}R, n={int(row['count'])}"
                )
    except Exception as e:
        pass


def main():
    print("=" * 70)
    print("ENTRY ANALYSIS: What predicts success?")
    print("=" * 70)

    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    test = pd.read_csv(DATA_DIR / "test.csv")
    all_data = pd.concat([train, val, test])

    print(f"\nTotal trades: {len(all_data)}")
    print(f"Baseline WR: {all_data['anchor_hit'].mean() * 100:.1f}%")

    # ==========================================
    # 1. ZONE EXIT BEHAVIOR
    # ==========================================
    print("\n" + "=" * 60)
    print("1. HOW DID PRICE EXIT THE ZONE?")
    print("=" * 60)

    # Exit direction vs trade direction
    print("\nExit direction alignment:")
    for aligned in [True, False]:
        sub = all_data[all_data["exit_direction_aligned"] == aligned]
        print(
            f"  exit_aligned={aligned}: WR={sub['anchor_hit'].mean() * 100:.1f}%, "
            f"EV={sub['outcome_r'].mean():+.3f}R, n={len(sub)}"
        )

    # For LONGS: should exit below zone (rejected from bottom)
    # For SHORTS: should exit above zone (rejected from top)
    print("\nBy direction and exit:")
    for direction in ["long", "short"]:
        dsub = all_data[all_data["direction"] == direction]
        for exit_above in [True, False]:
            sub = dsub[dsub["exited_above_zone"] == exit_above]
            if len(sub) > 50:
                exit_type = "above" if exit_above else "below"
                print(
                    f"  {direction} exited {exit_type}: WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                    f"EV={sub['outcome_r'].mean():+.3f}R, n={len(sub)}"
                )

    # ==========================================
    # 2. PRICE ACTION AT ENTRY
    # ==========================================
    print("\n" + "=" * 60)
    print("2. PRICE ACTION / MOMENTUM AT ENTRY")
    print("=" * 60)

    # Returns at different timeframes
    for col in ["ret_1s", "ret_5s", "ret_10s", "ret_30s", "ret_1m", "ret_5m"]:
        analyze_feature(all_data, col)

    # Tick velocity
    analyze_feature(all_data, "tick_velocity")
    analyze_feature(all_data, "tick_acceleration")

    # ==========================================
    # 3. BAR CHARACTERISTICS AT ENTRY
    # ==========================================
    print("\n" + "=" * 60)
    print("3. BAR ALIGNMENT AT ENTRY")
    print("=" * 60)

    # M1 bar direction
    print("\nM1 bar at signal:")
    for aligned in [True, False]:
        sub = all_data[all_data["m1_direction_aligned"] == aligned]
        if len(sub) > 50:
            print(
                f"  m1_aligned={aligned}: WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():+.3f}R, n={len(sub)}"
            )

    # M5 bar direction
    print("\nM5 bar at signal:")
    for aligned in [True, False]:
        sub = all_data[all_data["m5_direction_aligned"] == aligned]
        if len(sub) > 50:
            print(
                f"  m5_aligned={aligned}: WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():+.3f}R, n={len(sub)}"
            )

    # Both bars aligned
    print("\nBoth M1+M5 aligned:")
    for aligned in [True, False]:
        sub = all_data[all_data["bars_aligned"] == aligned]
        if len(sub) > 50:
            print(
                f"  bars_aligned={aligned}: WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                f"EV={sub['outcome_r'].mean():+.3f}R, n={len(sub)}"
            )

    # ==========================================
    # 4. MOMENTUM ALIGNMENT
    # ==========================================
    print("\n" + "=" * 60)
    print("4. MOMENTUM ALIGNMENT")
    print("=" * 60)

    for col in ["momentum_1m_aligned", "momentum_5m_aligned", "momentum_aligned"]:
        print(f"\n{col}:")
        for val in [True, False]:
            sub = all_data[all_data[col] == val]
            if len(sub) > 50:
                print(
                    f"  {val}: WR={sub['anchor_hit'].mean() * 100:.1f}%, "
                    f"EV={sub['outcome_r'].mean():+.3f}R, n={len(sub)}"
                )

    # ==========================================
    # 5. VOLUME AT ENTRY
    # ==========================================
    print("\n" + "=" * 60)
    print("5. VOLUME CHARACTERISTICS AT ENTRY")
    print("=" * 60)

    analyze_feature(all_data, "volume_spike_ratio")
    analyze_feature(all_data, "cumulative_delta_5m")

    print("\ndelta_supports_trade:")
    for val in [True, False]:
        sub = all_data[all_data["delta_supports_trade"] == val]
        print(
            f"  {val}: WR={sub['anchor_hit'].mean() * 100:.1f}%, "
            f"EV={sub['outcome_r'].mean():+.3f}R, n={len(sub)}"
        )

    # ==========================================
    # 6. TIME IN ZONE
    # ==========================================
    print("\n" + "=" * 60)
    print("6. TIME SPENT IN ZONE")
    print("=" * 60)

    analyze_feature(all_data, "time_in_zone_ms")

    # ==========================================
    # 7. SWING AFTER EXIT
    # ==========================================
    print("\n" + "=" * 60)
    print("7. SWING SIZE AFTER ZONE EXIT")
    print("=" * 60)

    # Calculate swing size relative to zone
    all_data["swing_range"] = (
        all_data["swing_high_after_exit"] - all_data["swing_low_after_exit"]
    )
    all_data["swing_range_pct"] = (
        all_data["swing_range"] / all_data["cluster_mid"] * 100
    )

    analyze_feature(all_data, "swing_range_pct")
    analyze_feature(all_data, "bars_after_exit")
    analyze_feature(all_data, "ticks_after_exit")

    # ==========================================
    # 8. POSITION IN SESSION
    # ==========================================
    print("\n" + "=" * 60)
    print("8. SESSION POSITION")
    print("=" * 60)

    analyze_feature(all_data, "intraday_range_position")
    analyze_feature(all_data, "dist_from_session_high")
    analyze_feature(all_data, "dist_from_session_low")

    # ==========================================
    # 9. COMBINED ANALYSIS - What's the best entry?
    # ==========================================
    print("\n" + "=" * 60)
    print("9. BEST ENTRY COMBINATIONS")
    print("=" * 60)

    # High volume spike + momentum aligned
    print("\nVolume spike > 2 + momentum aligned:")
    mask = (all_data["volume_spike_ratio"] > 2) & (all_data["momentum_aligned"] == True)
    sub = all_data[mask]
    print(
        f"  n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, EV={sub['outcome_r'].mean():+.3f}R"
    )

    # Delta + momentum
    print("\nDelta supports + momentum aligned:")
    mask = (all_data["delta_supports_trade"] == True) & (
        all_data["momentum_aligned"] == True
    )
    sub = all_data[mask]
    print(
        f"  n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, EV={sub['outcome_r'].mean():+.3f}R"
    )

    # Strong momentum (ret_1m > 0 for longs, < 0 for shorts)
    print("\nStrong 1m momentum in trade direction:")
    mask_long = (all_data["direction"] == "long") & (all_data["ret_1m"] > 0.001)
    mask_short = (all_data["direction"] == "short") & (all_data["ret_1m"] < -0.001)
    sub = all_data[mask_long | mask_short]
    print(
        f"  n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, EV={sub['outcome_r'].mean():+.3f}R"
    )

    # Weak momentum (potential reversal)
    print("\nWeak/contrary momentum (potential reversal entry):")
    mask_long = (all_data["direction"] == "long") & (all_data["ret_1m"] < 0)
    mask_short = (all_data["direction"] == "short") & (all_data["ret_1m"] > 0)
    sub = all_data[mask_long | mask_short]
    print(
        f"  n={len(sub)}, WR={sub['anchor_hit'].mean() * 100:.1f}%, EV={sub['outcome_r'].mean():+.3f}R"
    )


if __name__ == "__main__":
    main()
