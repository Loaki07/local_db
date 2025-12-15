use chrono::Utc;
use rand::seq::SliceRandom;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{env, fs::File, time::Duration};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sport_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    athlete_highlight: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participation_year: Option<u32>,
    unique_id: String,
    _timestamp: i64,
}

const API_URL: &str = "http://localhost:5080/api/default/oly/_json";
const USERNAME: &str = "root@example.com";
const PASSWORD: &str = "Complexpass#123";
const RECORDS_PER_SECOND: usize = 2;

// cargo r -- no-fts
// cargo r
// cargo r -- new-fields
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check for command line arguments
    let args: Vec<String> = env::args().collect();
    let no_fts_mode = args.iter().any(|arg| arg == "no-fts");
    let new_fields_mode = args.iter().any(|arg| arg == "new-fields");
    let include_body = !no_fts_mode;

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

    if new_fields_mode {
        println!("Mode: new-fields (includes body + sport_category, athlete_highlight, participation_year)");
    } else if no_fts_mode {
        println!("Mode: no-fts (body field excluded)");
    } else {
        println!("Mode: normal (includes body field)");
    }

    println!("Press Ctrl+C to stop\n");

    // Create HTTP client with basic auth
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let mut rng = rand::thread_rng();
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    // Define sport categories for random selection
    let sport_categories = vec![
        "Aquatics",
        "Athletics",
        "Gymnastics",
        "Team Sports",
        "Combat Sports",
        "Cycling",
        "Equestrian",
        "Rowing",
        "Sailing",
        "Shooting",
        "Weightlifting",
        "Wrestling",
        "Fencing",
        "Judo",
        "Taekwondo",
    ];

    let athlete_highlights = vec![
        "Record-breaking performance in finals",
        "Youngest medalist in team history",
        "Comeback victory after injury",
        "Dominant performance throughout tournament",
        "Historic achievement for the nation",
        "First-time Olympic medalist",
        "Multiple gold medals in single Games",
        "Upset victory against top seed",
    ];

    loop {
        interval.tick().await;

        // Select 2 random records
        let random_records: Vec<EnrichedRecord> = olympics_data
            .choose_multiple(&mut rng, RECORDS_PER_SECOND)
            .map(|record| {
                let (sport_category, athlete_highlight, participation_year) = if new_fields_mode {
                    (
                        Some(sport_categories.choose(&mut rng).unwrap().to_string()),
                        Some(athlete_highlights.choose(&mut rng).unwrap().to_string()),
                        Some(rng.gen_range(2000..=2024)),
                    )
                } else {
                    (None, None, None)
                };

                EnrichedRecord {
                    id: record.id.clone(),
                    name: record.name.clone(),
                    continent: record.continent.clone(),
                    flag_url: record.flag_url.clone(),
                    gold_medals: record.gold_medals,
                    silver_medals: record.silver_medals,
                    bronze_medals: record.bronze_medals,
                    total_medals: record.total_medals,
                    rank: record.rank,
                    rank_total_medals: record.rank_total_medals,
                    body: if include_body {
                        Some(record.body.clone())
                    } else {
                        None
                    },
                    sport_category,
                    athlete_highlight,
                    participation_year,
                    unique_id: Uuid::new_v4().to_string(),
                    _timestamp: Utc::now().timestamp_micros(),
                }
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
