#!/usr/bin/env bash
# ── Step 6: Query the _anomalies stream directly ──────────────────────────────
#
# After detection runs write anomalies, they land in the _anomalies stream.
# This script queries that stream via the standard search API.
#
# Usage:
#   ./06_query_anomalies_stream.sh              # last 50 anomalies
#   ./06_query_anomalies_stream.sh <config_id>  # filter by config
#   ./06_query_anomalies_stream.sh "" 100       # last 100 anomalies

set -euo pipefail
source "$(dirname "$0")/config.sh"

CONFIG_FILTER="${1:-}"
LIMIT="${2:-50}"

if [[ -n "$CONFIG_FILTER" ]]; then
    echo "=== _anomalies stream — config: $CONFIG_FILTER (last $LIMIT) ==="
else
    echo "=== _anomalies stream — all configs (last $LIMIT) ==="
fi
echo

# Use a 30-day window to catch everything
NOW_US=$(python3 -c "import time; print(int(time.time() * 1_000_000))")
START_US=$(python3 -c "import time; print(int((time.time() - 30*86400) * 1_000_000))")

# Build JSON body in Python to avoid shell quoting issues with config IDs
BODY=$(python3 -c "
import json, sys
config_filter = sys.argv[1]
limit = int(sys.argv[2])
start_us = int(sys.argv[3])
end_us = int(sys.argv[4])

if config_filter:
    sql = f'SELECT * FROM _anomalies WHERE config_id = \"{config_filter}\" ORDER BY _timestamp DESC LIMIT {limit}'
else:
    sql = f'SELECT * FROM _anomalies ORDER BY _timestamp DESC LIMIT {limit}'

print(json.dumps({
    'query': {
        'sql': sql,
        'from': 0,
        'size': limit,
        'start_time': start_us,
        'end_time': end_us
    }
}))
" "$CONFIG_FILTER" "$LIMIT" "$START_US" "$NOW_US")

resp=$(curl -s -X POST "$BASE/api/$ORG/_search?type=logs" \
    $AUTH \
    -H "Content-Type: application/json" \
    -d "$BODY")

echo "$resp" | python3 -c "
import sys, json
d = json.load(sys.stdin)
hits = d.get('hits', [])
total = d.get('total', len(hits))
print(f'Total records in _anomalies: {total}')
print()
if not hits:
    print('No anomalies found.')
    print()
    print('This means either:')
    print('  - Detection has not been run yet  (run 05_run_detection.sh)')
    print('  - No anomalies were found in the data window')
    print('  - The _anomalies stream has not been created yet')
    sys.exit(0)
for h in hits:
    print(f\"  config_id  : {h.get('config_id','')}\")
    print(f\"  timestamp  : {h.get('timestamp', h.get('_timestamp',''))}\")
    print(f\"  severity   : {h.get('severity','')}\")
    print(f\"  score      : {h.get('score','')}\")
    print(f\"  actual     : {h.get('actual_value','')}\")
    print(f\"  expected   : {h.get('expected_value','')}\")
    print(f\"  deviation  : {h.get('deviation_percent','')}%\")
    print()
" 2>/dev/null || echo "$resp"
