use std::{
    fs::File,
    io::{BufWriter, Write},
};

use chrono::Utc;

use super::generate::generate_log_record;
use super::types::HISTORICAL_LOGIN_ERROR_PROB;
use crate::config::{CHUNK_SIZE, INTERVAL_SECONDS};
use crate::topology::PODS;

pub fn run_historical_logs(days: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "../output_k8s.json";
    let num_pods = PODS.len();
    let total_intervals = (days as i64 * 86_400) / INTERVAL_SECONDS;
    let total_records = total_intervals as usize * num_pods;

    println!(
        "Historical logs: {} days, {} pods → {}",
        days, num_pods, output_path
    );
    println!("Total records: {}", total_records);

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut rng = rand::thread_rng();
    let now_us = Utc::now().timestamp_micros();

    writer.write_all(b"[")?;
    let mut first = true;
    let mut written = 0usize;

    for interval_idx in 0..total_intervals as usize {
        let ts_us = now_us - (interval_idx as i64 * INTERVAL_SECONDS * 1_000_000);
        for pod_idx in 0..num_pods {
            let record =
                generate_log_record(pod_idx, ts_us, None, HISTORICAL_LOGIN_ERROR_PROB, &mut rng);
            let json = serde_json::to_string(&record)?;
            if !first {
                writer.write_all(b",")?;
            }
            writer.write_all(json.as_bytes())?;
            first = false;
            written += 1;
        }
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
    println!("\nDone! {} records → '{}'", written, output_path);
    Ok(())
}
