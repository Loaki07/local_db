use std::{
    fs::File,
    io::{BufWriter, Write},
};

use chrono::Utc;
use rand::Rng;

use super::generate::generate_trace_spans;
use crate::config::CHUNK_SIZE;
use crate::topology::PODS;

pub fn run_historical_traces(days: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "../output_k8s_traces.json";
    let num_pods = PODS.len();
    let trace_interval_secs: i64 = 30;
    let traces_per_interval = 5usize;
    let total_intervals = (days as i64 * 86_400) / trace_interval_secs;
    let total_spans_approx = total_intervals as usize * num_pods * traces_per_interval * 2;

    println!("Historical traces: {} days → {}", days, output_path);
    println!("Approx spans: ~{}", total_spans_approx);

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut rng = rand::thread_rng();
    let now_us = Utc::now().timestamp_micros();

    writer.write_all(b"[")?;
    let mut first = true;
    let mut written = 0usize;

    for interval_idx in 0..total_intervals as usize {
        let base_ts_us = now_us - (interval_idx as i64 * trace_interval_secs * 1_000_000);
        for pod_idx in 0..num_pods {
            for _ in 0..traces_per_interval {
                let jitter_us = rng.gen_range(0..(trace_interval_secs * 1_000_000));
                let ts_us = base_ts_us - jitter_us;
                let spans = generate_trace_spans(pod_idx, ts_us, None, &mut rng);
                for span in spans {
                    let json = serde_json::to_string(&span)?;
                    if !first {
                        writer.write_all(b",")?;
                    }
                    writer.write_all(json.as_bytes())?;
                    first = false;
                    written += 1;
                }
            }
        }
        if written % CHUNK_SIZE == 0 {
            writer.flush()?;
            let pct = interval_idx as f64 / total_intervals as f64 * 100.0;
            println!("Progress: {:.1}% ({} spans written)", pct, written);
        }
    }

    writer.write_all(b"]")?;
    writer.flush()?;
    println!("\nDone! {} spans → '{}'", written, output_path);
    println!("Ingest: cargo run -- ingest ../output_k8s_traces.json --stream k8s_traces");
    Ok(())
}
