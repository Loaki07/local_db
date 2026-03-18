/// K8s Data Generator
///
/// Generates realistic Kubernetes observability data (logs, metrics, traces) for OpenObserve.
///
/// USAGE:
///   cargo run -- historical [--days N] [--stream logs|metrics|traces|all]
///   cargo run -- ingest [FILE] [--org ORG] [--stream STREAM]
///   cargo run -- live [--stream logs|metrics|traces] [--anomaly TYPE]
///
/// STREAMS:
///   logs    → k8s_logs    (default)
///   metrics → k8s_metrics
///   traces  → k8s_traces
///   all     → generates all three (historical only)
///
/// ANOMALY TYPES (work across all streams):
///   cpu        CPU millicores / cpu_percent spike (4-7x normal)
///   memory     Memory MB / memory_percent spike (3.5-5x normal)
///   errors     Error log/span rate spike (30-80%)
///   restarts   Pod restart count spike (5-15 restarts) [logs only]
///   latency    Response time / span duration spike (15-40x normal)
///   login      Auth failure spike — floods message with "login error" [logs only]
///
/// ANOMALY DETECTION CONFIGS — logs (k8s_logs):
///   CPU:      custom_sql = "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(cpu_millicores) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time"
///   Memory:   custom_sql = "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(memory_mb) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time"
///   Errors:   query_mode=filters, filter: log_level=ERROR, detection_function=count(*)
///   Restarts: custom_sql = "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, SUM(restarts) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time"
///   Latency:  custom_sql = "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(response_time_ms) AS zo_sql_val FROM \"k8s_logs\" GROUP BY zo_sql_time ORDER BY zo_sql_time"
///   Login:    query_mode=filters, filter: message contains "login error", detection_function=count(*)
///
/// ANOMALY DETECTION CONFIGS — metrics (OTLP → stream_type=metrics, one stream per field):
///   CPU:      stream=cpu_percent,        type=metrics, detection_function=avg(value)
///   Memory:   stream=memory_percent,     type=metrics, detection_function=avg(value)
///   Latency:  stream=request_latency_ms, type=metrics, detection_function=avg(value)
///   Errors:   stream=error_rate,         type=metrics, detection_function=avg(value)
///             (optionally filter: service_name=payments-api)
///
/// ANOMALY DETECTION CONFIGS — traces (OTLP → k8s_traces, stream_type=traces):
///   Latency:  stream=k8s_traces, type=traces, detection_function=avg(duration_ms)
///   Errors:   stream=k8s_traces, type=traces, filter: status=ERROR, detection_function=count(*)
///   Svc P99:  custom_sql = "SELECT histogram(_timestamp,'5 minute') AS zo_sql_time, AVG(duration_ms) AS zo_sql_val FROM \"k8s_traces\" WHERE service_name='payments-api' GROUP BY zo_sql_time ORDER BY zo_sql_time"
use chrono::Utc;
use rand::{seq::SliceRandom, Rng};
use reqwest::Client;
use serde::Serialize;
use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    time::Duration,
};
use uuid::Uuid;

// ── Config ────────────────────────────────────────────────────────────────────

const API_BASE: &str = "http://localhost:5080";
// const API_BASE: &str = "https://dev2.internal.zinclabs.dev";
const DEFAULT_ORG: &str = "default";
const DEFAULT_STREAM_LOGS: &str = "k8s_logs";
const DEFAULT_STREAM_METRICS: &str = "k8s_metrics";
const DEFAULT_STREAM_TRACES: &str = "k8s_traces";
const USERNAME: &str = "root@example.com";
const PASSWORD: &str = "Complexpass#123";

/// Seconds between records per pod in historical mode
const INTERVAL_SECONDS: i64 = 10;

/// Pod records sent per second in live mode
const PODS_PER_TICK: usize = 10;

/// Chunk size for historical JSON writing / progress reporting
const CHUNK_SIZE: usize = 5_000;

// ── K8s topology ─────────────────────────────────────────────────────────────

struct Pod {
    namespace: &'static str,
    service: &'static str,
    container: &'static str,
    base_cpu: u32,   // millicores
    base_mem: u32,   // MB
    base_rps: u32,
    base_rt: f64,    // ms
    base_err: f64,   // fraction
}

const PODS: &[Pod] = &[
    Pod { namespace: "payments",   service: "payments-api",      container: "api",        base_cpu: 350, base_mem: 512, base_rps: 220,  base_rt: 45.0,  base_err: 0.005 },
    Pod { namespace: "payments",   service: "payments-worker",   container: "worker",     base_cpu: 180, base_mem: 256, base_rps: 80,   base_rt: 30.0,  base_err: 0.002 },
    Pod { namespace: "inventory",  service: "inventory-service", container: "service",    base_cpu: 280, base_mem: 384, base_rps: 150,  base_rt: 60.0,  base_err: 0.008 },
    Pod { namespace: "inventory",  service: "inventory-db",      container: "postgres",   base_cpu: 420, base_mem: 768, base_rps: 50,   base_rt: 12.0,  base_err: 0.001 },
    Pod { namespace: "frontend",   service: "web-server",        container: "nginx",      base_cpu: 120, base_mem: 128, base_rps: 800,  base_rt: 8.0,   base_err: 0.003 },
    Pod { namespace: "frontend",   service: "static-cdn",        container: "cdn",        base_cpu: 90,  base_mem: 96,  base_rps: 600,  base_rt: 5.0,   base_err: 0.001 },
    Pod { namespace: "monitoring", service: "prometheus",        container: "prometheus", base_cpu: 460, base_mem: 900, base_rps: 20,   base_rt: 25.0,  base_err: 0.000 },
    Pod { namespace: "monitoring", service: "grafana",           container: "grafana",    base_cpu: 200, base_mem: 320, base_rps: 40,   base_rt: 120.0, base_err: 0.002 },
    Pod { namespace: "infra",      service: "nginx-ingress",     container: "controller", base_cpu: 310, base_mem: 256, base_rps: 1200, base_rt: 3.0,   base_err: 0.004 },
    Pod { namespace: "infra",      service: "coredns",           container: "coredns",    base_cpu: 150, base_mem: 192, base_rps: 400,  base_rt: 2.0,   base_err: 0.000 },
];

const CLUSTERS: &[&str] = &["prod-us-east-1", "prod-eu-west-1", "staging-us-west-2"];
const NODES: &[&str] = &["node-1", "node-2", "node-3", "node-4", "node-5"];

