#!/usr/bin/env bash
# ── Step 3: Trigger training for all configs ──────────────────────────────────
#
# Training runs in the background on the server. Each call returns immediately
# with status "in_progress". Use 02_list_configs.sh to poll until is_trained=true.
#
# Usage:
#   ./03_train_all.sh              # train all 6 configs
#   ./03_train_all.sh <config_id>  # train a single config

set -euo pipefail
source "$(dirname "$0")/config.sh"

train_one() {
    local config_id="$1"
    local name="$2"
    echo "Training: $name ($config_id)"
    curl -s -X POST "$BASE/api/$ORG/anomaly_detection/$config_id/train" \
        $AUTH \
        -H "Content-Type: application/json" \
        | python3 -m json.tool 2>/dev/null
    echo
}

# Single config mode
if [[ "${1:-}" != "" ]]; then
    train_one "$1" "config $1"
    exit 0
fi

# Load saved IDs
if [[ ! -f "$CONFIG_IDS_FILE" ]]; then
    echo "Error: $CONFIG_IDS_FILE not found. Run 01_create_configs.sh first."
    exit 1
fi
source "$CONFIG_IDS_FILE"

echo "=== Triggering training for all configs ==="
echo

[[ -n "${CPU_ID:-}"   ]] && train_one "$CPU_ID"   "K8s CPU Spike"
[[ -n "${MEM_ID:-}"   ]] && train_one "$MEM_ID"   "K8s Memory Pressure"
[[ -n "${ERR_ID:-}"   ]] && train_one "$ERR_ID"   "K8s Error Rate Spike"
[[ -n "${RST_ID:-}"   ]] && train_one "$RST_ID"   "K8s Pod Restarts"
[[ -n "${LAT_ID:-}"   ]] && train_one "$LAT_ID"   "K8s Latency Spike"
[[ -n "${LOGIN_ID:-}" ]] && train_one "$LOGIN_ID" "K8s Auth Failure Spike"

echo "Training triggered for all configs."
echo "Poll with: ./02_list_configs.sh   (wait for is_trained: true)"
