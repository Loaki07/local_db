# k8s_data_gen

Kubernetes observability data generator for OpenObserve anomaly detection testing.

Generates realistic K8s **logs**, **metrics**, and **traces** (10 pods across 5 namespaces) with:
- **historical** — write N days of data to a JSON file, then bulk ingest
- **live** — stream records to OpenObserve in real-time with optional anomaly injection

Each stream type uses the correct OpenObserve ingestion API:

| Stream | API endpoint | OpenObserve stream type |
|--------|-------------|------------------------|
| `k8s_logs` | `POST /_json` | `logs` |
| metrics fields | `POST /v1/metrics` (OTLP) | `metrics` (one stream per field) |
| `k8s_traces` | `POST /v1/traces` (OTLP) | `traces` |

```bash
cargo run -- help
```

---

## Quick Commands

```bash
# Generate 7 days historical data (all streams)
cargo run --release -- historical --days 7 --stream all

# Ingest all three streams
cargo run --release -- ingest ../output_k8s.json                                    # logs  → k8s_logs (/_json)
cargo run --release -- ingest ../output_k8s_metrics.json --stream k8s_metrics       # metrics → OTLP /v1/metrics
cargo run --release -- ingest ../output_k8s_traces.json  --stream k8s_traces        # traces  → OTLP /v1/traces

# Live ingestion (background all three)
cargo run --release -- live --stream logs    &
cargo run --release -- live --stream metrics &
cargo run --release -- live --stream traces  &

# Live with anomaly injection
cargo run --release -- live --stream logs    --anomaly cpu
cargo run --release -- live --stream logs    --anomaly memory
cargo run --release -- live --stream logs    --anomaly errors
cargo run --release -- live --stream logs    --anomaly restarts
cargo run --release -- live --stream logs    --anomaly latency
cargo run --release -- live --stream logs    --anomaly login
cargo run --release -- live --stream metrics --anomaly cpu
cargo run --release -- live --stream metrics --anomaly memory
cargo run --release -- live --stream metrics --anomaly latency
cargo run --release -- live --stream traces  --anomaly latency
cargo run --release -- live --stream traces  --anomaly errors
```

---

## Quick Start

```bash
cd k8s_data_gen
cargo build --release

# Generate 7 days of all streams, ingest, then stream live with a CPU anomaly
cargo run -- historical --days 7 --stream all
cargo run -- ingest ../output_k8s.json                                  # logs  → k8s_logs
cargo run -- ingest ../output_k8s_metrics.json --stream k8s_metrics     # metrics → OTLP
cargo run -- ingest ../output_k8s_traces.json  --stream k8s_traces      # traces  → OTLP
cargo run -- live --stream logs --anomaly cpu
```

---

## Commands

### `help`

```bash
cargo run -- help
```

Prints full usage, stream list, anomaly types, and example detection configs.

---

### `historical` — bulk data generation

```bash
cargo run -- historical [--days N] [--stream logs|metrics|traces|all]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--days N` | `7` | How many days of data to generate |
| `--stream` | `logs` | Which stream to generate (`logs`, `metrics`, `traces`, or `all`) |

Generates flat JSON files. The `ingest` command then converts them to the right format for each stream type.

**Output files:**

| Stream | Output file | Size (per day) |
|--------|------------|----------------|
| `logs` | `../output_k8s.json` | ~40 MB |
| `metrics` | `../output_k8s_metrics.json` | ~30 MB |
| `traces` | `../output_k8s_traces.json` | ~60 MB (includes child spans) |

```bash
cargo run -- historical                              # 7 days of logs
cargo run -- historical --days 2                     # 2 days of logs
cargo run -- historical --stream metrics             # 7 days of metrics
cargo run -- historical --stream traces              # 7 days of traces
cargo run -- historical --days 7 --stream all        # all three streams
```

---

### `ingest` — batch upload to OpenObserve

The endpoint and batch size depend on the stream:

| `--stream` | Endpoint | Batch | Notes |
|------------|----------|-------|-------|
| `k8s_logs` (default) | `POST /{stream}/_json` | 2,000 | flat JSON array |
| `k8s_metrics` | `POST /v1/metrics` (OTLP) | 100 | each record → 10 gauge metrics |
| `k8s_traces` | `POST /v1/traces` (OTLP) | 200 | `stream-name: k8s_traces` header |

