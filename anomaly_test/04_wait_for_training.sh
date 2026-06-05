#!/usr/bin/env bash
# ── Step 4: Poll until all configs are trained ────────────────────────────────
#
# Polls every 10 seconds and exits when all configs show is_trained=true.
#
# Usage:
#   ./04_wait_for_training.sh

set -euo pipefail
source "$(dirname "$0")/config.sh"

echo "Polling training status every 10s (Ctrl+C to stop)..."
echo

while true; do
    result=$(curl -s -X GET "$BASE/api/$ORG/anomaly_detection" \
        $AUTH \
        -H "Content-Type: application/json" \
        | python3 -c "
import sys, json
data = json.load(sys.stdin)
configs = data if isinstance(data, list) else data.get('configs', data.get('data', []))
total   = len(configs)
trained = sum(1 for c in configs if c.get('is_trained', False))
print(f'{trained}/{total}')
for c in configs:
    mark = '✓' if c.get('is_trained') else '○'
    print(f'  {mark} {c.get(\"name\",\"\"):35s}  status={c.get(\"status\",\"\")}')
" 2>/dev/null)

    progress=$(echo "$result" | head -1)
    echo "$(date '+%H:%M:%S')  trained $progress"
    echo "$result" | tail -n +2

    total=$(echo "$progress" | cut -d/ -f2)
    done=$(echo "$progress"  | cut -d/ -f1)
    if [[ "$done" == "$total" && "$total" -gt 0 ]]; then
        echo
        echo "All $total configs trained!"
        break
    fi

    echo
    sleep 10
done