// Trace operations per service
const TRACE_OPS: &[(&str, &[&str])] = &[
    ("payments-api",      &["POST /checkout", "GET /payment-methods", "POST /refund", "GET /balance"]),
    ("payments-worker",   &["process_payment", "reconcile_batch", "send_notification"]),
    ("inventory-service", &["GET /products", "GET /stock", "PUT /reserve", "POST /restock"]),
    ("inventory-db",      &["SELECT products", "UPDATE stock", "INSERT order_item"]),
    ("web-server",        &["GET /", "GET /products", "GET /cart", "POST /checkout"]),
    ("static-cdn",        &["GET /static/js", "GET /static/css", "GET /images"]),
    ("prometheus",        &["scrape_metrics", "evaluate_rules", "query_range"]),
    ("grafana",           &["dashboard_load", "panel_query", "alert_evaluate"]),
    ("nginx-ingress",     &["ROUTE /api", "ROUTE /static", "TLS_HANDSHAKE"]),
    ("coredns",           &["resolve_internal", "resolve_external", "cache_hit"]),
];

// ── Log-level / message constants ─────────────────────────────────────────────

const LOG_LEVELS_NORMAL: &[(&str, u32)] = &[("DEBUG", 10), ("INFO", 75), ("WARN", 12), ("ERROR", 3)];
const LOG_LEVELS_ERROR:  &[(&str, u32)] = &[("DEBUG", 2),  ("INFO", 10), ("WARN", 20), ("ERROR", 68)];

const EVENT_TYPES: &[&str] = &[
    "request", "request", "request", "request", "request", "request", "request",
    "healthcheck", "pod_lifecycle",
    "login",
];

const MESSAGES_INFO:  &[&str] = &["Processed request successfully", "Cache hit for key", "Health check passed", "Database query completed", "gRPC call returned OK", "Scheduled job ran", "Config reload triggered"];
const MESSAGES_WARN:  &[&str] = &["Slow query detected, took >500ms", "Retry attempt 1 of 3", "Connection pool near capacity", "Rate limit threshold approaching", "Certificate expiring in 14 days"];
const MESSAGES_ERROR: &[&str] = &["Upstream service returned 503", "Database connection timeout", "Failed to parse response body", "OOMKilled: container exceeded memory limit", "Panic: index out of bounds", "TLS handshake failed", "Request queue overflow, dropping request"];
const MESSAGES_LOGIN_OK:    &[&str] = &["User login successful", "Service account authenticated", "OAuth token validated", "API key auth successful"];
const MESSAGES_LOGIN_ERROR: &[&str] = &["login error: invalid credentials for user", "login error: account locked after repeated failures", "login error: token expired or revoked", "login error: IP blocked after too many attempts", "login error: MFA verification failed", "login error: service account rejected", "login error: brute force attempt detected"];

/// ~0.5% of live records → ~3 login errors/min background noise for RCF training
const LOGIN_ERROR_BACKGROUND_PROB: f64 = 0.005;

/// 60% during historical generation so every 10s bucket has non-zero login error count
const HISTORICAL_LOGIN_ERROR_PROB: f64 = 0.60;

// ── Record types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct K8sLogRecord {
    _timestamp: i64,
    cluster: String,
    namespace: String,
    pod: String,
    container: String,
    node: String,
    service: String,
    cpu_millicores: u32,
    memory_mb: u32,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
    restarts: u32,
    response_time_ms: f64,
    error_rate: f64,
    requests_per_second: u32,
    log_level: String,
    event_type: String,
    status_code: u16,
    message: String,
    unique_id: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
struct K8sMetricRecord {
    _timestamp: i64,
    cluster: String,
    namespace: String,
    pod: String,
    node: String,
    service: String,
    /// CPU in millicores
    cpu_millicores: u32,
    /// CPU as percentage of 1 core (millicores / 10)
    cpu_percent: f64,
    /// Memory in MB
    memory_mb: u32,
    /// Memory as percent of node capacity (4096 MB node)
    memory_percent: f64,
    network_rx_bytes_per_sec: u64,
    network_tx_bytes_per_sec: u64,
    requests_per_second: f64,
    /// Average request latency for this pod in this interval
    request_latency_ms: f64,
    /// Error rate 0.0–1.0
    error_rate: f64,
    /// Pod restarts in this interval
    restarts: u32,
}

#[derive(Debug, Serialize, serde::Deserialize)]
struct K8sTraceRecord {
    _timestamp: i64,
    trace_id: String,
    span_id: String,
    /// Empty string = root span
    parent_span_id: String,
    service_name: String,
    namespace: String,
    cluster: String,
    operation_name: String,
    /// Span duration in microseconds
    duration_us: i64,
    /// Span duration in milliseconds (float, for anomaly detection)
    duration_ms: f64,
    /// "OK" or "ERROR"
    status: String,
    http_status_code: u16,
    /// true = this is the root/entry span of the trace
    is_root: bool,
}

// ── Anomaly types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum AnomalyType {
    Cpu,
    Memory,
    Errors,
    Restarts,
    Latency,
    Login,
}

impl AnomalyType {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "cpu"      => Some(AnomalyType::Cpu),
            "memory"   => Some(AnomalyType::Memory),
            "errors"   => Some(AnomalyType::Errors),
            "restarts" => Some(AnomalyType::Restarts),
            "latency"  => Some(AnomalyType::Latency),
            "login"    => Some(AnomalyType::Login),
            _ => None,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            AnomalyType::Cpu      => "cpu",
            AnomalyType::Memory   => "memory",
            AnomalyType::Errors   => "errors",
            AnomalyType::Restarts => "restarts",
            AnomalyType::Latency  => "latency",
            AnomalyType::Login    => "login",
        }
    }
}

struct AnomalyState {
    anomaly_type: AnomalyType,
    remaining_secs: u32,
    cooldown_secs: u32,
}

impl AnomalyState {
    fn new(anomaly_type: AnomalyType) -> Self {
        AnomalyState { anomaly_type, remaining_secs: 0, cooldown_secs: 30 }
    }

    fn tick(&mut self, rng: &mut impl Rng) {
        if self.remaining_secs > 0 {
            self.remaining_secs -= 1;
            return;
        }
        if self.cooldown_secs > 0 {
            self.cooldown_secs -= 1;
            return;
        }
        if rng.gen_bool(0.10) {
            self.remaining_secs = rng.gen_range(120..=300);
            self.cooldown_secs = 120;
            println!("[ANOMALY] {} spike started — {}s", self.anomaly_type.label(), self.remaining_secs);
        }
    }

    fn is_active(&self) -> bool {
        self.remaining_secs > 0
    }
}

// ── Record generation ─────────────────────────────────────────────────────────

