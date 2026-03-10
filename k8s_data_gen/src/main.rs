/// K8s Data Generator
///
/// Generates realistic Kubernetes observability data for OpenObserve testing.
///
/// USAGE:
///   cargo run -- historical --days 7          # write N days → ../output_k8s.json
///   cargo run -- ingest                                    # ingest ../output_k8s.json into default/k8s_logs
///   cargo run -- ingest /path/to/file.json                # ingest a specific file
///   cargo run -- ingest /path/to/file.json --org myorg   # override org
///   cargo run -- ingest /path/to/file.json --stream logs  # override stream name
///   cargo run -- live                          # stream live, 10 records/sec
///   cargo run -- live --anomaly cpu            # inject CPU spike anomalies
///   cargo run -- live --anomaly memory         # inject memory pressure anomalies
///   cargo run -- live --anomaly errors         # inject error rate spikes
///   cargo run -- live --anomaly restarts       # inject pod restart spikes
///   cargo run -- live --anomaly latency        # inject p99 latency spikes
///   cargo run -- live --anomaly login          # inject auth failure / brute-force spikes
///
/// ANOMALY DETECTION CONFIGS (create in OpenObserve after training):
///
///   Stream: k8s_logs  |  detection_interval: 5m  |  training_window_days: 2
///
///   CPU:      custom_sql = "SELECT histogram(_timestamp,'1 minute') AS time_bucket, AVG(cpu_millicores) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket"
///   Memory:   custom_sql = "SELECT histogram(_timestamp,'1 minute') AS time_bucket, AVG(memory_mb) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket"
///   Errors:   query_mode = filters, filter: log_level = ERROR, detection_function = count
///   Restarts: custom_sql = "SELECT histogram(_timestamp,'1 minute') AS time_bucket, SUM(restarts) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket"
///   Latency:  custom_sql = "SELECT histogram(_timestamp,'1 minute') AS time_bucket, AVG(response_time_ms) AS value FROM k8s_logs GROUP BY time_bucket ORDER BY time_bucket"
///   Login:    query_mode = filters, filter: message contains "login error", detection_function = count
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
const DEFAULT_ORG: &str = "default";
const DEFAULT_STREAM: &str = "k8s_logs";
const USERNAME: &str = "root@example.com";
const PASSWORD: &str = "Complexpass#123";

/// How many seconds between records per pod in historical mode
const INTERVAL_SECONDS: i64 = 10;

/// How many pod records to POST each second in live mode
const PODS_PER_TICK: usize = 10;

/// Chunk size for historical JSON writing
const CHUNK_SIZE: usize = 5_000;

// ── K8s topology ─────────────────────────────────────────────────────────────

struct Pod {
    namespace: &'static str,
    service: &'static str,
    container: &'static str,
    /// base CPU millicores (normal load)
    base_cpu: u32,
    /// base memory MB (normal load)
    base_mem: u32,
    /// base requests/sec
    base_rps: u32,
    /// base response time ms
    base_rt: f64,
    /// base error rate (fraction)
    base_err: f64,
}

