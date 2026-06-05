use reqwest::Client;

use crate::client::http::stream_json_array;
use crate::config::{API_BASE, DEFAULT_STREAM_TRACES, INGEST_BATCH_SIZE, PASSWORD, USERNAME};
use crate::metrics::{otlp::metrics_to_otlp_payload, types::K8sMetricRecord};
use crate::traces::{otlp::traces_to_otlp_payload, types::K8sTraceRecord};

pub async fn run_ingest(
    file_path: &str,
    org: &str,
    stream: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Ingest mode");
    println!("  File:   {}", file_path);
    println!("  Org:    {}", org);
    println!("  Stream: {}", stream);

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    match stream {
        "k8s_metrics" => {
            let url = format!("{}/api/{}/v1/metrics", API_BASE, org);
            println!("  URL:    {} (OTLP metrics)", url);

            let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<K8sMetricRecord>>(4);
            let handle = stream_json_array::<K8sMetricRecord>(file_path.to_string(), 100, tx);
            let mut sent = 0usize;
            while let Some(batch) = rx.recv().await {
                let payload = metrics_to_otlp_payload(&batch);
                let resp = client
                    .post(&url)
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .json(&payload)
                    .send()
                    .await?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    eprintln!("Batch failed ({}): {}", status, text);
                    std::process::exit(1);
                }
                sent += batch.len();
                println!("Sent {} records", sent);
            }
            handle.await.map_err(|e| e.to_string())??;
            println!("\nDone! Ingested {} records as OTLP metrics", sent);
        }

        "k8s_traces" => {
            let url = format!("{}/api/{}/v1/traces", API_BASE, org);
            println!(
                "  URL:    {} (OTLP traces, stream-name: {})",
                url, DEFAULT_STREAM_TRACES
            );

            let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<K8sTraceRecord>>(4);
            let handle = stream_json_array::<K8sTraceRecord>(file_path.to_string(), 200, tx);
            let mut sent = 0usize;
            while let Some(batch) = rx.recv().await {
                let payload = traces_to_otlp_payload(&batch);
                let resp = client
                    .post(&url)
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .header("stream-name", DEFAULT_STREAM_TRACES)
                    .json(&payload)
                    .send()
                    .await?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    eprintln!("Batch failed ({}): {}", status, text);
                    std::process::exit(1);
                }
                sent += batch.len();
                println!("Sent {} spans", sent);
            }
            handle.await.map_err(|e| e.to_string())??;
            println!(
                "\nDone! Ingested {} spans as OTLP traces into {}/{}",
                sent, org, DEFAULT_STREAM_TRACES
            );
        }

        _ => {
            let url = format!("{}/api/{}/{}/_json", API_BASE, org, stream);
            println!("  URL:    {}", url);

            let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<serde_json::Value>>(4);
            let handle = stream_json_array::<serde_json::Value>(
                file_path.to_string(),
                INGEST_BATCH_SIZE,
                tx,
            );
            let mut sent = 0usize;
            while let Some(batch) = rx.recv().await {
                let resp = client
                    .post(&url)
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .json(&batch)
                    .send()
                    .await?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    eprintln!("Batch failed ({}): {}", status, text);
                    std::process::exit(1);
                }
                sent += batch.len();
                println!("Sent {} records", sent);
            }
            handle.await.map_err(|e| e.to_string())??;
            println!("\nDone! Ingested {} records into {}/{}", sent, org, stream);
        }
    }

    Ok(())
}
