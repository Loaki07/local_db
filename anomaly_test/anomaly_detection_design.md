# Anomaly Detection: Design & Logic Reference

## Overview

This document explains how the OpenObserve anomaly detection system works — specifically
the relationship between training and detection, the threshold calculation, and the
meaning of each output field. It compares our implementation to the original POC.

---

## How RCF (Random Cut Forest) Works

RCF is an **unsupervised streaming anomaly detection algorithm**. Key properties:

- **No labels needed** — it learns "normal" from historical data
- **No prediction** — RCF does NOT predict future values; it scores how surprising a
  new data point is relative to what it has seen
- **Streaming** — points are fed one at a time; the model updates continuously
- **Score = isolation depth** — a point that is easy to isolate (far from the cluster)
  gets a high anomaly score
- **Threshold** — a static score cutoff computed from training data (98th percentile in
  the POC); points above the threshold are anomalies

Because RCF only scores, there is **no meaningful `expected_value`** — that field is
currently `0.0` and is a known limitation (not a bug).

---

## POC Implementation (Python/rrcf)

The POC at `anomaly_detector/` works as follows:

### Training (`train.py`)

```sql
SELECT
    date_part('hour',   to_timestamp(_timestamp / 1000000)) AS hour,
    date_part('minute', to_timestamp(_timestamp / 1000000)) AS minute,
    COUNT(*) AS y
FROM "default"
GROUP BY hour, minute
ORDER BY hour, minute
LIMIT 2000
```

- Queries **all events** from the stream — no WHERE filter
- Groups by **(hour, minute)** — one row per minute-of-day bucket
- Feature vector per point: `[hour, minute, count(*)]` — 3 dimensions
- Trains 100 RCTrees, each on a sample of 256 points
- Scores every training point, then computes threshold = **98th percentile of training
  scores**
- Saves model + threshold to `rrcf_model.pkl`

### Detection (`main.py`)

- Runs the **same SQL** query for the current time window (last N minutes)
- Gets latest `[hour, minute, count(*)]` point
- Scores it with the saved forest
- Compares score to saved threshold: `is_anomaly = score > threshold`

**Key insight**: the POC uses the **same query** for training and detection. It learns
"at 14:37 we normally see ~500 events/min" and flags if the current minute has a
dramatically different count.

---

## Our Implementation

### Two Query Modes

#### 1. `custom_sql` mode (CPU, memory, latency, pod restarts)

Training and detection use the **same SQL** — whatever the user wrote:

```sql
SELECT histogram(_timestamp, '1 minute') AS time_bucket, AVG(cpu_millicores) AS value
FROM k8s_logs
GROUP BY time_bucket ORDER BY time_bucket
```

- One row per time bucket
- Feature: `[value]` — 1 dimension
- Training: processes all historical buckets, feeds values to RCF
- Detection: runs same query on new data, scores each bucket

This is straightforward — same query, same metric, same units.

#### 2. `filters` mode (error rate, auth failures)

This is where it gets interesting. Training and detection use **different queries**:

**Training query (no WHERE filter):**
```sql
SELECT histogram(_timestamp, '5m') AS time_bucket, count(*) AS value
FROM k8s_logs
GROUP BY time_bucket ORDER BY time_bucket
```

**Detection query (with filter):**
```sql
SELECT histogram(_timestamp, '5m') AS time_bucket, count(*) AS value
FROM k8s_logs
WHERE message LIKE '%login error%'
GROUP BY time_bucket ORDER BY time_bucket
```

---

## The Core Design Question: Is Training Without Filter Correct?

### The concern

If we train on `count(*)` (all events) and detect on `count(*) WHERE message LIKE '%login error%'`,
we're feeding the RCF values from two different distributions:

- Training sees: **500–600 events per 5-min bucket** (total traffic)
- Detection sees: **0–5 events per 5-min bucket** (login errors only)

These are completely different scales. The model learned "normal is ~550 events per
bucket" but we're asking it to score "is 600 login errors anomalous?" — and the model
has no idea what login errors are.

