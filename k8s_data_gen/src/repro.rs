/// Reproduces issue #1848: same service appears as two separate entries in
/// Discovered Services when logs lack service.name but share disambiguation with traces.
///
/// Sends:
///   1. ECS-style logs to stream "repro_logs" — no service.name, has namespace="ecs-prod"
///   2. Traces to OTLP — service.name="ecs-api-service", namespace="ecs-prod"
///
/// Expected (buggy) result after Reset Services:
///   Two entries in Discovered Services with identical disambiguation but different service names:
///     - service_name="repro_logs"  (stream-name fallback, logs only)
///     - service_name="ecs-api-service" (from service.name resource attr, traces only)
///
/// Required OO config before running:
///   Correlation Settings → Detection Rules → add identity set:
///     - Name: any (e.g. "repro")
///     - distinguish_by: ["k8s-namespace"]
///     - Correlate without service attribute: ON
///   Tracked Attributes: ensure "k8s-namespace" is in the tracked list
use chrono::Utc;
use reqwest::Client;
use serde::Serialize;

use crate::config::{api_base, password, username, DEFAULT_ORG};

const REPRO_NAMESPACE: &str = "ecs-prod";
const REPRO_CLUSTER: &str = "ecs-cluster-1";
const REPRO_SERVICE_NAME: &str = "ecs-api-service";
const REPRO_LOG_STREAM: &str = "repro_logs";
const REPRO_TRACE_STREAM: &str = "repro_traces";
const REPRO_RECORD_COUNT: usize = 20;

/// ECS-style log record — NO service field (simulates Firelens/Fluent Bit logs).
/// Intentionally omits all fields in the "service" alias group so the processor
/// falls back to the stream name when resolving service_name.
#[derive(Serialize)]
struct EcsLogRecord {
    _timestamp: i64,
    cluster: String,
    namespace: String,
    pod: String,
    container: String,
    log_level: String,
    message: String,
    // Deliberately no: service, service_name, app, application, svc, etc.
}

pub async fn run_repro() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    print_header();
    send_ecs_logs(&client).await?;
    send_repro_traces(&client).await?;
    print_instructions();

    Ok(())
}

async fn send_ecs_logs(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "{}/api/{}/{}/_json",
        api_base(),
        DEFAULT_ORG,
        REPRO_LOG_STREAM
    );

    let now = Utc::now().timestamp_micros();
    let records: Vec<EcsLogRecord> = (0..REPRO_RECORD_COUNT)
        .map(|i| EcsLogRecord {
            _timestamp: now + (i as i64 * 1_000_000),
            cluster: REPRO_CLUSTER.to_string(),
            namespace: REPRO_NAMESPACE.to_string(),
            pod: format!("ecs-task-{:06x}", i * 0xABCD),
            container: "api".to_string(),
            log_level: if i % 5 == 0 { "ERROR" } else { "INFO" }.to_string(),
            message: format!("ECS log record {} — no service.name set", i),
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
            "[logs]   ✓ {} records → stream '{}' (NO service field)",
            records.len(),
            REPRO_LOG_STREAM
        );
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!("[logs]   ✗ HTTP {}: {}", status, body);
    }

    Ok(())
}

async fn send_repro_traces(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/api/{}/v1/traces", api_base(), DEFAULT_ORG);

    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let resource_spans: Vec<serde_json::Value> = (0..REPRO_RECORD_COUNT)
        .map(|i| {
            let start_ns = now_ns + (i as i64 * 1_000_000_000);
            let end_ns = start_ns + 5_000_000; // 5ms duration
            let trace_id = format!("{:032x}", i as u128 * 0xDEADBEEF1234);
            let span_id = format!("{:016x}", i as u64 * 0xCAFE);
            serde_json::json!({
                "resource": {
                    "attributes": [
                        {"key": "service.name",       "value": {"stringValue": REPRO_SERVICE_NAME}},
                        // k8s.namespace.name → stored as k8s_namespace_name → in k8s-namespace alias group
                        // (plain "namespace" → stored as service_namespace → NOT in alias group)
                        {"key": "k8s.namespace.name", "value": {"stringValue": REPRO_NAMESPACE}},
                        {"key": "k8s.cluster.name",   "value": {"stringValue": REPRO_CLUSTER}}
                    ]
                },
                "scopeSpans": [{
                    "scope": {"name": "repro-1848"},
                    "spans": [{
                        "traceId": trace_id,
                        "spanId": span_id,
                        "parentSpanId": "",
                        "name": "repro-span",
                        "kind": 2,
                        "startTimeUnixNano": start_ns.to_string(),
                        "endTimeUnixNano": end_ns.to_string(),
                        "status": {"code": 1},
                        "attributes": [
                            {"key": "http.status_code", "value": {"intValue": 200}}
                        ]
                    }]
                }]
            })
        })
        .collect();

    let payload = serde_json::json!({"resourceSpans": resource_spans});

    let resp = client
        .post(&url)
        .basic_auth(username(), Some(password()))
        .header("stream-name", REPRO_TRACE_STREAM)
        .json(&payload)
        .send()
        .await?;

    if resp.status().is_success() {
        println!(
            "[traces] ✓ {} spans → stream '{}' (service.name='{}')",
            REPRO_RECORD_COUNT, REPRO_TRACE_STREAM, REPRO_SERVICE_NAME
        );
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!("[traces] ✗ HTTP {}: {}", status, body);
    }

    Ok(())
}

fn print_header() {
    println!("=== Repro: issue #1848 — split Discovered Services ===\n");
    println!("Sending:");
    println!(
        "  logs   → stream '{}' | namespace='{}' | NO service.name",
        REPRO_LOG_STREAM, REPRO_NAMESPACE
    );
    println!(
        "  traces → stream '{}' | namespace='{}' | service.name='{}'",
        REPRO_TRACE_STREAM, REPRO_NAMESPACE, REPRO_SERVICE_NAME
    );
    println!();
}

fn print_instructions() {
    println!("\n=== Next steps to see the bug ===\n");
    println!("1. In OO UI → Correlation Settings → Field Mappings:");
    println!("   Ensure 'K8s Namespace' alias group includes field 'namespace'");
    println!("   (it does by default — no change needed)\n");
    println!("2. Correlation Settings → Detection Rules → Add identity set:");
    println!("   - distinguish_by: k8s-namespace");
    println!("   - 'Correlate without service attribute': ON\n");
    println!("3. Correlation Settings → Tracked Attributes:");
    println!("   Ensure 'k8s-namespace' is tracked\n");
    println!("4. Click 'Reset Services' and wait ~10s\n");
    println!("5. Open 'Discovered Services' tab\n");
    println!("   BUG: Two entries appear:");
    println!("     service_name='{}' (logs, stream-name fallback)", REPRO_LOG_STREAM);
    println!(
        "     service_name='{}' (traces, from service.name attr)",
        REPRO_SERVICE_NAME
    );
    println!("   Both have identical disambiguation: {{\"k8s-namespace\": \"{}\"}}", REPRO_NAMESPACE);
    println!("\n   EXPECTED: One entry grouping both streams together.");
}