```bash
cargo run -- ingest [FILE] [--org ORG] [--stream STREAM]
```

| Argument | Default | Description |
|----------|---------|-------------|
| `FILE` | `../output_k8s.json` | Path to JSON file produced by `historical` |
| `--org` | `default` | OpenObserve org ID |
| `--stream` | `k8s_logs` | Stream name |

```bash
# Ingest logs (default)
cargo run -- ingest

# Ingest metrics (OTLP → stream_type=metrics, per-field streams)
cargo run -- ingest ../output_k8s_metrics.json --stream k8s_metrics

# Ingest traces (OTLP → stream_type=traces, stream=k8s_traces)
cargo run -- ingest ../output_k8s_traces.json  --stream k8s_traces

# Override org
cargo run -- ingest ../output_k8s.json --org myorg --stream k8s_logs
```

---

### `live` — real-time streaming

Streams ~10 records/sec to OpenObserve. Add `--anomaly` to inject spikes.

| `--stream` | Endpoint | Stream type |
|------------|----------|-------------|
| `logs` (default) | `POST /k8s_logs/_json` | `logs` |
| `metrics` | `POST /v1/metrics` (OTLP) | `metrics` (per-field) |
| `traces` | `POST /v1/traces` (OTLP) | `traces` → `k8s_traces` |

```bash
cargo run -- live [--stream logs|metrics|traces] [--anomaly TYPE]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--stream` | `logs` | Which stream type to generate |
| `--anomaly` | none | Anomaly type to inject (see table below) |

```bash
# Normal live streams
cargo run -- live                             # logs, no anomaly
cargo run -- live --stream metrics            # metrics (OTLP), no anomaly
cargo run -- live --stream traces             # traces (OTLP), no anomaly

# Live with anomaly injection
cargo run -- live --stream logs    --anomaly cpu
cargo run -- live --stream logs    --anomaly memory
cargo run -- live --stream logs    --anomaly errors
cargo run -- live --stream logs    --anomaly restarts
cargo run -- live --stream logs    --anomaly latency
cargo run -- live --stream logs    --anomaly login
cargo run -- live --stream metrics --anomaly cpu
cargo run -- live --stream metrics --anomaly memory
cargo run -- live --stream metrics --anomaly latency
cargo run -- live --stream traces  --anomaly latency
cargo run -- live --stream traces  --anomaly errors
```

**Anomaly injection behavior:**
- 30s of normal data first (ramp-up)
- 10% chance per second to trigger a spike
- Each spike lasts 2–5 minutes, followed by a 2-minute cooldown
- Console prints `[ANOMALY ACTIVE: Xs remaining]` during spike

---

## Anomaly Types

| `--anomaly` | Streams | Field(s) affected | Normal range | During spike |
|-------------|---------|-------------------|-------------|-------------|
| `cpu` | logs, metrics | `cpu_millicores`, `cpu_percent` | 100–500mc / 1–5% | 4–7x (400–3500mc) |
| `memory` | logs, metrics | `memory_mb`, `memory_percent` | 96–900MB / 2–22% | 3.5–5x |
| `errors` | all | `log_level`, `error_rate`, `status` | 3% ERROR / <5% rate | 68% ERROR / 30–80% rate |
| `restarts` | logs, metrics | `restarts` | 0 (rarely 1) | 5–15 |
| `latency` | all | `response_time_ms`, `request_latency_ms`, `duration_ms` | 2–120ms | 15–40x normal |
| `login` | logs only | `message` | ~3 "login error" msgs/min | ~600 "login error" msgs/min |

---

## Data Schemas

### `k8s_logs` (stream_type=logs, ingested via `/_json`)

