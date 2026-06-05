#!/usr/bin/env bash
# ── Step 2: List all configs and check their status ───────────────────────────
#
# Use this anytime to see all configs, their IDs, and training status.
#
# Usage:
#   ./02_list_configs.sh

set -euo pipefail
source "$(dirname "$0")/config.sh"

echo "=== Anomaly detection configs for org: $ORG ==="
echo

curl -s -X GET "$BASE/api/$ORG/anomaly_detection" \
    $AUTH \
    -H "Content-Type: application/json" \
    | python3 -c "
import sys, json
data = json.load(sys.stdin)
configs = data if isinstance(data, list) else data.get('configs', data.get('data', []))
if not configs:
    print('No configs found.')
    sys.exit(0)
print(f'Found {len(configs)} config(s):\n')
for c in configs:
    status    = c.get('status', 'unknown')
    trained   = c.get('is_trained', False)
    print(f\"  [{c.get('config_id','')}]\")
    print(f\"    name:       {c.get('name','')}\")
    print(f\"    status:     {status}\")
    print(f\"    is_trained: {trained}\")
    print(f\"    query_mode: {c.get('query_mode','')}\")
    print(f\"    interval:   {c.get('detection_interval','')}\")
    print()
"
