"""
Step 3: Feature Engineering & Analysis
======================================
Goal: Find features that ACTUALLY predict outcomes, not random correlations.

Approach:
1. Univariate analysis - which single features have predictive power?
2. Use TRAIN data only for discovery
3. Check if patterns hold in VAL data (out-of-sample)
4. Avoid correlation traps (if A and B are correlated, pick one)
"""

from pathlib import Path

import numpy as np
import pandas as pd
from scipy import stats

DATA_DIR = Path(__file__).parent


def load_splits():
    """Load train and validation data."""
    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    return train, val


def calculate_feature_power(df, feature, target="anchor_touch_60m"):
    """
    Calculate predictive power of a boolean/categorical feature.
    Returns lift (how much better than baseline).
    """
    if feature not in df.columns:
        return None

    baseline = df[target].mean()

    # For boolean features
    if df[feature].dtype == bool or set(df[feature].dropna().unique()).issubset(
        {True, False, 0, 1}
    ):
        true_rate = (
            df[df[feature] == True][target].mean()
            if (df[feature] == True).sum() > 0
            else baseline
        )
        false_rate = (
            df[df[feature] == False][target].mean()
            if (df[feature] == False).sum() > 0
            else baseline
        )

        n_true = (df[feature] == True).sum()
        n_false = (df[feature] == False).sum()

        lift_true = true_rate / baseline if baseline > 0 else 1.0
        lift_false = false_rate / baseline if baseline > 0 else 1.0

        return {
            "feature": feature,
            "true_rate": true_rate,
            "false_rate": false_rate,
            "n_true": n_true,
            "n_false": n_false,
            "lift_true": lift_true,
            "lift_false": lift_false,
            "best_lift": max(lift_true, lift_false),
            "spread": abs(true_rate - false_rate),
        }

    return None


def analyze_numeric_feature(df, feature, target="anchor_touch_60m", n_bins=5):
    """
    Analyze numeric feature by binning into quantiles.
    """
    if feature not in df.columns:
        return None

    # Skip if too many missing values
    if df[feature].isna().sum() > len(df) * 0.5:
        return None

    baseline = df[target].mean()

    try:
        # Bin into quantiles
        df_temp = df[[feature, target]].dropna()
        df_temp["bin"] = pd.qcut(df_temp[feature], n_bins, duplicates="drop")

        bin_stats = df_temp.groupby("bin")[target].agg(["mean", "count"])

        best_bin = bin_stats["mean"].idxmax()
        worst_bin = bin_stats["mean"].idxmin()

        return {
            "feature": feature,
            "best_bin": str(best_bin),
            "best_rate": bin_stats["mean"].max(),
            "worst_bin": str(worst_bin),
            "worst_rate": bin_stats["mean"].min(),
            "lift_best": bin_stats["mean"].max() / baseline if baseline > 0 else 1.0,
            "spread": bin_stats["mean"].max() - bin_stats["mean"].min(),
        }
    except Exception:
        return None