### The rationale for the current design

The motivation was **avoiding 0-row training**:

> If there are no login errors in the historical 7 days (because no attacks happened),
> training with the filter returns 0 rows → model never trains → detection never works.

The "train on all traffic" approach ensures training always has data.

### Why this is still a problem

Even if training avoids the 0-row problem, the resulting model is **wrong for the task**:

- The model learned the shape of total traffic (e.g. ~550 events/5min with daily rhythm)
- Login error counts (e.g. 3–5 normally, 600 during attack) are a completely different
  scale and distribution
- The RCF will likely see all login error counts as anomalous (they're tiny compared
  to total traffic) OR as normal (if the scale difference makes them look like noise)

### The correct approach

**Train and detect with the same query** — same filter, same aggregation, same units.

For filter-based configs:
- **Training**: `SELECT histogram(...), count(*) FROM stream WHERE <filter> GROUP BY ...`
- **Detection**: same query on new data

If training returns 0 rows because no matching events exist in history:
- This is **valid signal**: the model should detect ANY non-zero count as anomalous
- Handle 0-row training by creating a "zero baseline" model that scores everything > 0
  as an anomaly, OR set a minimum synthetic count (e.g. inject a few 0-count buckets)

### Status

The current implementation uses "train without filter" as a pragmatic workaround.
The correct fix is to train with the filter, with proper handling of sparse/zero
training data. This is a known issue documented here for future work.

**For the demo and current state**: the system works well for `custom_sql` configs
(CPU, memory, latency, restarts) where training = detection query. For filter-based
configs (login errors, error rate), results may be less meaningful because of this
unit mismatch — but spikes are large enough that they're still detected.

---

## Threshold Calculation

### POC approach (correct)

```python
# At training time:
anomaly_scores = score_all_training_points(forest, X)
threshold = np.percentile(anomaly_scores, 98)  # 98th percentile
# Save threshold with model

# At detection time:
score = forest.score(new_point)
is_anomaly = score > threshold  # Use saved threshold
```

The threshold is **computed once from training data** and **stored with the model**.

### Current implementation (needs fixing)

```rust
// At detection time (WRONG):
fn calculate_threshold(&self, scored_points: &[(Vec<f64>, f64)]) -> Result<f64> {
    // Computes mean + N*std_dev of CURRENT DETECTION BATCH
    let mean = scores.iter().sum() / scores.len();
    let std_dev = ...;
    mean + (sensitivity_factor * std_dev)
}
```

**Problems:**
1. Threshold shifts with each detection run (not stable)
2. If detection batch is all anomalous, threshold rises → anomalies missed
3. If detection batch is all normal, threshold drops → false positives spike
4. `sensitivity` maps to std_dev multiplier (not percentile) — arbitrary

### Target approach

Compute threshold at training time:
1. After training the RCF on all historical points, score each point
2. Compute `threshold = percentile(training_scores, percentile_for_sensitivity)`
3. Store threshold inside `RCFModel`
4. At detection time: use stored threshold directly

Sensitivity → percentile mapping:
```
sensitivity  1 → 99.5th percentile (very few anomalies)
sensitivity  5 → 98th percentile   (matches POC default)
sensitivity  7 → 96th percentile
sensitivity 10 → 90th percentile   (many anomalies)
```

---

## `deviation_percent` Field

### Current (broken)

```rust
deviation_percent: ((actual_value - a.threshold) / a.threshold * 100.0).abs()
//                   ^^^^^^^^^^^   ^^^^^^^^^^^
//                   metric units  RCF score units  ← INCOMPATIBLE
```

`actual_value` = the metric measurement (e.g. 600 login errors)
`a.threshold` = an RCF anomaly score (e.g. 2.06)

These are different units — the result is meaningless.

### Fix (Option A — score-relative)

```rust
deviation_percent: ((score - threshold) / threshold * 100.0).clamp(0.0, f64::MAX)
```

