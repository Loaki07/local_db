#!/usr/bin/env bash
# ── Step 1: Create all 6 anomaly detection configs ────────────────────────────
#
# Run this once after ingesting training data.
# Config IDs are saved to config_ids.env for use by later scripts.
#
# Usage:
#   ./01_create_configs.sh

set -euo pipefail
source "$(dirname "$0")/config.sh"

post_config() {
    local name="$1"
    local body="$2"
    echo "Creating: $name" >&2
    local resp
    resp=$(curl -s -X POST "$BASE/api/$ORG/anomaly_detection" \
        $AUTH \
        -H "Content-Type: application/json" \
        -d "$body")
    # print full response to stderr for visibility
    echo "$resp" | python3 -m json.tool 2>/dev/null >&2 || echo "$resp" >&2
    echo >&2
    # only emit the config_id to stdout so $() captures just the ID
    echo "$resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('config_id',''))" 2>/dev/null
}

# ── CPU spike ─────────────────────────────────────────────────────────────────
CPU_ID=$(post_config "K8s CPU Spike" '{
  "name": "K8s CPU Spike",
  "stream_name": "k8s_logs",
  "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '\''1 minute'\'') AS time_bucket, AVG(cpu_millicores) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "avg",
  "detection_interval": "5m",
  "training_window_days": 7,
  "sensitivity": 6,
  "alert_enabled": false,
  "enabled": true
}')

# ── Memory pressure ───────────────────────────────────────────────────────────
MEM_ID=$(post_config "K8s Memory Pressure" '{
  "name": "K8s Memory Pressure",
  "stream_name": "k8s_logs",
  "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '\''1 minute'\'') AS time_bucket, AVG(memory_mb) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "avg",
  "detection_interval": "5m",
  "training_window_days": 7,
  "sensitivity": 6,
  "alert_enabled": false,
  "enabled": true
}')

# ── Error rate spike ──────────────────────────────────────────────────────────
ERR_ID=$(post_config "K8s Error Rate Spike" '{
  "name": "K8s Error Rate Spike",
  "stream_name": "k8s_logs",
  "stream_type": "logs",
  "query_mode": "filters",
  "filters": [{"field": "log_level", "operator": "equals", "value": "ERROR"}],
  "detection_function": "count",
  "detection_interval": "5m",
  "training_window_days": 7,
  "sensitivity": 7,
  "alert_enabled": false,
  "enabled": true
}')

# ── Pod restarts ──────────────────────────────────────────────────────────────
RST_ID=$(post_config "K8s Pod Restarts" '{
  "name": "K8s Pod Restarts",
  "stream_name": "k8s_logs",
  "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '\''1 minute'\'') AS time_bucket, SUM(restarts) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "sum",
  "detection_interval": "5m",
  "training_window_days": 7,
  "sensitivity": 7,
  "alert_enabled": false,
  "enabled": true
}')

# ── Latency spike ─────────────────────────────────────────────────────────────
LAT_ID=$(post_config "K8s Latency Spike" '{
  "name": "K8s Latency Spike",
  "stream_name": "k8s_logs",
  "stream_type": "logs",
  "query_mode": "custom_sql",
  "custom_sql": "SELECT histogram(_timestamp, '\''1 minute'\'') AS time_bucket, AVG(response_time_ms) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket",
  "detection_function": "avg",
  "detection_interval": "5m",
  "training_window_days": 7,
  "sensitivity": 6,
  "alert_enabled": false,
  "enabled": true
}')

# ── Login / auth failure spike ────────────────────────────────────────────────
LOGIN_ID=$(post_config "K8s Auth Failure Spike" '{
  "name": "K8s Auth Failure Spike",
  "stream_name": "k8s_logs",
  "stream_type": "logs",
  "query_mode": "filters",
  "filters": [{"field": "message", "operator": "contains", "value": "login error"}],
  "detection_function": "count",
  "detection_interval": "5m",
  "training_window_days": 7,
  "sensitivity": 7,
  "alert_enabled": false,
  "enabled": true
}')

# ── Save IDs ──────────────────────────────────────────────────────────────────

# bash-compatible (used by the other scripts internally)
cat > "$CONFIG_IDS_FILE" <<EOF
CPU_ID=$CPU_ID
MEM_ID=$MEM_ID
ERR_ID=$ERR_ID
RST_ID=$RST_ID
LAT_ID=$LAT_ID
LOGIN_ID=$LOGIN_ID
EOF

# fish-compatible (for your interactive shell session)
FISH_IDS_FILE="$(dirname "$0")/config_ids.fish"
cat > "$FISH_IDS_FILE" <<EOF
set CPU_ID $CPU_ID
set MEM_ID $MEM_ID
set ERR_ID $ERR_ID
set RST_ID $RST_ID
set LAT_ID $LAT_ID
set LOGIN_ID $LOGIN_ID
EOF

echo "Config IDs saved."
echo "  bash : source $CONFIG_IDS_FILE"
echo "  fish : source $FISH_IDS_FILE"
echo
cat "$CONFIG_IDS_FILE"