const PODS: &[Pod] = &[
    Pod {
        namespace: "payments",
        service: "payments-api",
        container: "api",
        base_cpu: 350,
        base_mem: 512,
        base_rps: 220,
        base_rt: 45.0,
        base_err: 0.005,
    },
    Pod {
        namespace: "payments",
        service: "payments-worker",
        container: "worker",
        base_cpu: 180,
        base_mem: 256,
        base_rps: 80,
        base_rt: 30.0,
        base_err: 0.002,
    },
    Pod {
        namespace: "inventory",
        service: "inventory-service",
        container: "service",
        base_cpu: 280,
        base_mem: 384,
        base_rps: 150,
        base_rt: 60.0,
        base_err: 0.008,
    },
    Pod {
        namespace: "inventory",
        service: "inventory-db",
        container: "postgres",
        base_cpu: 420,
        base_mem: 768,
        base_rps: 50,
        base_rt: 12.0,
        base_err: 0.001,
    },
    Pod {
        namespace: "frontend",
        service: "web-server",
        container: "nginx",
        base_cpu: 120,
        base_mem: 128,
        base_rps: 800,
        base_rt: 8.0,
        base_err: 0.003,
    },
    Pod {
        namespace: "frontend",
        service: "static-cdn",
        container: "cdn",
        base_cpu: 90,
        base_mem: 96,
        base_rps: 600,
        base_rt: 5.0,
        base_err: 0.001,
    },
    Pod {
        namespace: "monitoring",
        service: "prometheus",
        container: "prometheus",
        base_cpu: 460,
        base_mem: 900,
        base_rps: 20,
        base_rt: 25.0,
        base_err: 0.000,
    },
    Pod {
        namespace: "monitoring",
        service: "grafana",
        container: "grafana",
        base_cpu: 200,
        base_mem: 320,
        base_rps: 40,
        base_rt: 120.0,
        base_err: 0.002,
    },
    Pod {
        namespace: "infra",
        service: "nginx-ingress",
        container: "controller",
        base_cpu: 310,
        base_mem: 256,
        base_rps: 1200,
        base_rt: 3.0,
        base_err: 0.004,
    },
    Pod {
        namespace: "infra",
        service: "coredns",
        container: "coredns",
        base_cpu: 150,
        base_mem: 192,
        base_rps: 400,
        base_rt: 2.0,
        base_err: 0.000,
    },
];

const CLUSTERS: &[&str] = &["prod-us-east-1", "prod-eu-west-1", "staging-us-west-2"];
const NODES: &[&str] = &["node-1", "node-2", "node-3", "node-4", "node-5"];

const LOG_LEVELS_NORMAL: &[(&str, u32)] =
    &[("DEBUG", 10), ("INFO", 75), ("WARN", 12), ("ERROR", 3)];

const LOG_LEVELS_ERROR: &[(&str, u32)] = &[("DEBUG", 2), ("INFO", 10), ("WARN", 20), ("ERROR", 68)];

/// Normal event type distribution. "login" appears ~2% of the time as background.
const EVENT_TYPES: &[&str] = &[
    "request",
    "request",
    "request",
    "request",
    "request",
    "request",
    "request",
    "healthcheck",
    "pod_lifecycle",
    "login",  // ~10% background login events (normal successful logins)
];

const MESSAGES_INFO: &[&str] = &[
    "Processed request successfully",
    "Cache hit for key",
    "Health check passed",
    "Database query completed",
    "gRPC call returned OK",
    "Scheduled job ran",
    "Config reload triggered",
];

const MESSAGES_WARN: &[&str] = &[
    "Slow query detected, took >500ms",
    "Retry attempt 1 of 3",
    "Connection pool near capacity",
    "Rate limit threshold approaching",
    "Certificate expiring in 14 days",
];

const MESSAGES_ERROR: &[&str] = &[
    "Upstream service returned 503",
    "Database connection timeout",
    "Failed to parse response body",
    "OOMKilled: container exceeded memory limit",
    "Panic: index out of bounds",
    "TLS handshake failed",
    "Request queue overflow, dropping request",
];

/// Normal login messages — rare, show up as background noise (~2% of records)
const MESSAGES_LOGIN_OK: &[&str] = &[
    "User login successful",
    "Service account authenticated",
    "OAuth token validated",
    "API key auth successful",
];

/// Login error messages — these are what the anomaly detection filter targets.
/// Filter config: message contains "login error"
///
/// BACKGROUND RATE (normal): ~0.5% of all records = ~3/min per 10 pods.
/// This background noise is intentional: it gives RCF non-zero buckets to
/// learn from during training, so "0–5/min" is the established baseline.
/// Without any login errors in training data, RCF has nothing to learn from
/// (histogram returns 0 rows, not rows with value=0).
///
/// ANOMALY RATE (injected): 100% of records = ~600/min → clearly anomalous.
///
/// NOTE: lowercase "login error" matches the filter: message contains "login error"
const MESSAGES_LOGIN_ERROR: &[&str] = &[
    "login error: invalid credentials for user",
    "login error: account locked after repeated failures",
    "login error: token expired or revoked",
    "login error: IP blocked after too many attempts",
    "login error: MFA verification failed",
    "login error: service account rejected",
    "login error: brute force attempt detected",
];

