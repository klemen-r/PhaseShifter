"""
Step 1: Data Acquisition and Preparation (Interaction-Only)
===========================================================
- Load interaction dataset
- Verify data integrity
- Check for outliers
- Document coverage
"""

import pandas as pd
import numpy as np
from pathlib import Path

DATA_DIR = Path(__file__).parent.parent
DATA_FILE = DATA_DIR / "zone_interactions.csv"


def load_data():
    """Load interaction dataset."""
    if not DATA_FILE.exists():
        raise FileNotFoundError(f"Missing data file: {DATA_FILE}")

    df = pd.read_csv(DATA_FILE)
    print(f"Loaded: {DATA_FILE}")
    print(f"Rows: {len(df):,}")
    return df


def check_timestamps(df):
    """Verify timestamp integrity."""
    print("\n=== Timestamp Analysis ===")

    df["exit_dt"] = pd.to_datetime(df["zone_exit_time"], unit="ms")

    print(f"Date range: {df['exit_dt'].min()} to {df['exit_dt'].max()}")

    dupes = df["interaction_id"].duplicated().sum()
    print(f"Duplicate IDs: {dupes}")

    is_sorted = df["zone_exit_time"].is_monotonic_increasing
    print(f"Chronologically sorted: {is_sorted}")

    df["month"] = df["exit_dt"].dt.to_period("M")
    monthly = df.groupby("month").size()
    print("\nInteractions per month:")
    print(monthly.to_string())

    return df


def check_outcomes(df):
    """Summarize interaction-only outcomes."""
    print("\n=== Interaction Outcome Summary ===")

    total = len(df)
    print(f"Total interactions: {total:,}")

    for col, label in [
        ("anchor_touch_30m", "Anchor touch (30m)"),
        ("anchor_touch_60m", "Anchor touch (60m)"),
        ("anchor_touch_session", "Anchor touch (session)"),
    ]:
        if col in df.columns:
            print(f"{label}: {df[col].mean() * 100:.1f}%")

    if "max_up_ticks_60m" in df.columns and "max_down_ticks_60m" in df.columns:
        print("\nPost-exit excursion (60m, ticks):")
        print(f"  Max up: mean={df['max_up_ticks_60m'].mean():.2f}, median={df['max_up_ticks_60m'].median():.2f}")
        print(f"  Max down: mean={df['max_down_ticks_60m'].mean():.2f}, median={df['max_down_ticks_60m'].median():.2f}")

    print("\nBy direction (anchor touch 60m):")
    if "anchor_touch_60m" in df.columns:
        for d in df["direction"].unique():
            sub = df[df["direction"] == d]
            print(
                f"  {d}: {len(sub):,} interactions, touch60m={sub['anchor_touch_60m'].mean() * 100:.1f}%"
            )


def check_outliers(df):
    """Check for outliers using z-scores."""
    print("\n=== Outlier Check ===")

    numeric_cols = [
        "time_in_zone_ms",
        "cluster_width_pct",
        "zone_to_anchor_distance_pct",
        "max_up_ticks_60m",
        "max_down_ticks_60m",
        "max_above_zone_ticks_60m",
        "max_below_zone_ticks_60m",
    ]

    for col in numeric_cols:
        if col not in df.columns:
            continue
        z = np.abs((df[col] - df[col].mean()) / df[col].std())
        outliers = (z > 3).sum()
        print(f"  {col}: {outliers} outliers (|z| > 3)")


def check_data_quality(df):
    """Check for missing values and logical consistency."""
    print("\n=== Data Quality ===")

    missing = df.isnull().sum()
    cols_with_missing = missing[missing > 0]
    if len(cols_with_missing) > 0:
        print("Columns with missing values:")
        print(cols_with_missing.to_string())
    else:
        print("No missing values in any column")

    # Logical consistency: 30m <= 60m <= session
    if {"anchor_touch_30m", "anchor_touch_60m", "anchor_touch_session"}.issubset(df.columns):
        bad_30_60 = ((df["anchor_touch_30m"] == True) & (df["anchor_touch_60m"] == False)).sum()
        bad_60_sess = ((df["anchor_touch_60m"] == True) & (df["anchor_touch_session"] == False)).sum()
        print(f"Anchor touch inconsistencies (30m->60m): {bad_30_60}")
        print(f"Anchor touch inconsistencies (60m->session): {bad_60_sess}")


def main():
    print("=" * 60)
    print("STEP 1: DATA ACQUISITION AND PREPARATION")
    print("=" * 60)

    df = load_data()
    df = check_timestamps(df)
    check_outcomes(df)
    check_outliers(df)
    check_data_quality(df)

    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    print(f"Total records: {len(df):,}")
    print(f"Date range: {df['exit_dt'].min()} to {df['exit_dt'].max()}")

    combined_path = DATA_DIR / "ml" / "combined_data.csv"
    df.to_csv(combined_path, index=False)
    print(f"\nSaved combined data to: {combined_path}")


if __name__ == "__main__":
    main()