fn weighted_choice<'a>(choices: &[(&'a str, u32)], rng: &mut impl Rng) -> &'a str {
    let total: u32 = choices.iter().map(|(_, w)| w).sum();
    let mut pick = rng.gen_range(0..total);
    for (item, weight) in choices {
        if pick < *weight { return item; }
        pick -= weight;
    }
    choices.last().unwrap().0
}

/// Applies daily sinusoidal seasonality: peaks at noon, troughs at 4am.
/// Returns a multiplier in [1.0 - amplitude, 1.0 + amplitude].
fn daily_seasonal(timestamp_us: i64, amplitude: f64) -> f64 {
    use std::f64::consts::PI;
    let secs = timestamp_us / 1_000_000;
    let hour_f = (secs % 86400) as f64 / 3600.0;
    // Peak at hour 12 (noon), trough at hour 0/24
    1.0 + amplitude * (2.0 * PI * (hour_f - 6.0) / 24.0).sin()
}

fn pod_name(pod_idx: usize) -> String {
    format!("{}-{:06x}", PODS[pod_idx].service, pod_idx * 0x1a3f + 0x4b7)
}

// ── Log record generation ─────────────────────────────────────────────────────

fn generate_log_record(
    pod_idx: usize,
    timestamp_us: i64,
    anomaly: Option<&AnomalyState>,
    login_error_prob: f64,
    rng: &mut impl Rng,
) -> K8sLogRecord {
    let pod = &PODS[pod_idx];
    let cluster = CLUSTERS[rng.gen_range(0..CLUSTERS.len())];
    let node = NODES[pod_idx % NODES.len()];
    let pod_name = pod_name(pod_idx);

    let is_anomaly = anomaly.map(|a| a.is_active()).unwrap_or(false);
    let anomaly_type = anomaly.map(|a| &a.anomaly_type);

    let season = daily_seasonal(timestamp_us, 0.20);

    let cpu = if is_anomaly && anomaly_type == Some(&AnomalyType::Cpu) {
        (pod.base_cpu as f64 * rng.gen_range(4.0..7.0)) as u32
    } else {
        (pod.base_cpu as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))) as u32
    };

    let memory = if is_anomaly && anomaly_type == Some(&AnomalyType::Memory) {
        (pod.base_mem as f64 * rng.gen_range(3.5..5.0)) as u32
    } else {
        (pod.base_mem as f64 * (1.0 + rng.gen_range(-0.15_f64..=0.15))) as u32
    };

    let restarts: u32 = if is_anomaly && anomaly_type == Some(&AnomalyType::Restarts) {
        rng.gen_range(5..=15)
    } else if rng.gen_bool(0.002) { 1 } else { 0 };

    let response_time = if is_anomaly && anomaly_type == Some(&AnomalyType::Latency) {
        pod.base_rt * rng.gen_range(15.0..40.0) * (1.0 + rng.gen_range(-0.30_f64..=0.30))
    } else {
        (pod.base_rt * season * (1.0 + rng.gen_range(-0.25_f64..=0.25))).max(1.0)
    };

    let error_rate = if is_anomaly && anomaly_type == Some(&AnomalyType::Errors) {
        rng.gen_range(0.30..0.80_f64)
    } else {
        (pod.base_err * (1.0 + rng.gen_range(-0.50_f64..=0.50))).clamp(0.0, 0.05)
    };

    let rps = (pod.base_rps as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))) as u32;
    let net_rx = rps as u64 * rng.gen_range(800..1200);
    let net_tx = rps as u64 * rng.gen_range(400..800);

    let is_error_anomaly = is_anomaly && anomaly_type == Some(&AnomalyType::Errors);
    let is_login_anomaly = is_anomaly && anomaly_type == Some(&AnomalyType::Login);

    let log_level = weighted_choice(
        if is_error_anomaly { LOG_LEVELS_ERROR } else { LOG_LEVELS_NORMAL },
        rng,
    );

    let (event_type, message) = if is_login_anomaly {
        let msg = MESSAGES_LOGIN_ERROR[rng.gen_range(0..MESSAGES_LOGIN_ERROR.len())].to_string();
        ("login_error", msg)
    } else if rng.gen_bool(login_error_prob) {
        let msg = MESSAGES_LOGIN_ERROR[rng.gen_range(0..MESSAGES_LOGIN_ERROR.len())].to_string();
        ("login_error", msg)
    } else {
        let et = EVENT_TYPES[rng.gen_range(0..EVENT_TYPES.len())];
        let msg = if et == "login" {
            MESSAGES_LOGIN_OK[rng.gen_range(0..MESSAGES_LOGIN_OK.len())].to_string()
        } else {
            match log_level {
                "ERROR" => MESSAGES_ERROR[rng.gen_range(0..MESSAGES_ERROR.len())].to_string(),
                "WARN"  => MESSAGES_WARN[rng.gen_range(0..MESSAGES_WARN.len())].to_string(),
                _       => MESSAGES_INFO[rng.gen_range(0..MESSAGES_INFO.len())].to_string(),
            }
        };
        (et, msg)
    };

    let status_code: u16 = if error_rate > 0.20 {
        *[500u16, 502, 503, 504][..].choose(rng).unwrap()
    } else if error_rate > 0.05 {
        if rng.gen_bool(0.5) { 429 } else { 500 }
    } else {
        *[200u16, 200, 200, 200, 201, 204, 304][..].choose(rng).unwrap()
    };

    K8sLogRecord {
        _timestamp: timestamp_us,
        cluster: cluster.to_string(),
        namespace: pod.namespace.to_string(),
        pod: pod_name,
        container: pod.container.to_string(),
        node: node.to_string(),
        service: pod.service.to_string(),
        cpu_millicores: cpu,
        memory_mb: memory,
        network_rx_bytes: net_rx,
        network_tx_bytes: net_tx,
        restarts,
        response_time_ms: (response_time * 10.0).round() / 10.0,
        error_rate: (error_rate * 1000.0).round() / 1000.0,
        requests_per_second: rps,
        log_level: log_level.to_string(),
        event_type: event_type.to_string(),
        status_code,
        message,
        unique_id: Uuid::new_v4().to_string(),
    }
}

// ── Metrics record generation ─────────────────────────────────────────────────

const NODE_MEMORY_MB: f64 = 4096.0;  // assume 4 GB nodes

