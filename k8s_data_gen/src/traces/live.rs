use std::time::Duration;

use chrono::Utc;
use reqwest::Client;

use super::flows::generate_prod_trace;
use super::generate::generate_trace_spans;
use super::otlp::{prod_spans_to_resource_spans, traces_to_otlp_payload};
use crate::anomaly::{AnomalyState, AnomalyType};
use crate::client::grpc::{grpc_client, send_grpc_traces};
use crate::client::http::post_otlp;
use crate::config::{api_base, grpc_endpoint, DEFAULT_ORG, DEFAULT_STREAM_TRACES, PODS_PER_TICK};
use crate::utils::print_anomaly_header;

pub async fn run_live_traces(
    anomaly_type: Option<AnomalyType>,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_url = format!("{}/api/{}/v1/traces", api_base(), DEFAULT_ORG);
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!(
        "Live traces (OTLP) → {} [stream-name: {}]",
        api_url, DEFAULT_STREAM_TRACES
    );
    print_anomaly_header(&anomaly_state);
    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;
        if let Some(ref mut s) = anomaly_state {
            s.tick(&mut rng);
        }

        let now_us = Utc::now().timestamp_micros();
        let mut spans = Vec::new();
        for pod_idx in 0..PODS_PER_TICK {
            for _ in 0..3 {
                spans.extend(generate_trace_spans(
                    pod_idx,
                    now_us,
                    anomaly_state.as_ref(),
                    &mut rng,
                ));
            }
        }

        let payload = traces_to_otlp_payload(&spans);
        post_otlp(
            &client,
            &api_url,
            Some(DEFAULT_STREAM_TRACES),
            &payload,
            spans.len(),
            &anomaly_state,
        )
        .await;
    }
}

pub async fn run_live_traces_grpc(
    anomaly_type: Option<AnomalyType>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = grpc_client(&grpc_endpoint()).await?;
    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!(
        "Live traces (gRPC OTLP) → {} [org: {}, stream: {}]",
        &grpc_endpoint(), DEFAULT_ORG, DEFAULT_STREAM_TRACES
    );
    println!("Services: api-gateway, auth-service, cart-service, inventory-service,");
    println!("          payment-service, order-service, product-catalog, search-service,");
    println!("          notification-service, user-service, redis-cache, postgres-primary, postgres-replica");
    println!("Flows: checkout(35%) | product-search(30%) | login(15%) | browse(20%)");
    print_anomaly_header(&anomaly_state);
    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;
        if let Some(ref mut s) = anomaly_state {
            s.tick(&mut rng);
        }

        let now_us = Utc::now().timestamp_micros() as u64;
        let mut all_spans = Vec::new();
        for _ in 0..10 {
            all_spans.extend(generate_prod_trace(
                now_us,
                anomaly_state.as_ref(),
                &mut rng,
            ));
        }

        let span_count = all_spans.len();
        let resource_spans = prod_spans_to_resource_spans(all_spans);

        match send_grpc_traces(
            &mut client,
            resource_spans,
            DEFAULT_ORG,
            DEFAULT_STREAM_TRACES,
        )
        .await
        {
            Ok(_) => {
                let anomaly_active = anomaly_state
                    .as_ref()
                    .map(|a| a.is_active())
                    .unwrap_or(false);
                let anomaly_remaining = anomaly_state
                    .as_ref()
                    .map(|a| a.remaining_secs)
                    .unwrap_or(0);
                let suffix = if anomaly_active {
                    format!(" [ANOMALY ACTIVE: {}s remaining]", anomaly_remaining)
                } else {
                    String::new()
                };
                println!(
                    "[{}] ✓ {} spans (gRPC){}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                    span_count,
                    suffix
                );
            }
            Err(e) => {
                eprintln!(
                    "[{}] ✗ gRPC error: {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                    e
                );
                if let Ok(new_client) = grpc_client(&grpc_endpoint()).await {
                    client = new_client;
                }
            }
        }
    }
}