| Field | Type | Description |
|-------|------|-------------|
| `_timestamp` | i64 | Microseconds since epoch |
| `cluster` | string | prod-us-east-1 / prod-eu-west-1 / staging-us-west-2 |
| `namespace` | string | payments / inventory / frontend / monitoring / infra |
| `pod` | string | e.g. `payments-api-0004b7` |
| `container` | string | Container name |
| `node` | string | node-1 through node-5 |
| `service` | string | Service name |
| `cpu_millicores` | u32 | CPU usage |
| `memory_mb` | u32 | Memory usage in MB |
| `network_rx_bytes` | u64 | Network received bytes |
| `network_tx_bytes` | u64 | Network transmitted bytes |
| `restarts` | u32 | Pod restart count (normally 0) |
| `response_time_ms` | f64 | Request latency in ms |
| `error_rate` | f64 | Fraction of failed requests (0.0–1.0) |
| `requests_per_second` | u32 | Request throughput |
| `log_level` | string | DEBUG / INFO / WARN / ERROR |
| `event_type` | string | request / healthcheck / pod_lifecycle / login / login_error |
| `status_code` | u16 | HTTP status code |
| `message` | string | Log message text |
| `unique_id` | string | UUID per record |

### Metrics (stream_type=metrics, ingested via OTLP `/v1/metrics`)

Each field from the source record becomes a **separate metrics stream** in OpenObserve. The stream name is the field name. Each stream has a `value` column (the gauge reading) plus resource attribute columns (`service_name`, `namespace`, `pod`, `node`, `cluster`).

| Stream name | Value type | Description |
|-------------|-----------|-------------|
| `cpu_millicores` | int | CPU usage in millicores |
| `cpu_percent` | float | CPU as % of 1 core |
| `memory_mb` | int | Memory usage in MB |
| `memory_percent` | float | Memory as % of 4 GB node |
| `request_latency_ms` | float | Average request latency |
| `error_rate` | float | Fraction of failed requests (0.0–1.0) |
| `requests_per_second` | float | Request throughput |
| `network_rx_bytes_per_sec` | int | Inbound network bytes/sec |
| `network_tx_bytes_per_sec` | int | Outbound network bytes/sec |
| `restarts` | int | Pod restarts in this interval |

Resource attributes available as filter fields in all metric streams: `service_name`, `namespace`, `pod`, `node`, `cluster`.

### `k8s_traces` (stream_type=traces, ingested via OTLP `/v1/traces`)

OpenObserve flattens OTLP span fields. Key queryable fields:

| Field | Source | Description |
|-------|--------|-------------|
| `_timestamp` | span startTimeUnixNano | Span start time (microseconds) |
| `service_name` | resource attribute | Service that produced this span |
| `namespace` | resource attribute | Kubernetes namespace |
| `cluster` | resource attribute | Cluster name |
| `name` / `operation_name` | span name | e.g. `POST /checkout` |
| `duration` | end − start (ns) | Span duration in nanoseconds (OpenObserve native) |
| `duration_ms` | span attribute | Span duration in milliseconds (added for anomaly detection) |
| `status` | span attribute | `OK` or `ERROR` (string, for anomaly detection filters) |
| `http.status_code` | span attribute | HTTP status code |
| `trace_id` | span field | Trace identifier |
| `span_id` | span field | Span identifier |
| `parent_span_id` | span field | Parent span ID (empty = root) |

---

## Anomaly Detection Setup

### Step 1 — Generate and ingest training data

```bash
cargo run -- historical --days 7 --stream all
cargo run -- ingest                                                    # k8s_logs
cargo run -- ingest ../output_k8s_metrics.json --stream k8s_metrics   # OTLP metrics
cargo run -- ingest ../output_k8s_traces.json  --stream k8s_traces    # OTLP traces
```

### Step 2 — Create anomaly detection configs in OpenObserve

