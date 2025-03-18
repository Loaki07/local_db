#!/bin/bash

trap 'echo "Exiting..."; exit 0' SIGINT  # Handle Ctrl+C gracefully

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

monitor_pod() {
    local pod=$1
    local namespace=$2
    
    echo -e "${GREEN}=== Stats for $pod ===${NC}"
    
    # Get FD count
    fd_count=$(kubectl exec -n $namespace $pod -- ls -l /proc/1/fd 2>/dev/null | wc -l)
    echo -e "${BLUE}Open FDs:${NC} $fd_count"
    
    # Get network stats
    echo -e "${BLUE}Network Connections:${NC}"
    kubectl exec -n $namespace $pod -- netstat -ant | awk '
        BEGIN {print "State\t\tCount"}
        NR>2 {
            a[$6]++
        }
        END {
            for (i in a) {
                printf "%-15s\t%d\n", i, a[i]
            }
        }'
        
    # Get connection resets
    resets=$(kubectl exec -n $namespace $pod -- netstat -s | grep -i "connection reset" | awk '{print $1}')
    echo -e "${BLUE}Connection Resets:${NC} $resets"
    
    echo ""
}

while true; do
    clear
    date_time=$(date '+%Y-%m-%d %H:%M:%S')
    echo "Time: $date_time | Press Ctrl+C to exit"
    echo "----------------------------------------"
    
    # main pods with error handling
    for pod in "main-openobserve-router-5fffc6bbcf-knrbh" "main-openobserve-querier-0" "main-openobserve-ingester-0"; do
        monitor_pod "$pod" "main" || echo "Failed to monitor $pod"
    done
    
    sleep 1
done 