fn generate_metric_record(
    pod_idx: usize,
    timestamp_us: i64,
    anomaly: Option<&AnomalyState>,
    rng: &mut impl Rng,
) -> K8sMetricRecord {
    let pod = &PODS[pod_idx];
    let cluster = CLUSTERS[rng.gen_range(0..CLUSTERS.len())];
    let node = NODES[pod_idx % NODES.len()];

    let is_anomaly = anomaly.map(|a| a.is_active()).unwrap_or(false);
    let anomaly_type = anomaly.map(|a| &a.anomaly_type);

    let season = daily_seasonal(timestamp_us, 0.25);

    let cpu_mc = if is_anomaly && anomaly_type == Some(&AnomalyType::Cpu) {
        (pod.base_cpu as f64 * rng.gen_range(4.0..7.0)) as u32
    } else {
        (pod.base_cpu as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))).max(1.0) as u32
    };

    let memory_mb = if is_anomaly && anomaly_type == Some(&AnomalyType::Memory) {
        (pod.base_mem as f64 * rng.gen_range(3.5..5.0)) as u32
    } else {
        (pod.base_mem as f64 * (1.0 + rng.gen_range(-0.15_f64..=0.15))) as u32
    };

    let latency = if is_anomaly && anomaly_type == Some(&AnomalyType::Latency) {
        pod.base_rt * rng.gen_range(15.0..40.0) * (1.0 + rng.gen_range(-0.30_f64..=0.30))
    } else {
        (pod.base_rt * season * (1.0 + rng.gen_range(-0.25_f64..=0.25))).max(0.5)
    };

    let error_rate = if is_anomaly && anomaly_type == Some(&AnomalyType::Errors) {
        rng.gen_range(0.30..0.80_f64)
    } else {
        (pod.base_err * (1.0 + rng.gen_range(-0.50_f64..=0.50))).clamp(0.0, 0.05)
    };

    let restarts: u32 = if is_anomaly && anomaly_type == Some(&AnomalyType::Restarts) {
        rng.gen_range(5..=15)
    } else if rng.gen_bool(0.002) { 1 } else { 0 };

    let rps = (pod.base_rps as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))).max(0.0);
    let net_rx = (rps as u64).saturating_mul(rng.gen_range(800..1200));
    let net_tx = (rps as u64).saturating_mul(rng.gen_range(400..800));

    K8sMetricRecord {
        _timestamp: timestamp_us,
        cluster: cluster.to_string(),
        namespace: pod.namespace.to_string(),
        pod: pod_name(pod_idx),
        node: node.to_string(),
        service: pod.service.to_string(),
        cpu_millicores: cpu_mc,
        cpu_percent: ((cpu_mc as f64 / 10.0) * 100.0).round() / 100.0,
        memory_mb,
        memory_percent: ((memory_mb as f64 / NODE_MEMORY_MB) * 10000.0).round() / 100.0,
        network_rx_bytes_per_sec: net_rx,
        network_tx_bytes_per_sec: net_tx,
        requests_per_second: (rps * 10.0).round() / 10.0,
        request_latency_ms: (latency * 10.0).round() / 10.0,
        error_rate: (error_rate * 10000.0).round() / 10000.0,
        restarts,
    }
}

// ── Trace record generation ───────────────────────────────────────────────────

/// Generates a root span + 1-2 child spans for one synthetic request.
/// Returns all spans; first element is always the root.
fn generate_trace_spans(
    pod_idx: usize,
    timestamp_us: i64,
    anomaly: Option<&AnomalyState>,
    rng: &mut impl Rng,
) -> Vec<K8sTraceRecord> {
    let pod = &PODS[pod_idx];
    let cluster = CLUSTERS[rng.gen_range(0..CLUSTERS.len())];

    let is_anomaly = anomaly.map(|a| a.is_active()).unwrap_or(false);
    let anomaly_type = anomaly.map(|a| &a.anomaly_type);

    let season = daily_seasonal(timestamp_us, 0.20);

    // Pick operation for this pod
    let ops = TRACE_OPS.iter()
        .find(|(svc, _)| *svc == pod.service)
        .map(|(_, ops)| *ops)
        .unwrap_or(&["unknown"]);
    let operation = ops[rng.gen_range(0..ops.len())];

    let base_duration = if is_anomaly && anomaly_type == Some(&AnomalyType::Latency) {
        pod.base_rt * rng.gen_range(15.0..40.0) * (1.0 + rng.gen_range(-0.30_f64..=0.30))
    } else {
        (pod.base_rt * season * (1.0 + rng.gen_range(-0.25_f64..=0.25))).max(0.5)
    };

    let is_error = if is_anomaly && anomaly_type == Some(&AnomalyType::Errors) {
        rng.gen_bool(0.60)
    } else {
        rng.gen_bool(pod.base_err.min(0.05))
    };

    let status = if is_error { "ERROR" } else { "OK" };
    let http_status: u16 = if is_error {
        *[500u16, 502, 503, 504][..].choose(rng).unwrap()
    } else {
        *[200u16, 200, 200, 201, 204][..].choose(rng).unwrap()
    };

    let trace_id = Uuid::new_v4().simple().to_string();
    let root_span_id = Uuid::new_v4().simple().to_string()[..16].to_string();
    let duration_us = (base_duration * 1000.0) as i64;

    let mut spans = vec![K8sTraceRecord {
        _timestamp: timestamp_us,
        trace_id: trace_id.clone(),
        span_id: root_span_id.clone(),
        parent_span_id: String::new(),
        service_name: pod.service.to_string(),
        namespace: pod.namespace.to_string(),
        cluster: cluster.to_string(),
        operation_name: operation.to_string(),
        duration_us,
        duration_ms: (base_duration * 10.0).round() / 10.0,
        status: status.to_string(),
        http_status_code: http_status,
        is_root: true,
    }];

    // Add 1-2 child spans (downstream calls) for some pods
    let has_children = matches!(pod.service, "payments-api" | "web-server" | "inventory-service" | "nginx-ingress");
    if has_children && rng.gen_bool(0.7) {
        let child_count = rng.gen_range(1..=2_usize);
        for _ in 0..child_count {
            let child_duration = base_duration * rng.gen_range(0.3..0.8);
            let child_offset_us = rng.gen_range(0..duration_us / 2);
            let child_span_id = Uuid::new_v4().simple().to_string()[..16].to_string();

            // Child calls a downstream service
            let downstream_idx = rng.gen_range(0..PODS.len());
            let downstream = &PODS[downstream_idx];

            spans.push(K8sTraceRecord {
                _timestamp: timestamp_us + child_offset_us,
                trace_id: trace_id.clone(),
                span_id: child_span_id,
                parent_span_id: root_span_id.clone(),
                service_name: downstream.service.to_string(),
                namespace: downstream.namespace.to_string(),
                cluster: cluster.to_string(),
                operation_name: format!("call_{}", downstream.service.replace('-', "_")),
                duration_us: (child_duration * 1000.0) as i64,
                duration_ms: (child_duration * 10.0).round() / 10.0,
                status: if is_error && rng.gen_bool(0.5) { "ERROR" } else { "OK" }.to_string(),
                http_status_code: if is_error && rng.gen_bool(0.5) { 500 } else { 200 },
                is_root: false,
            });
        }
    }

    spans
}