/// Probability of a normal (non-anomaly) record generating a "login error" message
/// during live mode. At 10 pods/sec this produces ~3 login errors/min as background noise.
const LOGIN_ERROR_BACKGROUND_PROB: f64 = 0.005;

/// Login error probability used when generating historical training data.
/// 60% ensures every 10-second histogram bucket has ~6 login error records,
/// giving RCF enough non-zero buckets to establish a meaningful baseline.
/// (At 0.5% most buckets are empty → histogram returns 0 rows → training fails.)
const HISTORICAL_LOGIN_ERROR_PROB: f64 = 0.60;

// ── Record type ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct K8sRecord {
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

// ── Anomaly mode ──────────────────────────────────────────────────────────────

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
            "cpu" => Some(AnomalyType::Cpu),
            "memory" => Some(AnomalyType::Memory),
            "errors" => Some(AnomalyType::Errors),
            "restarts" => Some(AnomalyType::Restarts),
            "latency" => Some(AnomalyType::Latency),
            "login" => Some(AnomalyType::Login),
            _ => None,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            AnomalyType::Cpu => "cpu",
            AnomalyType::Memory => "memory",
            AnomalyType::Errors => "errors",
            AnomalyType::Restarts => "restarts",
            AnomalyType::Latency => "latency",
            AnomalyType::Login => "login",
        }
    }
}

struct AnomalyState {
    anomaly_type: AnomalyType,
    /// Seconds remaining in current anomaly window; 0 = not active
    remaining_secs: u32,
    /// Seconds until we next consider triggering (used as cooldown)
    cooldown_secs: u32,
}

