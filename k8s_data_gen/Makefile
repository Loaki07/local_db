BIN := cargo run --release --

# ── Build ─────────────────────────────────────────────────────────────────────

build:
	cargo build --release

# ── Live — gRPC prod traces (port 5081) ──────────────────────────────────────

live-traces-grpc:
	$(BIN) live --stream traces --grpc

live-traces-grpc-latency:
	$(BIN) live --stream traces --grpc --anomaly latency

live-traces-grpc-errors:
	$(BIN) live --stream traces --grpc --anomaly errors

# ── Live — HTTP traces ────────────────────────────────────────────────────────

live-traces:
	$(BIN) live --stream traces

live-traces-latency:
	$(BIN) live --stream traces --anomaly latency

live-traces-errors:
	$(BIN) live --stream traces --anomaly errors

# ── Live — logs ───────────────────────────────────────────────────────────────

live-logs:
	$(BIN) live --stream logs

live-logs-cpu:
	$(BIN) live --stream logs --anomaly cpu

live-logs-memory:
	$(BIN) live --stream logs --anomaly memory

live-logs-errors:
	$(BIN) live --stream logs --anomaly errors

live-logs-restarts:
	$(BIN) live --stream logs --anomaly restarts

live-logs-latency:
	$(BIN) live --stream logs --anomaly latency

live-logs-login:
	$(BIN) live --stream logs --anomaly login

# ── Live — metrics ────────────────────────────────────────────────────────────

live-metrics:
	$(BIN) live --stream metrics

live-metrics-cpu:
	$(BIN) live --stream metrics --anomaly cpu

live-metrics-memory:
	$(BIN) live --stream metrics --anomaly memory

live-metrics-latency:
	$(BIN) live --stream metrics --anomaly latency

# ── Historical + ingest ───────────────────────────────────────────────────────

historical:
	$(BIN) historical --days 7 --stream all

historical-logs:
	$(BIN) historical --days 7 --stream logs

historical-metrics:
	$(BIN) historical --days 7 --stream metrics

historical-traces:
	$(BIN) historical --days 7 --stream traces

ingest-logs:
	$(BIN) ingest ../output_k8s.json --stream k8s_logs

ingest-metrics:
	$(BIN) ingest ../output_k8s_metrics.json --stream k8s_metrics

ingest-traces:
	$(BIN) ingest ../output_k8s_traces.json --stream k8s_traces

ingest-all: ingest-logs ingest-metrics ingest-traces

# ── Help ──────────────────────────────────────────────────────────────────────

help:
	@echo ""
	@echo "  gRPC prod traces (port 5081):"
	@echo "    make live-traces-grpc              — 13-service topology, continuous"
	@echo "    make live-traces-grpc-latency      — with 15–40x latency spike"
	@echo "    make live-traces-grpc-errors       — with payment/auth error spike"
	@echo ""
	@echo "  HTTP traces (port 5080):"
	@echo "    make live-traces                   — simple K8s spans"
	@echo "    make live-traces-latency"
	@echo "    make live-traces-errors"
	@echo ""
	@echo "  Logs:"
	@echo "    make live-logs[-cpu|-memory|-errors|-restarts|-latency|-login]"
	@echo ""
	@echo "  Metrics:"
	@echo "    make live-metrics[-cpu|-memory|-latency]"
	@echo ""
	@echo "  Historical:"
	@echo "    make historical                    — 7 days all streams → JSON files"
	@echo "    make ingest-all                    — bulk upload all three JSON files"
	@echo ""

.PHONY: build help \
	live-traces-grpc live-traces-grpc-latency live-traces-grpc-errors \
	live-traces live-traces-latency live-traces-errors \
	live-logs live-logs-cpu live-logs-memory live-logs-errors \
	live-logs-restarts live-logs-latency live-logs-login \
	live-metrics live-metrics-cpu live-metrics-memory live-metrics-latency \
	historical historical-logs historical-metrics historical-traces \
	ingest-logs ingest-metrics ingest-traces ingest-all