#### Logs — CPU spike (`--anomaly cpu`)
```json
{
  "name": "K8s CPU Spike",
  "stream_name": "k8s_logs", "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(cpu_millicores) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Logs — Memory pressure (`--anomaly memory`)
```json
{
  "name": "K8s Memory Pressure",
  "stream_name": "k8s_logs", "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(memory_mb) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Logs — Error rate spike (`--anomaly errors`)
```json
{
  "name": "K8s Error Rate Spike",
  "stream_name": "k8s_logs", "stream_type": "logs",
  "query_mode": "filters",
  "filters": [{"field": "log_level", "operator": "=", "value": "ERROR"}],
  "detection_function": "count(*)", "histogram_interval": "5m", "threshold": 97
}
```

#### Logs — Pod restarts (`--anomaly restarts`)
```json
{
  "name": "K8s Pod Restarts",
  "stream_name": "k8s_logs", "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, SUM(restarts) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Logs — Latency spike (`--anomaly latency`)
```json
{
  "name": "K8s Latency Spike",
  "stream_name": "k8s_logs", "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(response_time_ms) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Logs — Auth failure spike (`--anomaly login`)
```json
{
  "name": "K8s Auth Failure",
  "stream_name": "k8s_logs", "stream_type": "logs",
  "query_mode": "filters",
  "filters": [{"field": "message", "operator": "str_contains", "value": "login error"}],
  "detection_function": "count(*)", "histogram_interval": "5m", "threshold": 97
}
```

#### Metrics — CPU percent (`--stream metrics --anomaly cpu`)

> Stream is named `cpu_percent` (OTLP creates one stream per metric). Query the `value` field.

```json
{
  "name": "K8s Metrics CPU",
  "stream_name": "cpu_percent", "stream_type": "metrics",
  "query_mode": "filters", "filters": [],
  "detection_function": "avg", "detection_function_field": "value",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Metrics — Memory percent (`--stream metrics --anomaly memory`)
```json
{
  "name": "K8s Metrics Memory",
  "stream_name": "memory_percent", "stream_type": "metrics",
  "query_mode": "filters", "filters": [],
  "detection_function": "avg", "detection_function_field": "value",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Metrics — Request latency (`--stream metrics --anomaly latency`)
```json
{
  "name": "K8s Metrics Latency",
  "stream_name": "request_latency_ms", "stream_type": "metrics",
  "query_mode": "filters", "filters": [],
  "detection_function": "avg", "detection_function_field": "value",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Metrics — Error rate for payments-api
```json
{
  "name": "K8s Metrics Error Rate",
  "stream_name": "error_rate", "stream_type": "metrics",
  "query_mode": "filters",
  "filters": [{"field": "service_name", "operator": "=", "value": "payments-api"}],
  "detection_function": "avg", "detection_function_field": "value",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Traces — Span duration (`--stream traces --anomaly latency`)
```json
{
  "name": "K8s Trace Latency",
  "stream_name": "k8s_traces", "stream_type": "traces",
  "query_mode": "filters", "filters": [],
  "detection_function": "avg", "detection_function_field": "duration_ms",
  "histogram_interval": "5m", "threshold": 97
}
```

#### Traces — Error span count (`--stream traces --anomaly errors`)
```json
{
  "name": "K8s Trace Errors",
  "stream_name": "k8s_traces", "stream_type": "traces",
  "query_mode": "filters",
  "filters": [{"field": "status", "operator": "=", "value": "ERROR"}],
  "detection_function": "count(*)", "histogram_interval": "5m", "threshold": 97
}
```

#### Traces — payments-api latency (per-service, custom SQL)
```json
{
  "name": "K8s payments-api Latency",
  "stream_name": "k8s_traces", "stream_type": "traces",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(duration_ms) AS zo_sql_val FROM \"k8s_traces\" WHERE service_name='payments-api' GROUP BY zo_sql_time ORDER BY zo_sql_time",
  "histogram_interval": "5m", "threshold": 97
}
```

### Step 3 — Train models

Use the OpenObserve UI "Re-train" button, or via API:
```bash
curl -X PATCH "http://localhost:5080/api/v1/default/anomaly_detection/<config_id>/train" \
  -u root@example.com:Complexpass#123
```

### Step 4 — Stream with anomaly injection

```bash
cargo run -- live --stream logs    --anomaly cpu
cargo run -- live --stream metrics --anomaly latency
cargo run -- live --stream traces  --anomaly errors
```

Then query the `_anomalies` stream in OpenObserve:
```sql
SELECT _timestamp, config_name, score, threshold_value
FROM "_anomalies"
ORDER BY _timestamp DESC
LIMIT 50
```

---

## Config

Edit constants at the top of `src/main.rs`:

| Constant | Default | Description |
|----------|---------|-------------|
| `API_BASE` | `http://localhost:5080` | OpenObserve base URL |
| `DEFAULT_ORG` | `default` | Default org ID |
| `USERNAME` | `root@example.com` | Auth username |
| `PASSWORD` | `Complexpass#123` | Auth password |
| `INTERVAL_SECONDS` | `10` | Seconds between records per pod (historical) |
| `PODS_PER_TICK` | `10` | Records sent per second (live) |