def main():
    print("=" * 70)
    print("STEP 3: FEATURE ANALYSIS")
    print("=" * 70)

    train, val = load_splits()
    baseline_train = train["anchor_touch_60m"].mean()
    baseline_val = val["anchor_touch_60m"].mean()

    print(f"Train baseline win rate: {baseline_train * 100:.1f}%")
    print(f"Val baseline win rate: {baseline_val * 100:.1f}%")

    # Boolean features to analyze
    bool_features = [
        "exit_direction_aligned",
        "m1_direction_aligned",
        "m5_direction_aligned",
        "bars_aligned",
        "momentum_1m_aligned",
        "momentum_5m_aligned",
        "momentum_aligned",
        "momentum_divergence",
        "htf_trend_aligned",
        "against_htf_trend",
        "vol_expanding",
        "vol_contracting",
        "high_vol_environment",
        "low_vol_environment",
        "multi_scenario_zone",
        "high_confluence_zone",
        "tight_zone",
        "wide_zone",
        "reward_exceeds_risk",
        "tight_stop",
        "wide_stop",
        "in_optimal_session",
        "avoid_session",
        "high_activity_period",
        "low_activity_period",
        "near_session_high",
        "near_session_low",
        "session_mid_zone",
        "delta_supports_trade",
        "delta_divergence",
        "is_rth",
        "is_first_hour",
        "is_last_hour",
        "is_monday_open",
        "is_friday_close",
    ]

    # Analyze boolean features
    print("\n" + "=" * 70)
    print("BOOLEAN FEATURE ANALYSIS (sorted by spread)")
    print("=" * 70)

    results = []
    for feat in bool_features:
        res_train = calculate_feature_power(train, feat)
        res_val = calculate_feature_power(val, feat)
        if res_train and res_val:
            results.append(
                {
                    "feature": feat,
                    "train_true_rate": res_train["true_rate"],
                    "train_false_rate": res_train["false_rate"],
                    "train_spread": res_train["spread"],
                    "train_best_lift": res_train["best_lift"],
                    "val_true_rate": res_val["true_rate"],
                    "val_false_rate": res_val["false_rate"],
                    "val_spread": res_val["spread"],
                    "val_best_lift": res_val["best_lift"],
                    "n_true_train": res_train["n_true"],
                    "n_false_train": res_train["n_false"],
                }
            )

    results_df = pd.DataFrame(results)
    results_df = results_df.sort_values("train_spread", ascending=False)

    print(
        f"\n{'Feature':<30} {'Train T%':>8} {'Train F%':>8} {'Spread':>8} | {'Val T%':>8} {'Val F%':>8} {'Spread':>8} | {'Holds?':>6}"
    )
    print("-" * 110)

    for _, row in results_df.iterrows():
        # Check if direction of effect holds in validation
        train_better_when_true = row["train_true_rate"] > row["train_false_rate"]
        val_better_when_true = row["val_true_rate"] > row["val_false_rate"]
        holds = "YES" if train_better_when_true == val_better_when_true else "NO"

        print(
            f"{row['feature']:<30} {row['train_true_rate'] * 100:>7.1f}% {row['train_false_rate'] * 100:>7.1f}% {row['train_spread'] * 100:>7.2f}% | "
            f"{row['val_true_rate'] * 100:>7.1f}% {row['val_false_rate'] * 100:>7.1f}% {row['val_spread'] * 100:>7.2f}% | {holds:>6}"
        )

    # Identify robust features (hold in validation AND have meaningful spread)
    print("\n" + "=" * 70)
    print("ROBUST FEATURES (effect direction holds in validation, spread > 1%)")
    print("=" * 70)

    robust = []
    for _, row in results_df.iterrows():
        train_better_when_true = row["train_true_rate"] > row["train_false_rate"]
        val_better_when_true = row["val_true_rate"] > row["val_false_rate"]
        holds = train_better_when_true == val_better_when_true

        if holds and row["train_spread"] > 0.01:  # At least 1% spread
            robust.append(row)
            when = "TRUE" if train_better_when_true else "FALSE"
            better_rate_train = (
                row["train_true_rate"]
                if train_better_when_true
                else row["train_false_rate"]
            )
            better_rate_val = (
                row["val_true_rate"] if val_better_when_true else row["val_false_rate"]
            )
            print(
                f"  {row['feature']:<30} better when {when}: train={better_rate_train * 100:.1f}%, val={better_rate_val * 100:.1f}%"
            )

    # Analyze composite scores
    print("\n" + "=" * 70)
    print("COMPOSITE SCORE ANALYSIS")
    print("=" * 70)

    for score in ["alignment_score", "red_flag_count"]:
        print(f"\n--- {score} ---")
        for df, name in [(train, "Train"), (val, "Val")]:
            print(f"\n{name}:")
            grouped = df.groupby(score)["anchor_touch_60m"].agg(["mean", "count"])
            for idx, row in grouped.iterrows():
                if row["count"] >= 10:  # Only show if enough samples
                    print(
                        f"  {score}={idx}: win rate={row['mean'] * 100:.1f}% (n={row['count']:,})"
                    )

    # Numeric features
    print("\n" + "=" * 70)
    print("KEY NUMERIC FEATURES")
    print("=" * 70)

    numeric_features = [
        "rr_ratio",
        "cluster_count",
        "cluster_unique_scenarios",
        "volume_spike_ratio",
        "vol_percentile_session",
        "tf_alignment",
    ]

    for feat in numeric_features:
        res_train = analyze_numeric_feature(train, feat)
        res_val = analyze_numeric_feature(val, feat)
        if res_train and res_val:
            print(f"\n{feat}:")
            print(
                f"  Train: best bin {res_train['best_bin']} = {res_train['best_rate'] * 100:.1f}%, "
                f"worst = {res_train['worst_rate'] * 100:.1f}%, spread = {res_train['spread'] * 100:.2f}%"
            )
            print(
                f"  Val:   best bin {res_val['best_bin']} = {res_val['best_rate'] * 100:.1f}%, "
                f"worst = {res_val['worst_rate'] * 100:.1f}%, spread = {res_val['spread'] * 100:.2f}%"
            )

    # Save robust features list
    if robust:
        robust_df = pd.DataFrame(robust)
        robust_df.to_csv(DATA_DIR / "robust_features.csv", index=False)
        print(f"\nSaved {len(robust)} robust features to robust_features.csv")


if __name__ == "__main__":
    main()