This computes "how far above the threshold is this score, as a percentage of threshold":
- threshold=2.06, score=7.74 → deviation = (7.74-2.06)/2.06 × 100 = **275%**
- threshold=2.06, score=2.10 → deviation = **1.9%** (barely anomalous)
- threshold=2.06, score=6.43 → deviation = **212%**

This is meaningful: "the anomaly score is 275% above the normal threshold."

### Why not Option B (actual vs expected)?

Option B would compute `(actual_value - expected_value) / expected_value * 100`.
This requires a meaningful `expected_value`, which RCF cannot provide (it only scores,
doesn't predict). This would require a separate forecasting model (e.g. ARIMA, Holt-Winters).
Leave `expected_value = 0.0` with a TODO for now.

---

## Output Fields Explained

| Field | Meaning | Example |
|-------|---------|---------|
| `timestamp` | When the anomalous time bucket occurred | `2026-02-24T12:30:00` |
| `score` | RCF anomaly score — higher = more anomalous | `7.74` |
| `threshold` | Score cutoff (from training, percentile-based) | `2.06` |
| `severity` | `high` (score > threshold×2), `medium`, `low` | `high` |
| `actual_value` | What the metric measured in this bucket | `600` (login errors) |
| `expected_value` | Always `0.0` — RCF cannot predict values | `0.0` |
| `deviation_percent` | How far score exceeded threshold, as % | `275%` |
| `detection_function` | Aggregation used | `count` |
| `filters_applied` | Filters used in detection query | `[{message contains "login error"}]` |
| `model_version` | Unix timestamp of when model was trained | `1740391234` |
| `config_id` | Links to the detection configuration | `3A4P08B0...` |

---

## Sensitivity Tuning Guide

| Sensitivity | Percentile | Use case |
|-------------|-----------|----------|
| 1–2 | 99.5th | Only extreme outliers; very stable metrics |
| 3–4 | 99th | Conservative; few false positives |
| 5 | 98th | **Default** (matches POC); balanced |
| 6–7 | 96th | Moderate sensitivity; catches moderate deviations |
| 8–9 | 94th | High sensitivity; more false positives |
| 10 | 90th | Very sensitive; noisy metrics |

---

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────┐
│                        TRAINING                             │
│                                                             │
│  Historical data (N days)                                   │
│       ↓                                                     │
│  SQL query → time_bucket + value rows                       │
│       ↓                                                     │
│  Feed each value to RCF.update()  (streaming)               │
│       ↓                                                     │
│  Score all training points → anomaly_scores[]               │
│       ↓                                                     │
│  threshold = percentile(anomaly_scores, P)  ← stored        │
│       ↓                                                     │
│  Serialize RCF + threshold → S3                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                        DETECTION                            │
│                                                             │
│  New data (since last run / last N minutes)                 │
│       ↓                                                     │
│  Same SQL query → time_bucket + value rows                  │
│       ↓                                                     │
│  Load RCF model + threshold from S3/cache                   │
│       ↓                                                     │
│  For each new value:                                        │
│    score = RCF.score(value)                                 │
│    if score > threshold → ANOMALY                           │
│       ↓                                                     │
│  Write anomalies to _anomalies stream                       │
└─────────────────────────────────────────────────────────────┘
```

---

## Known Limitations / Future Work

1. **`expected_value = 0.0`** — RCF cannot predict values; needs separate forecasting model
2. **Filter-based training mismatch** — training without filter creates unit mismatch vs
   filtered detection; should train with filter + handle sparse data
3. **No concept drift detection** — model should be retrained periodically (weekly)
4. **Single-dimensional RCF** — POC uses 3D features `[hour, minute, count]` which
   captures time-of-day patterns; our implementation uses 1D `[value]` which doesn't
   know about hour/minute patterns; `shingle_size` partially compensates
5. **Scheduler auto-run** — detection is currently triggered manually; scheduler wiring
   to auto-run every N minutes needs validation