impl AnomalyState {
    fn new(anomaly_type: AnomalyType) -> Self {
        AnomalyState {
            anomaly_type,
            remaining_secs: 0,
            cooldown_secs: 30, // start after 30s so first batch is normal
        }
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
        // 10% chance each second to trigger a 2–5 minute anomaly
        if rng.gen_bool(0.10) {
            self.remaining_secs = rng.gen_range(120..=300);
            self.cooldown_secs = 120; // 2-min cooldown after anomaly ends
            println!(
                "[ANOMALY] {} spike started — will last {}s",
                self.anomaly_type.label(),
                self.remaining_secs
            );
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
        if pick < *weight {
            return item;
        }
        pick -= weight;
    }
    choices.last().unwrap().0
}

fn generate_record(
    pod_idx: usize,
    timestamp_us: i64,
    anomaly: Option<&AnomalyState>,
    login_error_prob: f64,
    rng: &mut impl Rng,
) -> K8sRecord {
    let pod = &PODS[pod_idx];
    let cluster = CLUSTERS[rng.gen_range(0..CLUSTERS.len())];
    let node = NODES[pod_idx % NODES.len()];

    // Derive a stable-ish pod hash suffix from pod_idx (zero-padded to 6 chars)
    let pod_name = format!("{}-{:06x}", pod.service, pod_idx * 0x1a3f + 0x4b7);

    let is_anomaly = anomaly.map(|a| a.is_active()).unwrap_or(false);
    let anomaly_type = anomaly.map(|a| &a.anomaly_type);

    // ── Metrics with optional anomaly injection ──────────────────────────────

    let cpu = if is_anomaly && anomaly_type == Some(&AnomalyType::Cpu) {
        (pod.base_cpu as f64 * rng.gen_range(4.0..7.0)) as u32
    } else {
        (pod.base_cpu as f64 * (1.0 + rng.gen_range(-0.20_f64..=0.20))) as u32
    };

    let memory = if is_anomaly && anomaly_type == Some(&AnomalyType::Memory) {
        (pod.base_mem as f64 * rng.gen_range(3.5..5.0)) as u32
    } else {
        (pod.base_mem as f64 * (1.0 + rng.gen_range(-0.15_f64..=0.15))) as u32
    };

    let restarts: u32 = if is_anomaly && anomaly_type == Some(&AnomalyType::Restarts) {
        rng.gen_range(5..=15)
    } else if rng.gen_bool(0.002) {
        1
    } else {
        0
    };

    let response_time = if is_anomaly && anomaly_type == Some(&AnomalyType::Latency) {
        let spike: f64 = rng.gen_range(15.0..40.0);
        (pod.base_rt * spike) * (1.0 + rng.gen_range(-0.30_f64..=0.30))
    } else {
        (pod.base_rt * (1.0 + rng.gen_range(-0.25_f64..=0.25))).max(1.0)
    };

    let error_rate = if is_anomaly && anomaly_type == Some(&AnomalyType::Errors) {
        rng.gen_range(0.30..0.80_f64)
    } else {
        (pod.base_err * (1.0 + rng.gen_range(-0.50_f64..=0.50))).clamp(0.0, 0.05)
    };

    let rps = (pod.base_rps as f64 * (1.0 + rng.gen_range(-0.20_f64..=0.20))) as u32;
    let net_rx: u64 = (rps as u64 * rng.gen_range(800..1200)) as u64;
    let net_tx: u64 = (rps as u64 * rng.gen_range(400..800)) as u64;

    // ── Log level & message ──────────────────────────────────────────────────

    let is_error_anomaly = is_anomaly && anomaly_type == Some(&AnomalyType::Errors);
    let is_login_anomaly = is_anomaly && anomaly_type == Some(&AnomalyType::Login);

    let log_level = weighted_choice(
        if is_error_anomaly { LOG_LEVELS_ERROR } else { LOG_LEVELS_NORMAL },
        rng,
    );

    // During a login anomaly every record becomes a "login error" log line.
    // The anomaly detection config uses:
    //   query_mode = filters, filter: message contains "login error", detection_function = count
    //
    // Normal rate: ~0.5% of records → ~3 "login error" messages/min background noise.
    //   This is intentional: RCF needs non-zero histogram buckets during training to
    //   establish a baseline. Without any training rows it can't learn what "normal" is.
    //
    // Anomaly rate: 100% of records → ~600 "login error" lines/min → clearly anomalous.
    let (event_type, message) = if is_login_anomaly {
        let msg = MESSAGES_LOGIN_ERROR[rng.gen_range(0..MESSAGES_LOGIN_ERROR.len())].to_string();
        ("login_error", msg)
    } else if rng.gen_bool(login_error_prob) {
        // Background login error noise — keeps histogram buckets non-zero during training
        let msg = MESSAGES_LOGIN_ERROR[rng.gen_range(0..MESSAGES_LOGIN_ERROR.len())].to_string();
        ("login_error", msg)
    } else {
        let et = EVENT_TYPES[rng.gen_range(0..EVENT_TYPES.len())];
        // For normal "login" events emit a success message; everything else follows log level
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
        if rng.gen_bool(0.5) {
            429
        } else {
            500
        }
    } else {
        *[200u16, 200, 200, 200, 201, 204, 304][..]
            .choose(rng)
            .unwrap()
    };

    K8sRecord {
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

// ── Historical mode ───────────────────────────────────────────────────────────

fn run_historical(days: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "../output_k8s.json";
    let num_pods = PODS.len();
    let total_intervals = (days as i64 * 86_400) / INTERVAL_SECONDS;
    let total_records = total_intervals as usize * num_pods;

    println!(
        "Historical mode: {} days, {} pods, {} records/interval",
        days, num_pods, num_pods
    );
    println!("Total records to generate: {}", total_records);
    println!("Output: {}", output_path);

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut rng = rand::thread_rng();

    let now_us = Utc::now().timestamp_micros();

    writer.write_all(b"[")?;

    let mut first = true;
    let mut written = 0usize;

    for interval_idx in 0..total_intervals as usize {
        // timestamps go backward from now
        let ts_us = now_us - (interval_idx as i64 * INTERVAL_SECONDS * 1_000_000);

        for pod_idx in 0..num_pods {
            let record = generate_record(pod_idx, ts_us, None, HISTORICAL_LOGIN_ERROR_PROB, &mut rng);
            let json = serde_json::to_string(&record)?;

            if !first {
                writer.write_all(b",")?;
            }
            writer.write_all(json.as_bytes())?;
            first = false;
            written += 1;
        }

        // Flush and report progress every CHUNK_SIZE records
        if written % CHUNK_SIZE == 0 {
            writer.flush()?;
            println!(
                "Progress: {:.1}% ({}/{})",
                written as f64 / total_records as f64 * 100.0,
                written,
                total_records
            );
        }
    }

    writer.write_all(b"]")?;
    writer.flush()?;

    println!("\nDone! Wrote {} records to '{}'", written, output_path);
    println!("\nNext steps:");
    println!("  1. Ingest: batch_upload.sh or POST ../output_k8s.json to OpenObserve");
    println!("  2. Create anomaly detection configs (see top of this file for SQL)");
    println!("  3. POST /api/v1/default/anomaly_detection/<config_id>/train");
    println!("  4. Run: cargo run -- live --anomaly cpu");

    Ok(())
}

// ── Ingest mode ───────────────────────────────────────────────────────────────

/// Ingest a JSON file into OpenObserve by splitting it into batches.
///
/// The file must be a JSON array of records (as produced by historical mode).
/// Records are sent INGEST_BATCH_SIZE at a time.
const INGEST_BATCH_SIZE: usize = 2_000;

#[tokio::main]
async fn run_ingest(
    file_path: &str,
    org: &str,
    stream: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;

    let url = format!("{}/api/{}/{}/_json", API_BASE, org, stream);

    println!("Ingest mode");
    println!("  File:   {}", file_path);
    println!("  Org:    {}", org);
    println!("  Stream: {}", stream);
    println!("  URL:    {}", url);

    let mut raw = String::new();
    File::open(file_path)?.read_to_string(&mut raw)?;

    let records: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
    let total = records.len();
    println!("Loaded {} records, sending in batches of {}\n", total, INGEST_BATCH_SIZE);

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let mut sent = 0usize;
    for batch in records.chunks(INGEST_BATCH_SIZE) {
        let body = serde_json::to_string(batch)?;
        let resp = client
            .post(&url)
            .basic_auth(USERNAME, Some(PASSWORD))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            eprintln!("Batch failed ({}): {}", status, text);
            std::process::exit(1);
        }

        sent += batch.len();
        println!(
            "Progress: {:.1}% ({}/{})",
            sent as f64 / total as f64 * 100.0,
            sent,
            total
        );
    }

    println!("\nDone! Ingested {} records into {}/{}", sent, org, stream);
    Ok(())
}

// ── Live mode ─────────────────────────────────────────────────────────────────

#[tokio::main]
async fn run_live(anomaly_type: Option<AnomalyType>) -> Result<(), Box<dyn std::error::Error>> {
    let api_url = format!("{}/api/{}/{}/_json", API_BASE, DEFAULT_ORG, DEFAULT_STREAM);

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!("Live mode — streaming to {}", api_url);
    println!("Pods: {}, Records/sec: {}", PODS.len(), PODS_PER_TICK);

    match &anomaly_state {
        Some(a) => println!(
            "Anomaly injection: {} (10% chance/sec to trigger 2–5 min spike)\n",
            a.anomaly_type.label()
        ),
        None => println!("No anomaly injection. Run with --anomaly <cpu|memory|errors|restarts|latency|login> to inject.\n"),
    }

    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;

        // Advance anomaly state machine
        if let Some(ref mut state) = anomaly_state {
            state.tick(&mut rng);
        }

        let now_us = Utc::now().timestamp_micros();

        let records: Vec<K8sRecord> = (0..PODS_PER_TICK)
            .map(|pod_idx| generate_record(pod_idx, now_us, anomaly_state.as_ref(), LOGIN_ERROR_BACKGROUND_PROB, &mut rng))
            .collect();

        let anomaly_active = anomaly_state
            .as_ref()
            .map(|a| a.is_active())
            .unwrap_or(false);
        let anomaly_remaining = anomaly_state
            .as_ref()
            .map(|a| a.remaining_secs)
            .unwrap_or(0);

        match client
            .post(&api_url)
            .basic_auth(USERNAME, Some(PASSWORD))
            .json(&records)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let status_str = if anomaly_active {
                    format!(" [ANOMALY ACTIVE: {}s remaining]", anomaly_remaining)
                } else {
                    String::new()
                };
                println!(
                    "[{}] ✓ {} records ingested{}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                    records.len(),
                    status_str
                );
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!(
                    "[{}] ✗ HTTP {}: {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                    status,
                    body
                );
            }
            Err(e) => {
                eprintln!(
                    "[{}] ✗ Request error: {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                    e
                );
            }
        }
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
            let days = parse_flag_u32(&args, "--days").unwrap_or(7);
            if let Err(e) = run_historical(days) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        "ingest" => {
            // positional arg: file path (optional, default ../output_k8s.json)
            // flags:  --org <org>      (default: "default")
            //         --stream <name>  (default: "k8s_logs")
            let file_path = if args.len() > 2 && !args[2].starts_with("--") {
                args[2].as_str()
            } else {
                "../output_k8s.json"
            };
            let org = parse_flag_str(&args, "--org")
                .unwrap_or_else(|| DEFAULT_ORG.to_string());
            let stream = parse_flag_str(&args, "--stream")
                .unwrap_or_else(|| DEFAULT_STREAM.to_string());
            if let Err(e) = run_ingest(file_path, &org, &stream) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        "live" => {
            let anomaly_type =
                parse_flag_str(&args, "--anomaly").and_then(|s| AnomalyType::from_str(&s));

            if let Some(ref s) = parse_flag_str(&args, "--anomaly") {
                if AnomalyType::from_str(s).is_none() {
                    eprintln!("Unknown anomaly type '{}'. Valid: cpu, memory, errors, restarts, latency, login", s);
                    std::process::exit(1);
                }
            }

            if let Err(e) = run_live(anomaly_type) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}

fn parse_flag_u32(args: &[String], flag: &str) -> Option<u32> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w[1].parse().ok())
}