// ── OTLP conversion helpers ───────────────────────────────────────────────────

/// Convert one K8sMetricRecord into an OTLP ResourceMetrics JSON object.
/// Each numeric field becomes a separate gauge metric; resource attributes carry
/// service/namespace/pod/node/cluster so they are queryable as filter fields.
/// OpenObserve stores each metric name as its own stream (type=metrics):
///   cpu_percent, memory_percent, request_latency_ms, error_rate, …
fn metric_record_to_resource_metrics(r: &K8sMetricRecord) -> serde_json::Value {
    let ts_owned = (r._timestamp * 1000).to_string(); // microseconds → nanoseconds
    let ts = ts_owned.as_str();
    serde_json::json!({
        "resource": {
            "attributes": [
                {"key": "service.name", "value": {"stringValue": r.service.as_str()}},
                {"key": "namespace",    "value": {"stringValue": r.namespace.as_str()}},
                {"key": "pod",          "value": {"stringValue": r.pod.as_str()}},
                {"key": "node",         "value": {"stringValue": r.node.as_str()}},
                {"key": "cluster",      "value": {"stringValue": r.cluster.as_str()}},
            ]
        },
        "scopeMetrics": [{
            "scope": {"name": "k8s-data-gen"},
            "metrics": [
                {"name": "cpu_millicores",           "gauge": {"dataPoints": [{"timeUnixNano": ts, "asInt": r.cpu_millicores.to_string()}]}},
                {"name": "cpu_percent",              "gauge": {"dataPoints": [{"timeUnixNano": ts, "asDouble": r.cpu_percent}]}},
                {"name": "memory_mb",                "gauge": {"dataPoints": [{"timeUnixNano": ts, "asInt": r.memory_mb.to_string()}]}},
                {"name": "memory_percent",           "gauge": {"dataPoints": [{"timeUnixNano": ts, "asDouble": r.memory_percent}]}},
                {"name": "request_latency_ms",       "gauge": {"dataPoints": [{"timeUnixNano": ts, "asDouble": r.request_latency_ms}]}},
                {"name": "error_rate",               "gauge": {"dataPoints": [{"timeUnixNano": ts, "asDouble": r.error_rate}]}},
                {"name": "requests_per_second",      "gauge": {"dataPoints": [{"timeUnixNano": ts, "asDouble": r.requests_per_second}]}},
                {"name": "network_rx_bytes_per_sec", "gauge": {"dataPoints": [{"timeUnixNano": ts, "asInt": r.network_rx_bytes_per_sec.to_string()}]}},
                {"name": "network_tx_bytes_per_sec", "gauge": {"dataPoints": [{"timeUnixNano": ts, "asInt": r.network_tx_bytes_per_sec.to_string()}]}},
                {"name": "restarts",                 "gauge": {"dataPoints": [{"timeUnixNano": ts, "asInt": r.restarts.to_string()}]}},
            ]
        }]
    })
}

fn metrics_to_otlp_payload(records: &[K8sMetricRecord]) -> serde_json::Value {
    serde_json::json!({
        "resourceMetrics": records.iter().map(metric_record_to_resource_metrics).collect::<Vec<_>>()
    })
}

/// Convert one K8sTraceRecord into an OTLP ResourceSpans JSON object.
/// Span attributes carry duration_ms and status (string) so anomaly detection
/// can filter on status=ERROR and aggregate avg(duration_ms).
fn trace_record_to_resource_spans(s: &K8sTraceRecord) -> serde_json::Value {
    let start_owned = (s._timestamp * 1000).to_string();
    let end_owned   = ((s._timestamp + s.duration_us) * 1000).to_string();
    let start_ns    = start_owned.as_str();
    let end_ns      = end_owned.as_str();
    let status_code: u8 = if s.status == "ERROR" { 2 } else { 1 }; // 1=OK, 2=ERROR
    let kind: u8 = if s.is_root { 2 } else { 3 }; // 2=SERVER, 3=CLIENT
    serde_json::json!({
        "resource": {
            "attributes": [
                {"key": "service.name", "value": {"stringValue": s.service_name.as_str()}},
                {"key": "namespace",    "value": {"stringValue": s.namespace.as_str()}},
                {"key": "cluster",      "value": {"stringValue": s.cluster.as_str()}},
            ]
        },
        "scopeSpans": [{
            "scope": {"name": "k8s-data-gen"},
            "spans": [{
                "traceId":      s.trace_id.as_str(),
                "spanId":       s.span_id.as_str(),
                "parentSpanId": s.parent_span_id.as_str(),
                "name":         s.operation_name.as_str(),
                "kind": kind,
                "startTimeUnixNano": start_ns,
                "endTimeUnixNano":   end_ns,
                "status": {"code": status_code},
                "attributes": [
                    {"key": "http.status_code", "value": {"intValue": s.http_status_code.to_string()}},
                    {"key": "duration_ms",      "value": {"doubleValue": s.duration_ms}},
                    {"key": "status",           "value": {"stringValue": s.status.as_str()}},
                ]
            }]
        }]
    })
}

fn traces_to_otlp_payload(spans: &[K8sTraceRecord]) -> serde_json::Value {
    serde_json::json!({
        "resourceSpans": spans.iter().map(trace_record_to_resource_spans).collect::<Vec<_>>()
    })
}

/// POST an OTLP JSON payload (metrics or traces).
/// Pass `stream_name = Some("k8s_traces")` for the traces stream-name header.
async fn post_otlp(
    client: &Client,
    url: &str,
    stream_name: Option<&str>,
    body: &serde_json::Value,
    record_count: usize,
    anomaly_state: &Option<AnomalyState>,
) {
    let anomaly_active    = anomaly_state.as_ref().map(|a| a.is_active()).unwrap_or(false);
    let anomaly_remaining = anomaly_state.as_ref().map(|a| a.remaining_secs).unwrap_or(0);

    let builder = client
        .post(url)
        .basic_auth(USERNAME, Some(PASSWORD))
        .json(body);
    let builder = match stream_name {
        Some(name) => builder.header("stream-name", name),
        None       => builder,
    };

    match builder.send().await {
        Ok(resp) if resp.status().is_success() => {
            let suffix = if anomaly_active {
                format!(" [ANOMALY ACTIVE: {}s remaining]", anomaly_remaining)
            } else {
                String::new()
            };
            println!("[{}] ✓ {} records{}", Utc::now().format("%Y-%m-%d %H:%M:%S"), record_count, suffix);
        }
        Ok(resp) => {
            let status = resp.status();
            let text   = resp.text().await.unwrap_or_default();
            eprintln!("[{}] ✗ HTTP {}: {}", Utc::now().format("%Y-%m-%d %H:%M:%S"), status, text);
        }
        Err(e) => {
            eprintln!("[{}] ✗ {}", Utc::now().format("%Y-%m-%d %H:%M:%S"), e);
        }
    }
}

