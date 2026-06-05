use std::time::Duration;

use chrono::Utc;
use reqwest::Client;

use super::generate::generate_metric_record;
use super::otlp::metrics_to_otlp_payload;
use crate::anomaly::{AnomalyState, AnomalyType};
use crate::client::http::post_otlp;
use crate::config::{API_BASE, DEFAULT_ORG, PODS_PER_TICK};
use crate::utils::print_anomaly_header;

pub async fn run_live_metrics(
    anomaly_type: Option<AnomalyType>,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_url = format!("{}/api/{}/v1/metrics", API_BASE, DEFAULT_ORG);
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!("Live metrics (OTLP) → {}", api_url);
    println!(
        "Streams (type=metrics): cpu_percent, memory_percent, request_latency_ms, error_rate, ..."
    );
    print_anomaly_header(&anomaly_state);
    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;
        if let Some(ref mut s) = anomaly_state {
            s.tick(&mut rng);
        }

        let now_us = Utc::now().timestamp_micros();
        let records: Vec<_> = (0..PODS_PER_TICK)
            .map(|i| generate_metric_record(i, now_us, anomaly_state.as_ref(), &mut rng))
            .collect();

        let payload = metrics_to_otlp_payload(&records);
        post_otlp(
            &client,
            &api_url,
            None,
            &payload,
            records.len(),
            &anomaly_state,
        )
        .await;
    }
}
