"""
Step 4: Model Selection and Training
====================================
Goal: Simple, interpretable model that generalizes.

Strategy:
1. Start with decision tree (max_depth=3) - forces simplicity
2. Use only robust features from Step 3
3. Train on TRAIN, tune on VAL
4. NO touching TEST yet
"""

from pathlib import Path

import numpy as np
import pandas as pd
from sklearn.ensemble import GradientBoostingClassifier, RandomForestClassifier
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import classification_report, confusion_matrix, roc_auc_score
from sklearn.preprocessing import StandardScaler
from sklearn.tree import DecisionTreeClassifier, export_text

DATA_DIR = Path(__file__).parent

def load_data():
    """Load train and validation splits."""
    train = pd.read_csv(DATA_DIR / "train.csv")
    val = pd.read_csv(DATA_DIR / "val.csv")
    return train, val


def prepare_features(df, feature_cols):
    """Prepare feature matrix, handling missing values."""
    X = df[feature_cols].copy()

    # Convert booleans to int
    for col in X.columns:
        if X[col].dtype == bool:
            X[col] = X[col].astype(int)

    # Fill missing with median
    X = X.fillna(X.median())

    return X


def evaluate_model(model, X, y, name):
    """Evaluate model performance."""
    y_pred = model.predict(X)
    y_proba = model.predict_proba(X)[:, 1] if hasattr(model, "predict_proba") else None

    # Metrics
    n_total = len(y)
    n_pred_pos = y_pred.sum()
    n_actual_pos = y.sum()

    # Among predicted positives
    if n_pred_pos > 0:
        precision = y[y_pred == 1].mean()
    else:
        precision = 0

    # Among actual positives
    if n_actual_pos > 0:
        recall = y_pred[y == 1].mean()
    else:
        recall = 0

    baseline = y.mean()
    lift = precision / baseline if baseline > 0 else 0

    print(f"\n{name}:")
    print(f"  Baseline win rate: {baseline * 100:.1f}%")
    print(
        f"  Predicted positives: {n_pred_pos:,} ({n_pred_pos / n_total * 100:.1f}% of trades)"
    )
    print(f"  Precision (win rate of predictions): {precision * 100:.1f}%")
    print(f"  Recall (% of wins captured): {recall * 100:.1f}%")
    print(f"  Lift: {lift:.2f}x")

    if y_proba is not None:
        auc = roc_auc_score(y, y_proba)
        print(f"  AUC: {auc:.3f}")

    return {
        "precision": precision,
        "recall": recall,
        "lift": lift,
        "n_pred": n_pred_pos,
    }