// ── Historical mode ───────────────────────────────────────────────────────────

fn run_historical_logs(days: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "../output_k8s.json";
    let num_pods = PODS.len();
    let total_intervals = (days as i64 * 86_400) / INTERVAL_SECONDS;
    let total_records = total_intervals as usize * num_pods;

    println!("Historical logs: {} days, {} pods → {}", days, num_pods, output_path);
    println!("Total records: {}", total_records);

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut rng = rand::thread_rng();
    let now_us = Utc::now().timestamp_micros();

    writer.write_all(b"[")?;
    let mut first = true;
    let mut written = 0usize;

    for interval_idx in 0..total_intervals as usize {
        let ts_us = now_us - (interval_idx as i64 * INTERVAL_SECONDS * 1_000_000);
        for pod_idx in 0..num_pods {
            let record = generate_log_record(pod_idx, ts_us, None, HISTORICAL_LOGIN_ERROR_PROB, &mut rng);
            let json = serde_json::to_string(&record)?;
            if !first { writer.write_all(b",")?; }
            writer.write_all(json.as_bytes())?;
            first = false;
            written += 1;
        }
        if written % CHUNK_SIZE == 0 {
            writer.flush()?;
            println!("Progress: {:.1}% ({}/{})", written as f64 / total_records as f64 * 100.0, written, total_records);
        }
    }

    writer.write_all(b"]")?;
    writer.flush()?;
    println!("\nDone! {} records → '{}'", written, output_path);
    Ok(())
}

fn run_historical_metrics(days: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "../output_k8s_metrics.json";
    let num_pods = PODS.len();
    let total_intervals = (days as i64 * 86_400) / INTERVAL_SECONDS;
    let total_records = total_intervals as usize * num_pods;

    println!("Historical metrics: {} days, {} pods → {}", days, num_pods, output_path);
    println!("Total records: {}", total_records);

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut rng = rand::thread_rng();
    let now_us = Utc::now().timestamp_micros();

    writer.write_all(b"[")?;
    let mut first = true;
    let mut written = 0usize;

    for interval_idx in 0..total_intervals as usize {
        let ts_us = now_us - (interval_idx as i64 * INTERVAL_SECONDS * 1_000_000);
        for pod_idx in 0..num_pods {
            let record = generate_metric_record(pod_idx, ts_us, None, &mut rng);
            let json = serde_json::to_string(&record)?;
            if !first { writer.write_all(b",")?; }
            writer.write_all(json.as_bytes())?;
            first = false;
            written += 1;
        }
        if written % CHUNK_SIZE == 0 {
            writer.flush()?;
            println!("Progress: {:.1}% ({}/{})", written as f64 / total_records as f64 * 100.0, written, total_records);
        }
    }

    writer.write_all(b"]")?;
    writer.flush()?;
    println!("\nDone! {} records → '{}'", written, output_path);
    println!("Ingest: cargo run -- ingest ../output_k8s_metrics.json --stream k8s_metrics");
    Ok(())
}

fn run_historical_traces(days: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "../output_k8s_traces.json";
    let num_pods = PODS.len();
    // ~5 traces per pod per 30-second interval (denser than logs/metrics)
    let trace_interval_secs: i64 = 30;
    let traces_per_interval = 5usize;
    let total_intervals = (days as i64 * 86_400) / trace_interval_secs;
    let total_spans_approx = total_intervals as usize * num_pods * traces_per_interval * 2; // avg ~2 spans/trace

    println!("Historical traces: {} days → {}", days, output_path);
    println!("Approx spans: ~{}", total_spans_approx);

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut rng = rand::thread_rng();
    let now_us = Utc::now().timestamp_micros();

    writer.write_all(b"[")?;
    let mut first = true;
    let mut written = 0usize;

    for interval_idx in 0..total_intervals as usize {
        let base_ts_us = now_us - (interval_idx as i64 * trace_interval_secs * 1_000_000);
        for pod_idx in 0..num_pods {
            for _ in 0..traces_per_interval {
                // Scatter timestamps within the interval
                let jitter_us = rng.gen_range(0..(trace_interval_secs * 1_000_000));
                let ts_us = base_ts_us - jitter_us;
                let spans = generate_trace_spans(pod_idx, ts_us, None, &mut rng);
                for span in spans {
                    let json = serde_json::to_string(&span)?;
                    if !first { writer.write_all(b",")?; }
                    writer.write_all(json.as_bytes())?;
                    first = false;
                    written += 1;
                }
            }
        }
        if written % CHUNK_SIZE == 0 {
            writer.flush()?;
            let pct = interval_idx as f64 / total_intervals as f64 * 100.0;
            println!("Progress: {:.1}% ({} spans written)", pct, written);
        }
    }

    writer.write_all(b"]")?;
    writer.flush()?;
    println!("\nDone! {} spans → '{}'", written, output_path);
    println!("Ingest: cargo run -- ingest ../output_k8s_traces.json --stream k8s_traces");
    Ok(())
}

// ── Ingest mode ───────────────────────────────────────────────────────────────

const INGEST_BATCH_SIZE: usize = 2_000;

