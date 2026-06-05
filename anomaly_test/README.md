# anomaly_test

End-to-end scripts for testing the OpenObserve anomaly detection system against `k8s_logs` data.

---

## Prerequisites

- 10 days of `k8s_logs` data ingested (via `k8s_data_gen ingest`)
- OpenObserve running at `http://localhost:5080`
- Edit `config.sh` if your org, base URL, or credentials differ

---

## Flow

```
01_create_configs.sh        Create 6 anomaly detection configs
        ↓
03_train_all.sh             Trigger training on all configs (async)
        ↓
04_wait_for_training.sh     Poll until all models are trained
        ↓
05_run_detection.sh         Run detection → get anomaly results
        ↓
06_query_anomalies_stream.sh  Query _anomalies stream to see stored records
        ↓
07_inject_and_detect.sh     (optional) Inject live anomaly + detect in one shot
        ↓
08_check_stream_exists.sh   Confirm _anomalies stream exists + record counts
```

---

## Scripts

### `config.sh`
Shared variables: `BASE`, `ORG`, `AUTH`, `STREAM`. Edit here to change endpoint or org.

### `01_create_configs.sh`
Creates all 6 configs and saves their IDs to `config_ids.env`.

| Config | Query mode | Detection |
|--------|-----------|-----------|
| K8s CPU Spike | custom_sql | AVG(cpu_millicores) |
| K8s Memory Pressure | custom_sql | AVG(memory_mb) |
| K8s Error Rate Spike | filters (log_level=ERROR) | COUNT(*) |
| K8s Pod Restarts | custom_sql | SUM(restarts) |
| K8s Latency Spike | custom_sql | AVG(response_time_ms) |
| K8s Auth Failure Spike | filters (message contains "login error") | COUNT(*) |

### `02_list_configs.sh`
Lists all configs with their ID, status, and `is_trained` flag. Use anytime.

### `03_train_all.sh`
Triggers training for all configs. Training runs async on the server.
```bash
./03_train_all.sh                    # train all
./03_train_all.sh <config_id>        # train one
```

### `04_wait_for_training.sh`
Polls every 10s until all configs show `is_trained: true`. Leave running after `03_train_all.sh`.

### `05_run_detection.sh`
Runs detection against the live data window. Prints anomaly count and details.
```bash
./05_run_detection.sh                # detect on all
./05_run_detection.sh <config_id>    # detect on one
```

### `06_query_anomalies_stream.sh`
Queries the `_anomalies` stream directly via the search API.
```bash
./06_query_anomalies_stream.sh                # last 50 anomalies (all configs)
./06_query_anomalies_stream.sh <config_id>    # filter by config
./06_query_anomalies_stream.sh "" 200         # last 200 anomalies
```

### `07_inject_and_detect.sh`
All-in-one: starts the live k8s_data_gen with anomaly injection for 3 minutes,
then runs detection and shows results. Requires trained models.
```bash
./07_inject_and_detect.sh cpu
./07_inject_and_detect.sh memory
./07_inject_and_detect.sh errors
./07_inject_and_detect.sh restarts
./07_inject_and_detect.sh latency
./07_inject_and_detect.sh login
```

### `08_check_stream_exists.sh`
Confirms the `_anomalies` stream was created and shows per-config record counts.

### `09_delete_all_configs.sh`
Deletes all 6 configs and removes `config_ids.env`. Use to reset.

---

## About alerts

Alerts are set to `alert_enabled: false` in these scripts — the focus is on testing
detection mechanics. To enable alerts:

1. Create an alert destination in OpenObserve (Settings → Alert Destinations)
2. Update a config: `PUT /api/{org}/anomaly_detection/{config_id}` with
   `{"alert_enabled": true, "alert_destination_id": "<dest_id>"}`
3. Anomalies written to `_anomalies` will then also fire alert notifications

---

## Typical full run

```bash
cd /Users/lin/o2/local_db/anomaly_test

chmod +x *.sh

./01_create_configs.sh      # create configs, saves IDs
./02_list_configs.sh        # verify they exist

./03_train_all.sh           # kick off training
./04_wait_for_training.sh   # wait... (may take a minute with 10 days of data)

./05_run_detection.sh       # should show 0 anomalies on normal data

# Now inject an anomaly in another terminal:
#   cd ../k8s_data_gen && cargo run -- live --anomaly cpu

./05_run_detection.sh       # run again — should see CPU anomalies

./06_query_anomalies_stream.sh            # view _anomalies stream
./08_check_stream_exists.sh               # confirm stream + counts
```

## Why filter-based configs need baseline noise in training data

The `errors` and `login` configs use `query_mode: filters`. They run a `COUNT(*)` on matching
records aggregated into 1-minute histogram buckets.

The RCF model learns "what is normal" from training data. For this to work, the training
histogram query must return **non-zero buckets** — the search engine only returns rows where
data exists, not zero-count rows. So if training data has zero matching records:

- `log_level = ERROR` → normal data has ~3% ERROR rate → already returns non-zero buckets ✓
- `message LIKE '%login error%'` → **historical data must include background login errors** ✓

The `k8s_data_gen historical` mode generates ~0.5% of records with "login error" messages as
background noise (≈3/min). This gives RCF enough baseline to learn "0–5/min is normal" and
then flag "600/min" during anomaly injection as anomalous.

If you regenerate training data, the new `historical` output already includes this noise.
If you have old data (generated before this was added), regenerate:

```bash
cd ../k8s_data_gen
cargo run -- historical --days 10
cargo run -- ingest
```

## TESTING

### Clean slate (if you have leftover configs from previous runs)

```bash
cd /Users/lin/o2/local_db/anomaly_test

# Delete ALL configs in the org (handles duplicate-run cleanup)
./09_delete_all_configs.sh --all
```

### Full flow

```bash
cd /Users/lin/o2/local_db/anomaly_test
chmod +x *.sh

# Step 1 — create all configs (only need to do once)
./01_create_configs.sh

# Step 2 — train the login model specifically (or train all: ./03_train_all.sh)
source config_ids.env          # bash
# source config_ids.fish       # fish
./03_train_all.sh $LOGIN_ID

source config_ids.fish
  ./03_train_all.sh $LOGIN_ID

# Step 3 — wait for it to train
./04_wait_for_training.sh

source config_ids.fish
./05_run_detection.sh $LOGIN_ID

./06_query_anomalies_stream.sh $LOGIN_ID

# Step 4 — inject + detect in one shot
./07_inject_and_detect.sh login
```

### Manual step-by-step

```bash
# Terminal 1 — start injecting
cd ../k8s_data_gen
cargo run -- live --anomaly login

# Terminal 2 — after ~2 minutes, run detection
source /Users/lin/o2/local_db/anomaly_test/config_ids.fish
./05_run_detection.sh $LOGIN_ID

# Then check the _anomalies stream
./06_query_anomalies_stream.sh $LOGIN_ID
```
