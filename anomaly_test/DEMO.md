# Anomaly Detection Demo Guide

## Config IDs (current)
```
CPU_ID     = 3A4P09J34t8cmPNTfcAU7zageUl   K8s CPU Spike
MEM_ID     = 3A4P09baY11IbbSz0mjJmKcIQbY   K8s Memory Pressure
ERR_ID     = 3A4P07NLCdqMjuWEL5i9qKYsWBC   K8s Error Rate Spike
RST_ID     = 3A4P0C4pIaWeD01J1p1R9IfSClF   K8s Pod Restarts
LAT_ID     = 3A4P0CYmsrwgy5vi078eHbZY7vZ   K8s Latency Spike
LOGIN_ID   = 3A4P08B0X7zuT5yfXBANDwQrSSu   K8s Auth Failure Spike
```

---

## Before the demo (do this 10 min before)

```fish
# Terminal 1 — start live ingest WITH login anomaly injection
cd /Users/lin/o2/local_db/k8s_data_gen
cargo run --release -- live --anomaly login

# Terminal 2 — run detection a couple times so _anomalies is populated
cd /Users/lin/o2/local_db/anomaly_test
source config_ids.fish
./05_run_detection.sh $LOGIN_ID
```

Open in browser (keep these tabs ready):
1. OpenObserve → Logs → stream: `k8s_logs`    (raw data flowing in)
2. OpenObserve → Logs → stream: `_anomalies`  (detected anomalies)
3. OpenObserve → Alerts                        (to show alerting)

---

## Demo flow

### 1. Show raw data (1 min)

**Tab: `k8s_logs`, time range: Last 15 minutes**

> "This is live K8s observability data — 10 pods across 5 namespaces streaming
> in at 10 records/sec. Each record has CPU, memory, response time, error rate,
> log level, and message fields."

Click a record, expand it — point out:
- `message`: "login error: invalid credentials for user"
- `log_level`, `cpu_millicores`, `response_time_ms`

---

### 2. Explain what anomaly detection does (2 min)

> "We built anomaly detection directly into OpenObserve.
>
> You create a config — point it at a stream, tell it what to measure.
> You train it on 7 days of historical data — it builds a Random Cut Forest
> model that learns what *normal* looks like for that metric.
> Every N minutes the scheduler scores new data against that model.
> Anomalies go into the `_anomalies` stream — queryable and alertable
> like any other stream."

---

### 3. Show the detection config (1 min)

Run in Terminal 2:
```bash
curl -s http://localhost:5080/api/default/anomaly_detection/$LOGIN_ID \
  -u root@example.com:Complexpass#123 | python3 -m json.tool | head -30
```

Point out:
- `"query_mode": "filters"` + `"filters": [{"field":"message","operator":"contains","value":"login error"}]`
- `"detection_function": "count"`
- `"detection_interval": "5m"`
- `"training_window_days": 7`
- `"sensitivity": 7`
- `"is_trained": true`

> "Trained. The model learned that normally we see ~3 login errors per
> 5-minute bucket as background noise."

---

### 4. Inject a live anomaly (2 min)

**Show Terminal 1** — point to the output:
```
[ANOMALY] login spike started — will last 194s
[2026-02-23 12:13:10] ✓ 10 records ingested [ANOMALY ACTIVE: 190s remaining]
[2026-02-23 12:13:11] ✓ 10 records ingested [ANOMALY ACTIVE: 189s remaining]
```

> "I'm now injecting a brute-force login attack. Every record being sent
> right now has a 'login error' message. Normally ~3 per minute.
> During the attack — 600 per minute."

---

### 5. Run detection live (1 min)

**In Terminal 2:**
```bash
./05_run_detection.sh $LOGIN_ID
```

Expected output:
```
─── config 3A4P08B0X7zuT5yfXBANDwQrSSu
  anomalies_found : 57
  points_scored   : 1591
  anomalies:
    ts=...  severity=high  score=7.74  actual=600  expected=0  deviation=...%
    ts=...  severity=high  score=6.43  ...
    ...
```

> "57 anomalies detected across 1591 scored data points.
> Score of 7.74 — our threshold is 2.06 for sensitivity 7.
> The model saw 600 login errors where it expected near zero."

---

### 6. Read the anomalies stream in OpenObserve (3 min)

**Tab: `_anomalies`, time range: Last 1 Hour**

Switch to SQL mode, run:
```sql
SELECT timestamp, severity, score, actual_value, expected_value, deviation_percent
FROM _anomalies
WHERE config_id = '3A4P08B0X7zuT5yfXBANDwQrSSu'
ORDER BY _timestamp DESC
```

**How to read each field:**

| Field | What it means |
|-------|--------------|
| `timestamp` | When the anomalous 5-min bucket occurred |
| `severity` | `high` (score > threshold×2), `medium`, or `low` |
| `score` | RCF anomaly score — higher = more anomalous. Threshold shown in `threshold` field |
| `actual_value` | What the metric measured (e.g. 600 login errors in this bucket) |
| `expected_value` | What the model predicted (e.g. ~3 login errors) |
| `deviation_percent` | How far actual is from expected as a % |
| `detection_function` | `count` — we counted matching events |
| `filters_applied` | The filter used: `message contains "login error"` |
| `model_version` | Unix timestamp of when the model was trained |
| `config_id` | Links back to the detection config |

> "278 anomalies recorded in this stream from our test runs.
> This is a standard log stream — you can filter, aggregate, chart, alert on it
> exactly like any other stream in OpenObserve."

---

### 7. Show alerting (2 min)

**Tab: Alerts → New Alert**

Fill in live:
- Stream: `_anomalies`
- Condition: `severity` = `high`  OR  `score` > `5`
- Destination: (show existing Slack/PagerDuty destination if configured)

> "Because `_anomalies` is just a stream, you get all of OpenObserve's
> existing alert destinations for free — Slack, PagerDuty, email, webhooks.
> No special anomaly alert system needed."

---

### 8. Show other detectors (2 min)

```bash
./05_run_detection.sh $CPU_ID
./05_run_detection.sh $ERR_ID
```

> "Same pattern for all 6 detectors — CPU spikes, memory pressure,
> error rates, pod restarts, latency, auth failures.
> Each has its own trained model, its own sensitivity tuning."

---

## Key talking points

**Why RCF (Random Cut Forest)?**
- Unsupervised — no labeled data needed
- Works on streaming time-series
- Handles seasonality naturally (learns time-of-day patterns from training data)
- O(1) memory per data point

**Why train on ALL data, detect on filtered data?**
- If you train only on filtered events (e.g. login errors), and there are no
  login errors in historical data, the model gets 0 training rows
- Instead: train on total traffic volume → model learns "normal business hours"
  pattern; then score filtered counts against that baseline

**Sensitivity 1–10:**
- Controls the anomaly score threshold
- Sensitivity 7 = threshold ~2.06 (catches moderate deviations)
- Sensitivity 10 = very sensitive (many low-severity hits)
- Sensitivity 1 = only extreme outliers flagged

---

## What's coming next

- `expected_value` currently shows 0 — fix in progress (model needs to
  compute predicted value, not just score)
- Scheduler auto-detection — currently manual /detect calls;
  scheduler wiring to run every N minutes automatically
- UI for creating/managing detection configs (currently API only)
- Alert integration in the detection pipeline (auto-fire alerts without
  manually querying `_anomalies`)
