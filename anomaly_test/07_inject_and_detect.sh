#!/usr/bin/env bash
# ── Step 7: Inject a live anomaly and watch detection ─────────────────────────
#
# Starts k8s_data_gen in live+anomaly mode for 3 minutes, then waits for
# OpenObserve to flush the data, runs detection, and shows what was caught.
# Best run after models are trained.
#
# Usage:
#   ./07_inject_and_detect.sh cpu      # inject CPU spike
#   ./07_inject_and_detect.sh memory
#   ./07_inject_and_detect.sh errors
#   ./07_inject_and_detect.sh restarts
#   ./07_inject_and_detect.sh latency
#   ./07_inject_and_detect.sh login

set -euo pipefail
source "$(dirname "$0")/config.sh"

ANOMALY_TYPE="${1:-cpu}"
K8S_GEN_DIR="$(dirname "$0")/../k8s_data_gen"

# Map anomaly type to config ID
if [[ ! -f "$CONFIG_IDS_FILE" ]]; then
    echo "Error: $CONFIG_IDS_FILE not found. Run 01_create_configs.sh first."
    exit 1
fi
source "$CONFIG_IDS_FILE"

case "$ANOMALY_TYPE" in
    cpu)      CONFIG_ID="${CPU_ID:-}"   ; CONFIG_NAME="K8s CPU Spike" ;;
    memory)   CONFIG_ID="${MEM_ID:-}"   ; CONFIG_NAME="K8s Memory Pressure" ;;
    errors)   CONFIG_ID="${ERR_ID:-}"   ; CONFIG_NAME="K8s Error Rate Spike" ;;
    restarts) CONFIG_ID="${RST_ID:-}"   ; CONFIG_NAME="K8s Pod Restarts" ;;
    latency)  CONFIG_ID="${LAT_ID:-}"   ; CONFIG_NAME="K8s Latency Spike" ;;
    login)    CONFIG_ID="${LOGIN_ID:-}" ; CONFIG_NAME="K8s Auth Failure Spike" ;;
    *)
        echo "Unknown anomaly type: $ANOMALY_TYPE"
        echo "Valid: cpu memory errors restarts latency login"
        exit 1
        ;;
esac

if [[ -z "$CONFIG_ID" ]]; then
    echo "Error: no config ID for '$ANOMALY_TYPE'. Check $CONFIG_IDS_FILE"
    exit 1
fi

INJECT_SECONDS=180  # 3 minutes of anomaly data
FLUSH_WAIT=30       # seconds to wait for OpenObserve to flush WAL → parquet

GEN_PID=""

# Trap Ctrl+C and kill the generator cleanly
cleanup() {
    echo
    echo "Interrupted — stopping data generator..."
    if [[ -n "$GEN_PID" ]]; then
        kill "$GEN_PID" 2>/dev/null || true
        wait "$GEN_PID" 2>/dev/null || true
    fi
    exit 1
}
trap cleanup INT TERM

echo "=== Anomaly injection test: $ANOMALY_TYPE ==="
echo "  Config:  $CONFIG_NAME ($CONFIG_ID)"
echo "  Inject:  ${INJECT_SECONDS}s of live data with --anomaly $ANOMALY_TYPE"
echo "  Flush:   ${FLUSH_WAIT}s wait after injection for OpenObserve to flush"
echo

echo "Starting k8s_data_gen live --anomaly $ANOMALY_TYPE for ${INJECT_SECONDS}s..."
(cd "$K8S_GEN_DIR" && cargo run --release -- live --anomaly "$ANOMALY_TYPE") &
GEN_PID=$!

sleep "$INJECT_SECONDS"

echo
echo "Stopping data generator..."
kill "$GEN_PID" 2>/dev/null || true
wait "$GEN_PID" 2>/dev/null || true
GEN_PID=""

# Wait for OpenObserve to flush ingested WAL data to parquet so the search
# query can find it. Without this, detection runs before the data is visible.
echo
echo "Waiting ${FLUSH_WAIT}s for OpenObserve to flush data..."
sleep "$FLUSH_WAIT"

echo
echo "Running detection on $CONFIG_NAME..."
resp=$(curl -s -X POST "$BASE/api/$ORG/anomaly_detection/$CONFIG_ID/detect" \
    $AUTH \
    -H "Content-Type: application/json")

echo "$resp" | python3 -c "
import sys, json
d = json.load(sys.stdin)
found = d.get('anomalies_found', d.get('anomaly_count', 0))
scored = d.get('points_scored', d.get('data_points_processed', 0))
print(f'Points scored   : {scored}')
print(f'Anomalies found : {found}')
anomalies = d.get('anomalies', [])
if anomalies:
    print()
    for a in anomalies:
        print(f\"  ts={a.get('timestamp','')}  severity={a.get('severity','')}  score={a.get('score',0):.2f}  actual={a.get('actual_value',0):.2f}  expected={a.get('expected_value',0):.2f}  dev={a.get('deviation_percent',0):.1f}%\")
" 2>/dev/null || echo "$resp"

echo
echo "Check _anomalies stream:"
echo "  ./06_query_anomalies_stream.sh $CONFIG_ID"