#[tokio::main]
async fn run_ingest(file_path: &str, org: &str, stream: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;

    println!("Ingest mode");
    println!("  File:   {}", file_path);
    println!("  Org:    {}", org);
    println!("  Stream: {}", stream);

    let mut raw = String::new();
    File::open(file_path)?.read_to_string(&mut raw)?;

    let client = Client::builder().danger_accept_invalid_certs(true).build()?;

    match stream {
        // ── Metrics → OTLP /v1/metrics (creates per-field streams of type=metrics)
        "k8s_metrics" => {
            let url = format!("{}/api/{}/v1/metrics", API_BASE, org);
            println!("  URL:    {} (OTLP metrics)", url);
            let records: Vec<K8sMetricRecord> = serde_json::from_str(&raw)?;
            let total = records.len();
            println!("Loaded {} records, sending as OTLP in batches of 100\n", total);
            let mut sent = 0usize;
            for batch in records.chunks(100) {
                let payload = metrics_to_otlp_payload(batch);
                let resp = client.post(&url)
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .json(&payload)
                    .send().await?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    eprintln!("Batch failed ({}): {}", status, text);
                    std::process::exit(1);
                }
                sent += batch.len();
                println!("Progress: {:.1}% ({}/{})", sent as f64 / total as f64 * 100.0, sent, total);
            }
            println!("\nDone! Ingested {} records as OTLP metrics", sent);
        }

        // ── Traces → OTLP /v1/traces with stream-name header (stream_type=traces)
        "k8s_traces" => {
            let url = format!("{}/api/{}/v1/traces", API_BASE, org);
            println!("  URL:    {} (OTLP traces, stream-name: {})", url, DEFAULT_STREAM_TRACES);
            let spans: Vec<K8sTraceRecord> = serde_json::from_str(&raw)?;
            let total = spans.len();
            println!("Loaded {} spans, sending as OTLP in batches of 200\n", total);
            let mut sent = 0usize;
            for batch in spans.chunks(200) {
                let payload = traces_to_otlp_payload(batch);
                let resp = client.post(&url)
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .header("stream-name", DEFAULT_STREAM_TRACES)
                    .json(&payload)
                    .send().await?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    eprintln!("Batch failed ({}): {}", status, text);
                    std::process::exit(1);
                }
                sent += batch.len();
                println!("Progress: {:.1}% ({}/{})", sent as f64 / total as f64 * 100.0, sent, total);
            }
            println!("\nDone! Ingested {} spans as OTLP traces into {}/{}", sent, org, DEFAULT_STREAM_TRACES);
        }

        // ── Logs (and any other stream) → flat JSON /_json endpoint
        _ => {
            let url = format!("{}/api/{}/{}/_json", API_BASE, org, stream);
            println!("  URL:    {}", url);
            let records: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
            let total = records.len();
            println!("Loaded {} records, sending in batches of {}\n", total, INGEST_BATCH_SIZE);
            let mut sent = 0usize;
            for batch in records.chunks(INGEST_BATCH_SIZE) {
                let resp = client.post(&url)
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .json(batch)
                    .send().await?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    eprintln!("Batch failed ({}): {}", status, text);
                    std::process::exit(1);
                }
                sent += batch.len();
                println!("Progress: {:.1}% ({}/{})", sent as f64 / total as f64 * 100.0, sent, total);
            }
            println!("\nDone! Ingested {} records into {}/{}", sent, org, stream);
        }
    }

    Ok(())
}

// ── Live mode ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn run_live_logs(anomaly_type: Option<AnomalyType>) -> Result<(), Box<dyn std::error::Error>> {
    let api_url = format!("{}/api/{}/{}/_json", API_BASE, DEFAULT_ORG, DEFAULT_STREAM_LOGS);
    let client = Client::builder().danger_accept_invalid_certs(true).build()?;
    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!("Live logs → {}", api_url);
    print_anomaly_header(&anomaly_state);
    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;
        if let Some(ref mut s) = anomaly_state { s.tick(&mut rng); }

        let now_us = Utc::now().timestamp_micros();
        let records: Vec<K8sLogRecord> = (0..PODS_PER_TICK)
            .map(|i| generate_log_record(i, now_us, anomaly_state.as_ref(), LOGIN_ERROR_BACKGROUND_PROB, &mut rng))
            .collect();

        post_live(&client, &api_url, &records, &anomaly_state).await;
    }
}

#[tokio::main]
async fn run_live_metrics(anomaly_type: Option<AnomalyType>) -> Result<(), Box<dyn std::error::Error>> {
    let api_url = format!("{}/api/{}/v1/metrics", API_BASE, DEFAULT_ORG);
    let client = Client::builder().danger_accept_invalid_certs(true).build()?;
    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!("Live metrics (OTLP) → {}", api_url);
    println!("Streams (type=metrics): cpu_percent, memory_percent, request_latency_ms, error_rate, ...");
    print_anomaly_header(&anomaly_state);
    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;
        if let Some(ref mut s) = anomaly_state { s.tick(&mut rng); }

        let now_us = Utc::now().timestamp_micros();
        let records: Vec<K8sMetricRecord> = (0..PODS_PER_TICK)
            .map(|i| generate_metric_record(i, now_us, anomaly_state.as_ref(), &mut rng))
            .collect();

        let payload = metrics_to_otlp_payload(&records);
        post_otlp(&client, &api_url, None, &payload, records.len(), &anomaly_state).await;
    }
}

#[tokio::main]
async fn run_live_traces(anomaly_type: Option<AnomalyType>) -> Result<(), Box<dyn std::error::Error>> {
    let api_url = format!("{}/api/{}/v1/traces", API_BASE, DEFAULT_ORG);
    let client = Client::builder().danger_accept_invalid_certs(true).build()?;
    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!("Live traces (OTLP) → {} [stream-name: {}]", api_url, DEFAULT_STREAM_TRACES);
    print_anomaly_header(&anomaly_state);
    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;
        if let Some(ref mut s) = anomaly_state { s.tick(&mut rng); }

        let now_us = Utc::now().timestamp_micros();
        // 3 traces per pod per second → ~30 root spans + children
        let mut spans: Vec<K8sTraceRecord> = Vec::new();
        for pod_idx in 0..PODS_PER_TICK {
            for _ in 0..3 {
                spans.extend(generate_trace_spans(pod_idx, now_us, anomaly_state.as_ref(), &mut rng));
            }
        }

        let payload = traces_to_otlp_payload(&spans);
        post_otlp(&client, &api_url, Some(DEFAULT_STREAM_TRACES), &payload, spans.len(), &anomaly_state).await;
    }
}

async fn post_live<T: serde::Serialize>(
    client: &Client,
    url: &str,
    records: &[T],
    anomaly_state: &Option<AnomalyState>,
) {
    let anomaly_active = anomaly_state.as_ref().map(|a| a.is_active()).unwrap_or(false);
    let anomaly_remaining = anomaly_state.as_ref().map(|a| a.remaining_secs).unwrap_or(0);

    match client.post(url).basic_auth(USERNAME, Some(PASSWORD)).json(records).send().await {
        Ok(resp) if resp.status().is_success() => {
            let suffix = if anomaly_active {
                format!(" [ANOMALY ACTIVE: {}s remaining]", anomaly_remaining)
            } else {
                String::new()
            };
            println!("[{}] ✓ {} records{}", Utc::now().format("%Y-%m-%d %H:%M:%S"), records.len(), suffix);
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("[{}] ✗ HTTP {}: {}", Utc::now().format("%Y-%m-%d %H:%M:%S"), status, body);
        }
        Err(e) => {
            eprintln!("[{}] ✗ {}", Utc::now().format("%Y-%m-%d %H:%M:%S"), e);
        }
    }
}

