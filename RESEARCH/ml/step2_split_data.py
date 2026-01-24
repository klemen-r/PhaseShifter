"""
Step 2: Dataset Splitting and Structuring (Interaction-Only)
===========================================================
- Chronological split (NO SHUFFLE)
- Train: 70% (oldest data)
- Validation: 15% (middle)
- Test: 15% (most recent)
"""

from pathlib import Path

import pandas as pd

DATA_DIR = Path(__file__).parent
COMBINED_FILE = DATA_DIR / "combined_data.csv"


def main():
    print("=" * 60)
    print("STEP 2: DATASET SPLITTING")
    print("=" * 60)

    df = pd.read_csv(COMBINED_FILE)
    print(f"Total records: {len(df):,}")

    df = df.sort_values("zone_exit_time").reset_index(drop=True)
    df["exit_dt"] = pd.to_datetime(df["zone_exit_time"], unit="ms")

    n = len(df)
    train_end = int(n * 0.70)
    val_end = int(n * 0.85)

    df_train = df.iloc[:train_end].copy()
    df_val = df.iloc[train_end:val_end].copy()
    df_test = df.iloc[val_end:].copy()

    print(f"\n{'Split':<10} {'Rows':>8} {'Pct':>8} {'Start Date':>24} {'End Date':>24}")
    print("-" * 80)

    for name, split_df in [("Train", df_train), ("Val", df_val), ("Test", df_test)]:
        pct = len(split_df) / n * 100
        start = split_df["exit_dt"].min()
        end = split_df["exit_dt"].max()
        print(
            f"{name:<10} {len(split_df):>8,} {pct:>7.1f}% {str(start):>24} {str(end):>24}"
        )

    print(f"\n{'Split':<10} {'AnchorTouch60m':>16} {'MeanMaxUp60m':>14} {'MeanMaxDown60m':>16}")
    print("-" * 70)
    for name, split_df in [("Train", df_train), ("Val", df_val), ("Test", df_test)]:
        touch = split_df["anchor_touch_60m"].mean() * 100 if "anchor_touch_60m" in split_df else 0
        mean_up = split_df["max_up_ticks_60m"].mean() if "max_up_ticks_60m" in split_df else 0
        mean_down = split_df["max_down_ticks_60m"].mean() if "max_down_ticks_60m" in split_df else 0
        print(f"{name:<10} {touch:>15.1f}% {mean_up:>14.2f} {mean_down:>16.2f}")

    print("\n=== Overlap Check ===")
    train_max = df_train["zone_exit_time"].max()
    val_min = df_val["zone_exit_time"].min()
    val_max = df_val["zone_exit_time"].max()
    test_min = df_test["zone_exit_time"].min()

    print(f"Train ends:  {pd.to_datetime(train_max, unit='ms')}")
    print(f"Val starts:  {pd.to_datetime(val_min, unit='ms')}")
    print(f"Val ends:    {pd.to_datetime(val_max, unit='ms')}")
    print(f"Test starts: {pd.to_datetime(test_min, unit='ms')}")

    gap_train_val = (val_min - train_max) / 1000 / 60
    gap_val_test = (test_min - val_max) / 1000 / 60
    print(f"\nGap train->val: {gap_train_val:.1f} minutes")
    print(f"Gap val->test: {gap_val_test:.1f} minutes")

    df_train.to_csv(DATA_DIR / "train.csv", index=False)
    df_val.to_csv(DATA_DIR / "val.csv", index=False)
    df_test.to_csv(DATA_DIR / "test.csv", index=False)

    print(f"\nSaved:")
    print(f"  - train.csv ({len(df_train):,} rows)")
    print(f"  - val.csv ({len(df_val):,} rows)")
    print(f"  - test.csv ({len(df_test):,} rows)")

    print("\n" + "=" * 60)
    print("SPLIT BOUNDARIES FOR DOCUMENTATION")
    print("=" * 60)
    print(
        f"Train: {df_train['exit_dt'].min().date()} to {df_train['exit_dt'].max().date()}"
    )
    print(
        f"Val:   {df_val['exit_dt'].min().date()} to {df_val['exit_dt'].max().date()}"
    )
    print(
        f"Test:  {df_test['exit_dt'].min().date()} to {df_test['exit_dt'].max().date()}"
    )


if __name__ == "__main__":
    main()
