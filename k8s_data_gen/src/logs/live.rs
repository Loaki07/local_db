use std::time::Duration;

use chrono::Utc;
use reqwest::Client;

use super::generate::generate_log_record;
use super::types::LOGIN_ERROR_BACKGROUND_PROB;
use crate::anomaly::{AnomalyState, AnomalyType};
use crate::client::http::post_live;
use crate::config::{api_base, DEFAULT_ORG, DEFAULT_STREAM_LOGS, PODS_PER_TICK};
use crate::utils::print_anomaly_header;

pub async fn run_live_logs(
    anomaly_type: Option<AnomalyType>,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_url = format!(
        "{}/api/{}/{}/_json",
        api_base(), DEFAULT_ORG, DEFAULT_STREAM_LOGS
    );
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut anomaly_state = anomaly_type.map(AnomalyState::new);

    println!("Live logs → {}", api_url);
    print_anomaly_header(&anomaly_state);
    println!("Press Ctrl+C to stop.\n");

    loop {
        interval.tick().await;
        if let Some(ref mut s) = anomaly_state {
            s.tick(&mut rng);
        }

        let now_us = Utc::now().timestamp_micros();
        let records: Vec<_> = (0..PODS_PER_TICK)
            .map(|i| {
                generate_log_record(
                    i,
                    now_us,
                    anomaly_state.as_ref(),
                    LOGIN_ERROR_BACKGROUND_PROB,
                    &mut rng,
                )
            })
            .collect();

        post_live(&client, &api_url, &records, &anomaly_state).await;
    }
}
