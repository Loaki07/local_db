use chrono::{DateTime, Duration, Utc};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use uuid::Uuid;

// Struct to match your input JSON structure
// You'll need to adjust these fields based on your olympics.json structure
#[derive(Debug, Serialize, Deserialize, Clone)]
struct OlympicsRecord {
    // Add your fields here based on olympics.json
    #[serde(flatten)]
    original_data: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct EnrichedRecord {
    #[serde(flatten)]
    original_data: serde_json::Value,
    unique_id: String,
    _timestamp: i64,
}

const DAYS_TO_GENERATE: i64 = 10;
const INTERVAL_SECONDS: i64 = 10;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Update paths to look in parent directory
    let input_path = "../olympics.json";
    let output_path = "../output.json";

    // Read input data
    println!("Reading input file...");
    let input_data: Vec<OlympicsRecord> = serde_json::from_reader(File::open(input_path)?)?;
    let input_len = input_data.len();

    // Calculate total records
    let seconds_in_day = 24 * 60 * 60;
    let total_records = (DAYS_TO_GENERATE * seconds_in_day / INTERVAL_SECONDS) as usize;

    println!("Generating {} records...", total_records);

    // Create output file with BufWriter for efficient writing
    let output_file = File::create(output_path)?;
    let mut writer = BufWriter::new(output_file);

    // Get current timestamp in microseconds
    let current_timestamp = Utc::now().timestamp_micros();

    // Write opening bracket
    writer.write_all(b"[")?;

    // Generate records in parallel chunks
    let chunk_size = 10000; // Adjust based on your system's memory
    let chunks = total_records / chunk_size
        + (if total_records % chunk_size != 0 {
            1
        } else {
            0
        });

    for chunk_index in 0..chunks {
        let start = chunk_index * chunk_size;
        let end = (start + chunk_size).min(total_records);

        let records: Vec<String> = (start..end)
            .into_par_iter()
            .map(|i| {
                let record_index = i % input_len;
                let base_record = &input_data[record_index];

                let enriched = EnrichedRecord {
                    original_data: base_record.original_data.clone(),
                    unique_id: Uuid::new_v4().to_string(),
                    _timestamp: current_timestamp - (i as i64 * INTERVAL_SECONDS * 1_000_000),
                };

                serde_json::to_string(&enriched).unwrap()
            })
            .collect();

        // Write chunk to file
        for (i, record) in records.iter().enumerate() {
            writer.write_all(record.as_bytes())?;
            if chunk_index < chunks - 1 || i < records.len() - 1 {
                writer.write_all(b",")?;
            }
        }

        // Flush periodically to prevent memory buildup
        writer.flush()?;

        println!(
            "Progress: {:.1}%",
            (end as f64 / total_records as f64) * 100.0
        );
    }

    // Write closing bracket
    writer.write_all(b"]")?;
    writer.flush()?;

    println!(
        "Data generation complete! Output written to '{}'",
        output_path
    );
    Ok(())
}