def main():
    print("=" * 70)
    print("STEP 4: MODEL SELECTION")
    print("=" * 70)

    train, val = load_data()

    # Target
    y_train = train["anchor_hit"].astype(int)
    y_val = val["anchor_hit"].astype(int)

    # Feature sets to try
    # Minimal: only the most robust features
    minimal_features = [
        "rr_ratio",
        "momentum_aligned",
        "bars_aligned",
        "volume_spike_ratio",
    ]

    # Extended: add more robust features
    extended_features = minimal_features + [
        "alignment_score",
        "delta_supports_trade",
        "high_vol_environment",
        "vol_expanding",
        "htf_trend_aligned",
        "m5_direction_aligned",
        "high_confluence_zone",
        "session_mid_zone",
    ]

    # Full: all robust features
    full_features = extended_features + [
        "is_rth",
        "avoid_session",
        "vol_percentile_session",
        "tf_alignment",
        "cluster_count",
    ]

    print("\n" + "=" * 70)
    print("MODEL 1: Simple Decision Tree (max_depth=3)")
    print("=" * 70)

    X_train = prepare_features(train, extended_features)
    X_val = prepare_features(val, extended_features)

    dt = DecisionTreeClassifier(max_depth=3, min_samples_leaf=100, random_state=42)
    dt.fit(X_train, y_train)

    print("\nDecision Tree Rules:")
    print(export_text(dt, feature_names=extended_features, max_depth=3))

    evaluate_model(dt, X_train, y_train, "Train")
    evaluate_model(dt, X_val, y_val, "Validation")

    print("\n" + "=" * 70)
    print("MODEL 2: Logistic Regression (interpretable weights)")
    print("=" * 70)

    scaler = StandardScaler()
    X_train_scaled = scaler.fit_transform(X_train)
    X_val_scaled = scaler.transform(X_val)

    lr = LogisticRegression(max_iter=1000, random_state=42, class_weight="balanced")
    lr.fit(X_train_scaled, y_train)

    print("\nFeature Coefficients (importance):")
    coefs = pd.DataFrame({"feature": extended_features, "coef": lr.coef_[0]})
    coefs = coefs.reindex(coefs["coef"].abs().sort_values(ascending=False).index)
    for _, row in coefs.iterrows():
        direction = "+" if row["coef"] > 0 else "-"
        print(f"  {direction} {row['feature']:<30} {row['coef']:>8.3f}")

    evaluate_model(lr, X_train_scaled, y_train, "Train")
    evaluate_model(lr, X_val_scaled, y_val, "Validation")

    print("\n" + "=" * 70)
    print("MODEL 3: Gradient Boosting (small, regularized)")
    print("=" * 70)

    gb = GradientBoostingClassifier(
        n_estimators=50,
        max_depth=2,
        min_samples_leaf=100,
        learning_rate=0.1,
        random_state=42,
    )
    gb.fit(X_train, y_train)

    print("\nFeature Importances:")
    importances = pd.DataFrame(
        {"feature": extended_features, "importance": gb.feature_importances_}
    ).sort_values("importance", ascending=False)
    for _, row in importances.head(10).iterrows():
        print(f"  {row['feature']:<30} {row['importance']:>8.3f}")

    evaluate_model(gb, X_train, y_train, "Train")
    evaluate_model(gb, X_val, y_val, "Validation")

    print("\n" + "=" * 70)
    print("MODEL 4: Simple Rule-Based (no ML)")
    print("=" * 70)
    print("\nBased on feature analysis, trying explicit rules:")

    # Rule: rr_ratio > 1 AND momentum_aligned AND bars_aligned
    def rule_v1(df):
        return (
            (df["rr_ratio"] > 1.0)
            & (df["momentum_aligned"] == True)
            & (df["bars_aligned"] == True)
        )

    # Rule v2: Just high rr_ratio
    def rule_v2(df):
        return df["rr_ratio"] > 2.0

    # Rule v3: rr_ratio + alignment_score
    def rule_v3(df):
        return (df["rr_ratio"] > 1.0) & (df["alignment_score"] >= 5)

    # Rule v4: rr_ratio + volume
    def rule_v4(df):
        return (df["rr_ratio"] > 1.5) & (df["volume_spike_ratio"] > 1.5)

    rules = [
        ("v1: RR>1 + momentum + bars aligned", rule_v1),
        ("v2: RR>2 only", rule_v2),
        ("v3: RR>1 + alignment_score>=5", rule_v3),
        ("v4: RR>1.5 + volume_spike>1.5", rule_v4),
    ]

    for name, rule in rules:
        print(f"\n--- {name} ---")
        for df, split_name in [(train, "Train"), (val, "Val")]:
            mask = rule(df)
            n_signals = mask.sum()
            if n_signals > 0:
                win_rate = df.loc[mask, "anchor_hit"].mean()
                baseline = df["anchor_hit"].mean()
                lift = win_rate / baseline
                mean_r = df.loc[mask, "outcome_r"].mean()
                print(
                    f"  {split_name}: {n_signals:>5} trades ({n_signals / len(df) * 100:.1f}%), "
                    f"win rate={win_rate * 100:.1f}%, lift={lift:.2f}x, mean R={mean_r:.2f}"
                )
            else:
                print(f"  {split_name}: 0 trades")

    print("\n" + "=" * 70)
    print("THRESHOLD ANALYSIS: Finding optimal RR cutoff")
    print("=" * 70)

    for threshold in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0]:
        print(f"\nRR > {threshold}:")
        for df, name in [(train, "Train"), (val, "Val")]:
            mask = df["rr_ratio"] > threshold
            n = mask.sum()
            if n > 10:
                wr = df.loc[mask, "anchor_hit"].mean()
                mean_r = df.loc[mask, "outcome_r"].mean()
                print(
                    f"  {name}: n={n:>5}, win rate={wr * 100:>5.1f}%, mean R={mean_r:>6.2f}"
                )
            else:
                print(f"  {name}: n={n:>5} (too few)")

                print(f"  {name}: n={n:>5} (too few)")

if __name__ == "__main__":
    main()
