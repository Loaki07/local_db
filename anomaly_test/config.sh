#!/usr/bin/env bash
# ── Shared config for all anomaly detection test scripts ──────────────────────

BASE="http://localhost:5080"
ORG="default"
AUTH="-u root@example.com:Complexpass#123"
STREAM="k8s_logs"

# Config IDs file — scripts write here after creating configs so later scripts
# can reference them without you having to copy/paste IDs manually.
CONFIG_IDS_FILE="$(dirname "$0")/config_ids.env"