fn print_anomaly_header(anomaly_state: &Option<AnomalyState>) {
    match anomaly_state {
        Some(a) => println!("Anomaly: {} (10% chance/sec to trigger 2–5 min spike)", a.anomaly_type.label()),
        None => println!("No anomaly injection. Add --anomaly <type> to inject."),
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "historical" => {
            let days   = parse_flag_u32(&args, "--days").unwrap_or(7);
            let stream = parse_flag_str(&args, "--stream").unwrap_or_else(|| "logs".to_string());

            let result = match stream.as_str() {
                "logs"    => run_historical_logs(days),
                "metrics" => run_historical_metrics(days),
                "traces"  => run_historical_traces(days),
                "all" => {
                    run_historical_logs(days)
                        .and_then(|_| run_historical_metrics(days))
                        .and_then(|_| run_historical_traces(days))
                }
                other => {
                    eprintln!("Unknown stream '{}'. Valid: logs, metrics, traces, all", other);
                    std::process::exit(1);
                }
            };
            if let Err(e) = result { eprintln!("Error: {}", e); std::process::exit(1); }
        }
        "ingest" => {
            let file_path = if args.len() > 2 && !args[2].starts_with("--") {
                args[2].as_str()
            } else {
                "../output_k8s.json"
            };
            let org    = parse_flag_str(&args, "--org").unwrap_or_else(|| DEFAULT_ORG.to_string());
            let stream = parse_flag_str(&args, "--stream").unwrap_or_else(|| DEFAULT_STREAM_LOGS.to_string());
            if let Err(e) = run_ingest(file_path, &org, &stream) {
                eprintln!("Error: {}", e); std::process::exit(1);
            }
        }
        "live" => {
            let stream = parse_flag_str(&args, "--stream").unwrap_or_else(|| "logs".to_string());
            let anomaly_type = parse_flag_str(&args, "--anomaly").and_then(|s| AnomalyType::from_str(&s));

            if let Some(ref s) = parse_flag_str(&args, "--anomaly") {
                if AnomalyType::from_str(s).is_none() {
                    eprintln!("Unknown anomaly type '{}'. Valid: cpu, memory, errors, restarts, latency, login", s);
                    std::process::exit(1);
                }
            }

            let result = match stream.as_str() {
                "logs"    => run_live_logs(anomaly_type),
                "metrics" => run_live_metrics(anomaly_type),
                "traces"  => run_live_traces(anomaly_type),
                other => {
                    eprintln!("Unknown stream '{}'. Valid: logs, metrics, traces", other);
                    std::process::exit(1);
                }
            };
            if let Err(e) = result { eprintln!("Error: {}", e); std::process::exit(1); }
        }
        "help" | "--help" | "-h" => { print_usage(); }
        _ => { print_usage(); std::process::exit(1); }
    }
}

fn parse_flag_u32(args: &[String], flag: &str) -> Option<u32> {
    args.windows(2).find(|w| w[0] == flag).and_then(|w| w[1].parse().ok())
}

fn parse_flag_str(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn print_usage() {
    println!("k8s_data_gen — Kubernetes observability data generator\n");
    println!("USAGE:");
    println!("  cargo run -- historical [--days N] [--stream logs|metrics|traces|all]");
    println!("    Generates N days of historical data (default: 7 days, logs)");
    println!("    logs    → ../output_k8s.json");
    println!("    metrics → ../output_k8s_metrics.json");
    println!("    traces  → ../output_k8s_traces.json");
    println!("    all     → generates all three files\n");
    println!("  cargo run -- ingest [FILE] [--org ORG] [--stream STREAM]");
    println!("    Ingests a JSON file into OpenObserve (endpoint depends on stream):");
    println!("    k8s_logs    → /_json           (stream_type=logs)");
    println!("    k8s_metrics → /v1/metrics OTLP (stream_type=metrics, per-field streams)");
    println!("    k8s_traces  → /v1/traces OTLP  (stream_type=traces, stream=k8s_traces)");
    println!("    FILE default: ../output_k8s.json, --stream default: k8s_logs\n");
    println!("  cargo run -- live [--stream logs|metrics|traces] [--anomaly TYPE]");
    println!("    Streams live data at ~10 records/sec");
    println!("    logs    → /_json           (stream_type=logs)");
    println!("    metrics → /v1/metrics OTLP (stream_type=metrics)");
    println!("    traces  → /v1/traces OTLP  (stream_type=traces, stream=k8s_traces)\n");
    println!("ANOMALY TYPES:");
    println!("  cpu        CPU spike (4-7x normal) — logs + metrics");
    println!("  memory     Memory spike (3.5-5x normal) — logs + metrics");
    println!("  errors     Error rate spike (30-80%) — all streams");
    println!("  restarts   Restart spike (5-15) — logs + metrics");
    println!("  latency    Latency spike (15-40x) — all streams");
    println!("  login      Auth failure flood — logs only\n");
    println!("EXAMPLES:");
    println!("  cargo run -- historical --days 7 --stream all");
    println!("  cargo run -- ingest ../output_k8s_metrics.json --stream k8s_metrics");
    println!("  cargo run -- ingest ../output_k8s_traces.json --stream k8s_traces");
    println!("  cargo run -- live --stream metrics --anomaly cpu");
    println!("  cargo run -- live --stream traces --anomaly latency");
    println!("  cargo run -- live --stream logs --anomaly login\n");
    println!("ANOMALY DETECTION CONFIGS (stream → type → detection_function):");
    println!("  Logs/CPU:           k8s_logs    → logs    → custom SQL AVG(cpu_millicores)");
    println!("  Logs/Errors:        k8s_logs    → logs    → count(*) filter log_level=ERROR");
    println!("  Logs/Latency:       k8s_logs    → logs    → custom SQL AVG(response_time_ms)");
    println!("  Logs/Restarts:      k8s_logs    → logs    → custom SQL SUM(restarts)");
    println!("  Logs/Login:         k8s_logs    → logs    → count(*) filter message~'login error'");
    println!("  Metrics/CPU:        cpu_percent        → metrics → avg(value)");
    println!("  Metrics/Memory:     memory_percent     → metrics → avg(value)");
    println!("  Metrics/Latency:    request_latency_ms → metrics → avg(value)");
    println!("  Metrics/ErrorRate:  error_rate         → metrics → avg(value)");
    println!("  Traces/Latency:     k8s_traces  → traces  → avg(duration_ms)");
    println!("  Traces/Errors:      k8s_traces  → traces  → count(*) filter status=ERROR");
    println!("\nSee top of src/main.rs for exact SQL query strings.");
}
