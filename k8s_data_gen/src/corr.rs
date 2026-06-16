/// Correlation test data generator.
///
/// Sends a one-shot batch of logs + metrics + traces for a fixed set of services,
/// all sharing the same service name and namespace so they can be correlated in OO.
///
/// Stream names:
///   corr_logs    → logs    (flat JSON: service, namespace, cluster fields)
///   corr_metrics → metrics (OTLP: service.name, k8s.namespace.name resource attrs)
///   corr_traces  → traces  (OTLP: service.name, namespace resource attrs)
///
/// Required OO config (Correlation Settings):
///   Field Mappings → defaults cover all fields used here
///   Tracked Attributes → add: service, k8s-namespace
///   Detection Rules → add identity set:
///     distinguish_by: [k8s-namespace]   (or leave empty to group by service name only)
///
/// After Reset Services, Discovered Services should show one entry per service,
/// each row with logs + metrics + traces linked.
use chrono::Utc;
use reqwest::Client;
use serde::Serialize;

use crate::config::{api_base, password, username, DEFAULT_ORG};

const LOG_STREAM: &str = "corr_logs";
const METRIC_STREAM: &str = "corr_metrics";
const TRACE_STREAM: &str = "corr_traces";
const CLUSTER: &str = "prod-cluster";
const RECORDS_PER_SERVICE: usize = 15;

struct Service {
    name: &'static str,
    namespace: &'static str,
}

const SERVICES: &[Service] = &[
    Service {
        name: "payments-api",
        namespace: "payments",
    },
    Service {
        name: "auth-service",
        namespace: "platform",
    },
    Service {
        name: "inventory-db",
        namespace: "inventory",
    },
];

// ── Log records ──────────────────────────────────────────────────────────────

/// Flat log record. `service` and `namespace` map to the "service" and
/// "k8s-namespace" alias groups respectively, enabling correlation.
#[derive(Serialize)]
struct CorrLogRecord {
    _timestamp: i64,
    service: String,
    namespace: String,
    cluster: String,
    log_level: String,
    message: String,
}

