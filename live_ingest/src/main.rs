use chrono::Utc;
use rand::seq::SliceRandom;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{fs::File, time::Duration};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OlympicsRecord {
    id: String,
    name: String,
    continent: String,
    flag_url: String,
    gold_medals: u32,
    silver_medals: u32,
    bronze_medals: u32,
    total_medals: u32,
    rank: u32,
    rank_total_medals: u32,
    body: String,
}

#[derive(Debug, Serialize)]
struct EnrichedRecord {
    #[serde(flatten)]
    original_data: OlympicsRecord,
    unique_id: String,
    _timestamp: i64,
}

const API_URL: &str = "http://localhost:5080/api/default/oly/_json";
const USERNAME: &str = "root@example.com";
const PASSWORD: &str = "Complexpass#123";
const RECORDS_PER_SECOND: usize = 2;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read olympics.json from parent directory
    let olympics_path = "../olympics.json";
    println!("Reading olympics data from {}...", olympics_path);

    let file = File::open(olympics_path)?;
    let olympics_data: Vec<OlympicsRecord> = serde_json::from_reader(file)?;

    println!("Loaded {} records from olympics.json", olympics_data.len());
    println!(
        "Starting live ingestion: {} records per second",
        RECORDS_PER_SECOND
    );
    println!("API: {}", API_URL);
    println!("Press Ctrl+C to stop\n");

    // Create HTTP client with basic auth
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        // Select 2 random records
        let random_records: Vec<EnrichedRecord> = olympics_data
            .choose_multiple(&mut rng, RECORDS_PER_SECOND)
            .map(|record| EnrichedRecord {
                original_data: record.clone(),
                unique_id: Uuid::new_v4().to_string(),
                _timestamp: Utc::now().timestamp_micros(),
            })
            .collect();

        // Send to API
        match client
            .post(API_URL)
            .basic_auth(USERNAME, Some(PASSWORD))
            .json(&random_records)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    println!(
                        "[{}] ✓ Ingested {} records (IDs: {})",
                        Utc::now().format("%Y-%m-%d %H:%M:%S"),
                        RECORDS_PER_SECOND,
                        random_records
                            .iter()
                            .map(|r| r.unique_id.chars().take(8).collect::<String>())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    eprintln!(
                        "[{}] ✗ Failed with status {}: {}",
                        Utc::now().format("%Y-%m-%d %H:%M:%S"),
                        status,
                        error_text
                    );
                }
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
