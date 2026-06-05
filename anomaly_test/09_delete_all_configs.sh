#!/usr/bin/env bash
# ── Cleanup: Delete all anomaly detection configs ─────────────────────────────
#
# Use this to reset and start fresh.
#
# Usage:
#   ./09_delete_all_configs.sh         # delete only configs tracked in config_ids.env
#   ./09_delete_all_configs.sh --all   # delete ALL configs in the org (fixes duplicate runs)

set -euo pipefail
source "$(dirname "$0")/config.sh"

DELETE_ALL=false
if [[ "${1:-}" == "--all" ]]; then
    DELETE_ALL=true
fi

delete_one() {
    local id="$1"
    local name="$2"
    [[ -z "$id" ]] && return
    echo "Deleting: $name ($id)"
    curl -s -X DELETE "$BASE/api/$ORG/anomaly_detection/$id" \
        $AUTH \
        | python3 -m json.tool 2>/dev/null
    echo
}

echo "=== Deleting anomaly detection configs ==="
echo

if [[ "$DELETE_ALL" == "true" ]]; then
    echo "Mode: --all (fetching all configs from API)"
    echo

    # Fetch all config IDs from the list endpoint
    ids=$(curl -s -X GET "$BASE/api/$ORG/anomaly_detection" \
        $AUTH \
        -H "Content-Type: application/json" \
        | python3 -c "
import sys, json
data = json.load(sys.stdin)
configs = data if isinstance(data, list) else data.get('configs', data.get('data', []))
for c in configs:
    cid = c.get('config_id','')
    name = c.get('name','unknown')
    if cid:
        print(f'{cid}\t{name}')
" 2>/dev/null)

    if [[ -z "$ids" ]]; then
        echo "No configs found in org '$ORG'."
    else
        count=0
        while IFS=$'\t' read -r id name; do
            delete_one "$id" "$name"
            ((count++))
        done <<< "$ids"
        echo "Deleted $count config(s)."
    fi
else
    # Default: only delete configs tracked in config_ids.env
    if [[ ! -f "$CONFIG_IDS_FILE" ]]; then
        echo "No $CONFIG_IDS_FILE found, nothing to delete."
        echo "Tip: use --all to delete every config in the org."
        exit 0
    fi
    source "$CONFIG_IDS_FILE"

    delete_one "${CPU_ID:-}"   "K8s CPU Spike"
    delete_one "${MEM_ID:-}"   "K8s Memory Pressure"
    delete_one "${ERR_ID:-}"   "K8s Error Rate Spike"
    delete_one "${RST_ID:-}"   "K8s Pod Restarts"
    delete_one "${LAT_ID:-}"   "K8s Latency Spike"
    delete_one "${LOGIN_ID:-}" "K8s Auth Failure Spike"
fi

rm -f "$CONFIG_IDS_FILE"
rm -f "$(dirname "$0")/config_ids.fish"
echo "Done. config_ids.env and config_ids.fish removed."