fn parse_flag_str(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn print_usage() {
    println!("k8s_data_gen — Kubernetes observability data generator for OpenObserve\n");
    println!("USAGE:");
    println!("  cargo run -- historical [--days N]          Generate N days of data (default: 7)");
    println!("  cargo run -- ingest [FILE] [--org ORG] [--stream STREAM]");
    println!("                                               Ingest JSON file into OpenObserve");
    println!("                                               FILE default: ../output_k8s.json");
    println!("                                               --org    default: default");
    println!("                                               --stream default: k8s_logs");
    println!("  cargo run -- live                            Stream live K8s data");
    println!("  cargo run -- live --anomaly <type>          Stream with anomaly injection\n");
    println!("ANOMALY TYPES:");
    println!("  cpu        CPU millicores spike (4-7x normal)");
    println!("  memory     Memory MB spike (3.5-5x normal)");
    println!("  errors     Error log rate spike (30-80% of logs become ERROR)");
    println!("  restarts   Pod restart count spike (5-15 restarts)");
    println!("  latency    Response time spike (15-40x normal)");
    println!("  login      Auth failure spike (floods message field with 'login error' lines)\n");
    println!("STREAM:  k8s_logs (10 pods across payments/inventory/frontend/monitoring/infra)\n");
    println!("ANOMALY DETECTION CONFIGS — create these in OpenObserve:");
    println!("  CPU:      AVG(cpu_millicores) with custom SQL histogram query");
    println!("  Memory:   AVG(memory_mb) with custom SQL histogram query");
    println!("  Errors:   COUNT(*) with filter log_level=ERROR");
    println!("  Restarts: SUM(restarts) with custom SQL histogram query");
    println!("  Latency:  AVG(response_time_ms) with custom SQL histogram query");
    println!("  Login:    COUNT(*) with filter message contains 'login error'");
    println!("\nSee top of src/main.rs for full SQL query strings and detection configs.");
}
