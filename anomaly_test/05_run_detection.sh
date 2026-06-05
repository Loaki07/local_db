#!/usr/bin/env bash
# ── Step 5: Run detection against all (or one) trained configs ────────────────
#
# Triggers a detection run and prints the result — anomaly count + any anomalies found.
#
# Usage:
#   ./05_run_detection.sh              # detect on all configs
#   ./05_run_detection.sh <config_id>  # detect on one config

set -euo pipefail
source "$(dirname "$0")/config.sh"

detect_one() {
    local config_id="$1"
    local name="$2"
    echo "─── $name ($config_id)"
    local resp
    resp=$(curl -s -X POST "$BASE/api/$ORG/anomaly_detection/$config_id/detect" \
        $AUTH \
        -H "Content-Type: application/json")

    echo "$resp" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(f'  anomalies_found : {d.get(\"anomalies_found\", d.get(\"anomaly_count\", \"?\"))}')
print(f'  points_scored   : {d.get(\"points_scored\", d.get(\"data_points_processed\", \"?\"))}')
anomalies = d.get('anomalies', [])
if anomalies:
    print(f'  anomalies:')
    for a in anomalies[:10]:  # show first 10
        ts  = a.get('timestamp', '')
        sev = a.get('severity', '')
        sc  = a.get('score', '')
        act = a.get('actual_value', '')
        exp = a.get('expected_value', '')
        dev = a.get('deviation_percent', '')
        print(f'    ts={ts}  severity={sev}  score={sc:.2f}  actual={act:.2f}  expected={exp:.2f}  deviation={dev:.1f}%')
    if len(anomalies) > 10:
        print(f'    ... and {len(anomalies)-10} more')
else:
    print('  (no anomalies detected in this window)')
" 2>/dev/null || echo "$resp"
    echo
}

# Single config mode
if [[ "${1:-}" != "" ]]; then
    detect_one "$1" "config $1"
    exit 0
fi

# Load saved IDs
if [[ ! -f "$CONFIG_IDS_FILE" ]]; then
    echo "Error: $CONFIG_IDS_FILE not found. Run 01_create_configs.sh first."
    exit 1
fi
source "$CONFIG_IDS_FILE"

echo "=== Running detection on all configs ==="
echo

[[ -n "${CPU_ID:-}"   ]] && detect_one "$CPU_ID"   "K8s CPU Spike"
[[ -n "${MEM_ID:-}"   ]] && detect_one "$MEM_ID"   "K8s Memory Pressure"
[[ -n "${ERR_ID:-}"   ]] && detect_one "$ERR_ID"   "K8s Error Rate Spike"
[[ -n "${RST_ID:-}"   ]] && detect_one "$RST_ID"   "K8s Pod Restarts"
[[ -n "${LAT_ID:-}"   ]] && detect_one "$LAT_ID"   "K8s Latency Spike"
[[ -n "${LOGIN_ID:-}" ]] && detect_one "$LOGIN_ID" "K8s Auth Failure Spike"
