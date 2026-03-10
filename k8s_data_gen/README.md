# k8s_data_gen

Kubernetes observability data generator for OpenObserve anomaly detection testing.

Generates realistic K8s metrics and logs (10 pods across 5 namespaces) with two modes:
- **historical** — write N days of data to a JSON file for bulk ingest
- **live** — stream records to OpenObserve in real-time, with optional anomaly injection

---

## Quick Start

```bash
cd k8s_data_gen
cargo build --release
```

---

## Modes

### Historical — bulk data generation

```bash
cargo run -- historical              # default: 7 days → ../output_k8s.json
cargo run -- historical --days 2     # 2 days (good for anomaly training baseline)
cargo run -- historical --days 30    # 30 days
```

Output: `../output_k8s.json` (~40MB per day)

### Ingest — batch upload to OpenObserve

Splits the JSON file into batches of 2,000 records and POSTs them sequentially.
Use this instead of a single curl for large files.

```bash
# defaults: file=../output_k8s.json, org=default, stream=k8s_logs
cargo run -- ingest

# override file path
cargo run -- ingest /path/to/file.json

# override org and/or stream
cargo run -- ingest ../output_k8s.json --org myorg
cargo run -- ingest ../output_k8s.json --stream my_stream
cargo run -- ingest ../output_k8s.json --org myorg --stream my_stream
```

---

### Live — real-time streaming

```bash
cargo run -- live                          # normal data, 10 records/sec
cargo run -- live --anomaly cpu            # inject CPU spike anomalies
cargo run -- live --anomaly memory         # inject memory pressure anomalies
cargo run -- live --anomaly errors         # inject error rate spikes
cargo run -- live --anomaly restarts       # inject pod restart spikes
cargo run -- live --anomaly latency        # inject response time spikes
cargo run -- live --anomaly login          # inject auth failure spikes
```

Streams to `http://localhost:5080/api/default/k8s_logs/_json` at 10 records/sec.

**Anomaly injection behavior:**
- 30s of normal data first, then 10% chance per second to trigger a spike
- Each spike lasts 2–5 minutes, followed by a 2-minute cooldown
- Console prints `[ANOMALY ACTIVE: Xs remaining]` while injecting

---

## Data Schema

Stream name: `k8s_logs`

| Field | Type | Description |
|-------|------|-------------|
| `_timestamp` | i64 | Microseconds since epoch |
| `cluster` | string | prod-us-east-1 / prod-eu-west-1 / staging-us-west-2 |
| `namespace` | string | payments / inventory / frontend / monitoring / infra |
| `pod` | string | e.g. `payments-api-0004b7` |
| `container` | string | Container name within pod |
| `node` | string | node-1 through node-5 |
| `service` | string | Service name |
| `cpu_millicores` | u32 | CPU usage (normal: 100–500) |
| `memory_mb` | u32 | Memory usage in MB (normal: 96–900) |
| `network_rx_bytes` | u64 | Network received bytes |
| `network_tx_bytes` | u64 | Network transmitted bytes |
| `restarts` | u32 | Pod restart count (normally 0) |
| `response_time_ms` | f64 | Request latency in ms (normal: 2–120ms) |
| `error_rate` | f64 | Fraction of failed requests (normal: 0–5%) |
| `requests_per_second` | u32 | Request throughput |
| `log_level` | string | DEBUG / INFO / WARN / ERROR |
| `event_type` | string | request / healthcheck / pod_lifecycle |
| `status_code` | u16 | HTTP status code |
| `message` | string | Log message |
| `unique_id` | string | UUID per record |

---

## Anomaly Types

| `--anomaly` | Field affected | Normal range | During spike |
|------------|---------------|-------------|-------------|
| `cpu` | cpu_millicores | 100–500 | 400–3500 (4–7x) |
| `memory` | memory_mb | 96–900 | 340–4500 (3.5–5x) |
| `errors` | log_level=ERROR, error_rate | 3% errors, <5% err rate | 68% errors, 30–80% err rate |
| `restarts` | restarts | 0 (rarely 1) | 5–15 |
| `latency` | response_time_ms | 2–120ms | 30ms–4800ms (15–40x) |
| `login` | `message` field content | ~0 "login error" msgs/min | all records become "login error: ..." lines (~600/min) |

---

## Anomaly Detection Setup

### Step 1 — Generate training data

```bash
cargo run -- historical --days 2
```

Ingest into OpenObserve (stream: `k8s_logs`).

### Step 2 — Create anomaly detection configs

Use the OpenObserve API or UI. Recommended settings for all configs:
- `stream_name`: `k8s_logs`
- `stream_type`: `logs`
- `detection_interval`: `5m`
- `training_window_days`: `2`
- `sensitivity`: `6` (or `7` for errors/restarts/login)
- `alert_enabled`: `false` (for testing)

**CPU spike** — detects `--anomaly cpu`
```json
{
  "name": "K8s CPU Spike",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '1 minute') AS time_bucket, AVG(cpu_millicores) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "avg",
  "sensitivity": 6
}
```

**Memory pressure** — detects `--anomaly memory`
```json
{
  "name": "K8s Memory Pressure",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '1 minute') AS time_bucket, AVG(memory_mb) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "avg",
  "sensitivity": 6
}
```

**Error rate spike** — detects `--anomaly errors`
```json
{
  "name": "K8s Error Rate Spike",
  "query_mode": "filters",
  "filters": [{"field": "log_level", "operator": "eq", "value": "ERROR"}],
  "detection_function": "count",
  "sensitivity": 7
}
```

**Pod restarts** — detects `--anomaly restarts`
```json
{
  "name": "K8s Pod Restarts",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '1 minute') AS time_bucket, SUM(restarts) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "sum",
  "sensitivity": 7
}
```

**Latency spike** — detects `--anomaly latency`
```json
{
  "name": "K8s Latency Spike",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '1 minute') AS time_bucket, AVG(response_time_ms) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "avg",
  "sensitivity": 6
}
```

**Auth failure spike** — detects `--anomaly login`
```json
{
  "name": "K8s Auth Failure Spike",
  "query_mode": "filters",
  "filters": [{"field": "message", "operator": "contains", "value": "login error"}],
  "detection_function": "count",
  "sensitivity": 7
}
```
Normal rate: ~0 records/min match `message contains "login error"`.
During spike: all pods emit "login error: ..." messages → ~600 matches/min detected as anomaly.

### Step 3 — Train models

```bash
# For each config_id returned from Step 2:
curl -X POST "http://localhost:5080/api/v1/default/anomaly_detection/<config_id>/train" \
  -u root@example.com:Complexpass#123
```

### Step 4 — Stream with anomaly injection

```bash
cargo run -- live --anomaly cpu
# watch the _anomalies stream in OpenObserve for detections
```

Query detected anomalies:
```sql
SELECT timestamp, severity, score, actual_value, expected_value, deviation_percent
FROM _anomalies
WHERE config_id = '<config_id>'
ORDER BY _timestamp DESC
LIMIT 50
```

---

## Config

Edit constants at the top of `src/main.rs`:

| Constant | Default | Description |
|----------|---------|-------------|
| `API_URL` | `http://localhost:5080/api/default/k8s_logs/_json` | Ingest endpoint |
| `USERNAME` | `root@example.com` | OpenObserve username |
| `PASSWORD` | `Complexpass#123` | OpenObserve password |
| `INTERVAL_SECONDS` | `10` | Seconds between records per pod (historical) |
| `PODS_PER_TICK` | `10` | Records sent per second (live) |