async fn send_logs(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    for svc in SERVICES {
        let url = format!(
            "{}/api/{}/{}/_json",
            api_base(),
            DEFAULT_ORG,
            LOG_STREAM
        );
        let now = Utc::now().timestamp_micros();
        let records: Vec<CorrLogRecord> = (0..RECORDS_PER_SERVICE)
            .map(|i| CorrLogRecord {
                _timestamp: now + (i as i64 * 1_000_000),
                service: svc.name.to_string(),
                namespace: svc.namespace.to_string(),
                cluster: CLUSTER.to_string(),
                log_level: if i % 7 == 0 { "ERROR" } else { "INFO" }.to_string(),
                message: format!("[{}] request processed ok", svc.name),
            })
            .collect();

        let resp = client
            .post(&url)
            .basic_auth(username(), Some(password()))
            .json(&records)
            .send()
            .await?;

        if resp.status().is_success() {
            println!(
                "  [logs]    ✓ {} records | service='{}' namespace='{}'",
                records.len(),
                svc.name,
                svc.namespace
            );
        } else {
            eprintln!(
                "  [logs]    ✗ HTTP {} | {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
    }
    Ok(())
}

// ── Metric records ────────────────────────────────────────────────────────────

async fn send_metrics(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/api/{}/v1/metrics", api_base(), DEFAULT_ORG);

    for svc in SERVICES {
        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let data_points: Vec<serde_json::Value> = (0..RECORDS_PER_SERVICE)
            .map(|i| {
                let t = (now_ns + i as i64 * 1_000_000_000).to_string();
                let attrs = serde_json::json!([
                    {"key": "service.name",       "value": {"stringValue": svc.name}},
                    {"key": "k8s.namespace.name", "value": {"stringValue": svc.namespace}},
                    {"key": "k8s.cluster.name",   "value": {"stringValue": CLUSTER}},
                ]);
                serde_json::json!({
                    "attributes": attrs,
                    "timeUnixNano": t,
                    "asDouble": 42.0 + i as f64
                })
            })
            .collect();

        let payload = serde_json::json!({
            "resourceMetrics": [{
                "resource": {
                    "attributes": [
                        {"key": "service.name",       "value": {"stringValue": svc.name}},
                        {"key": "k8s.namespace.name", "value": {"stringValue": svc.namespace}},
                        {"key": "k8s.cluster.name",   "value": {"stringValue": CLUSTER}},
                    ]
                },
                "scopeMetrics": [{
                    "scope": {"name": "corr-data-gen"},
                    "metrics": [
                        {
                            "name": "request_latency_ms",
                            "gauge": {"dataPoints": data_points}
                        }
                    ]
                }]
            }]
        });

        let resp = client
            .post(&url)
            .basic_auth(username(), Some(password()))
            .header("stream-name", METRIC_STREAM)
            .json(&payload)
            .send()
            .await?;

        if resp.status().is_success() {
            println!(
                "  [metrics] ✓ {} datapoints | service='{}' namespace='{}'",
                RECORDS_PER_SERVICE,
                svc.name,
                svc.namespace
            );
        } else {
            eprintln!(
                "  [metrics] ✗ HTTP {} | {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
    }
    Ok(())
}

// ── Trace spans ───────────────────────────────────────────────────────────────

async fn send_traces(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/api/{}/v1/traces", api_base(), DEFAULT_ORG);

    for svc in SERVICES {
        let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let spans: Vec<serde_json::Value> = (0..RECORDS_PER_SERVICE)
            .map(|i| {
                let start = now_ns + i as i64 * 1_000_000_000;
                let end = start + 5_000_000;
                serde_json::json!({
                    "traceId": format!("{:032x}", i as u128 * 0xDEAD0000 + 1),
                    "spanId":  format!("{:016x}", i as u64 * 0xBEEF + 1),
                    "parentSpanId": "",
                    "name": "corr-span",
                    "kind": 2,
                    "startTimeUnixNano": start.to_string(),
                    "endTimeUnixNano": end.to_string(),
                    "status": {"code": 1},
                    "attributes": [
                        {"key": "http.status_code", "value": {"intValue": 200}}
                    ]
                })
            })
            .collect();

        let payload = serde_json::json!({
            "resourceSpans": [{
                "resource": {
                    "attributes": [
                        {"key": "service.name",       "value": {"stringValue": svc.name}},
                        {"key": "k8s.namespace.name", "value": {"stringValue": svc.namespace}},
                        {"key": "k8s.cluster.name",   "value": {"stringValue": CLUSTER}},
                    ]
                },
                "scopeSpans": [{
                    "scope": {"name": "corr-data-gen"},
                    "spans": spans
                }]
            }]
        });

        let resp = client
            .post(&url)
            .basic_auth(username(), Some(password()))
            .header("stream-name", TRACE_STREAM)
            .json(&payload)
            .send()
            .await?;

        if resp.status().is_success() {
            println!(
                "  [traces]  ✓ {} spans | service='{}' namespace='{}'",
                RECORDS_PER_SERVICE,
                svc.name,
                svc.namespace
            );
        } else {
            eprintln!(
                "  [traces]  ✗ HTTP {} | {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
    }
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run_corr() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    println!("=== Correlation test data ===\n");
    println!("Services: {}", SERVICES.iter().map(|s| s.name).collect::<Vec<_>>().join(", "));
    println!("Streams:  {} | {} | {}", LOG_STREAM, METRIC_STREAM, TRACE_STREAM);
    println!("Records:  {} per service per type\n", RECORDS_PER_SERVICE);

    send_logs(&client).await?;
    send_metrics(&client).await?;
    send_traces(&client).await?;

    println!("\n=== OO configuration needed ===\n");
    println!("Correlation Settings → Tracked Attributes:");
    println!("  Add: service, k8s-namespace\n");
    println!("Correlation Settings → Detection Rules → Add identity set:");
    println!("  distinguish_by: [k8s-namespace]");
    println!("  'Correlate without service attribute': OFF (all streams have service.name)\n");
    println!("Click 'Reset Services', wait ~10s, then open 'Discovered Services'.");
    println!("Expected: 3 entries (one per service), each with logs + metrics + traces.\n");
    println!("To test the #1848 bug (service split), run: cargo run -- repro");

    Ok(())
}
