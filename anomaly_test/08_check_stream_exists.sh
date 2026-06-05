#!/usr/bin/env bash
# ── Step 8: Check if _anomalies stream exists and has data ────────────────────
#
# Queries the streams list API to confirm _anomalies was auto-created,
# then does a quick count query to show record count.
#
# Usage:
#   ./08_check_stream_exists.sh

set -euo pipefail
source "$(dirname "$0")/config.sh"

echo "=== Checking _anomalies stream ==="
echo

# 1. List streams — look for _anomalies
echo "--- Streams list ---"
curl -s "$BASE/api/$ORG/streams?type=logs" \
    $AUTH \
    | python3 -c "
import sys, json
d = json.load(sys.stdin)
streams = d.get('list', d.get('streams', d if isinstance(d, list) else []))
names = [s.get('name','') if isinstance(s,dict) else str(s) for s in streams]
if '_anomalies' in names:
    print('  _anomalies stream EXISTS')
else:
    print('  _anomalies stream NOT FOUND')
    print(f'  (found {len(names)} other stream(s): {names[:10]})')
" 2>/dev/null
echo

# 2. Count records
echo "--- Record count ---"
NOW_US=$(python3 -c "import time; print(int(time.time() * 1_000_000))")
START_US=$(python3 -c "import time; print(int((time.time() - 30*86400) * 1_000_000))")

curl -s -X POST "$BASE/api/$ORG/_search?type=logs" \
    $AUTH \
    -H "Content-Type: application/json" \
    -d "{
        \"query\": {
            \"sql\": \"SELECT COUNT(*) AS total FROM _anomalies\",
            \"from\": 0,
            \"size\": 1,
            \"start_time\": $START_US,
            \"end_time\": $NOW_US
        }
    }" \
    | python3 -c "
import sys, json
d = json.load(sys.stdin)
hits = d.get('hits', [])
if hits:
    print(f'  Total anomaly records: {hits[0].get(\"total\", \"?\")}')
else:
    print('  Could not count records (stream may be empty)')
" 2>/dev/null
echo

# 3. Per-config breakdown
echo "--- Anomalies per config ---"
curl -s -X POST "$BASE/api/$ORG/_search?type=logs" \
    $AUTH \
    -H "Content-Type: application/json" \
    -d "{
        \"query\": {
            \"sql\": \"SELECT config_id, COUNT(*) AS cnt FROM _anomalies GROUP BY config_id ORDER BY cnt DESC\",
            \"from\": 0,
            \"size\": 20,
            \"start_time\": $START_US,
            \"end_time\": $NOW_US
        }
    }" \
    | python3 -c "
import sys, json
d = json.load(sys.stdin)
hits = d.get('hits', [])
if not hits:
    print('  No data yet')
else:
    for h in hits:
        print(f\"  {h.get('config_id',''):40s}  count={h.get('cnt','?')}\")
" 2>/dev/null
