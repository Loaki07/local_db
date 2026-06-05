//! Histogram Left-Edge Drop — Reproduction Scenario
//!
//! Generates a flat, stable metric stream that starts at a NON-BUCKET-ALIGNED
//! timestamp. When queried via OpenObserve's histogram() function, the first
//! bucket is partial (covers less than one full interval), producing a count
//! lower than all subsequent buckets — the visual "left-edge drop".
//!
//! Root cause: OpenObserve rewrites histogram() to date_bin(interval, ts, '2001-01-01').
//! date_bin snaps the first bucket boundary LEFT of query start_time, so the first
//! bucket only contains data from [start_time → next_boundary), not a full interval.
//!
//! USAGE:
//!   cargo run --bin histogram_edge_sim -- preview
//!       Print bucket math only. No file I/O.
//!
//!   cargo run --bin histogram_edge_sim -- generate
//!       Write flat records to ../output_histogram_edge_sim.json
//!
//!   cargo run --bin histogram_edge_sim -- ingest
//!       POST the file to OpenObserve (stream: histogram_edge_sim)
//!
//!   cargo run --bin histogram_edge_sim -- full
//!       generate + ingest in one shot
//!
//! HOW TO REPRODUCE IN OPENOBSERVE:
//!   1. cargo run --bin histogram_edge_sim -- full
//!   2. Open dashboard, create a bar/line panel on stream "histogram_edge_sim"
//!   3. SQL:  SELECT histogram(_timestamp) AS zo_sql_key, count(*) AS zo_sql_num
//!            FROM "histogram_edge_sim"
//!            GROUP BY zo_sql_key ORDER BY zo_sql_key
//!   4. Set time range to the window printed by "preview"
//!   5. Observe: first bar is visibly shorter than all other bars (left-edge drop)

use chrono::{TimeZone, Utc};
use rand::Rng;
use reqwest::Client;
use serde::Serialize;
use std::{
    env,
    fs::File,
    io::{BufReader, BufWriter, Write},
};

// ── Config ────────────────────────────────────────────────────────────────────
//
// Override any of these via env vars:
//   OO_API_BASE  — e.g. https://main.o2aks1.internal.zinclabs.dev
//   OO_ORG       — organisation slug (default: "default")
//   OO_USERNAME  — basic-auth user
//   OO_PASSWORD  — basic-auth password
//   OO_STREAM    — stream name to ingest into (default: "histogram_edge_sim")

fn cfg_api_base() -> String {
    std::env::var("OO_API_BASE").unwrap_or_else(|_| "http://localhost:5080".into())
}
fn cfg_org() -> String {
    std::env::var("OO_ORG").unwrap_or_else(|_| "default".into())
}
fn cfg_stream() -> String {
    std::env::var("OO_STREAM").unwrap_or_else(|_| "histogram_edge_sim".into())
}
fn cfg_username() -> String {
    std::env::var("OO_USERNAME").unwrap_or_else(|_| "root@example.com".into())
}
fn cfg_password() -> String {
    std::env::var("OO_PASSWORD").unwrap_or_else(|_| "Complexpass#123".into())
}

// ── Histogram interval ────────────────────────────────────────────────────────
// For a 2-hour query window OO auto-selects 30s interval.
// Records are generated at 10s steps aligned from midnight.
// With 30s buckets: each full bucket has records at T+0s, T+10s, T+20s.
// Setting START_MISALIGNMENT_SECS=25 means query_start falls between the T+20s
// and T+30s record — so [query_start, T+30s) contains NO records → first bucket = 0.
// This produces the most dramatic possible left-edge drop (count goes 0 → full).

/// Must match OO's auto-selected interval for the query window.
/// 2h window → OO picks 30s. Change DATA_WINDOW_HOURS and this together.
const HISTOGRAM_INTERVAL_SECS: i64 = 30;
const HISTOGRAM_INTERVAL_US: i64 = HISTOGRAM_INTERVAL_SECS * 1_000_000;

/// One record every 10 seconds aligned from midnight (3 steps per 30s bucket).
const RECORD_INTERVAL_SECS: i64 = 10;
const RECORD_INTERVAL_US: i64 = RECORD_INTERVAL_SECS * 1_000_000;

/// Severity levels emitted at every step (different counts → different-height lines).
const SEVERITIES: &[(&str, i64)] = &[("INFO", 10), ("WARN", 4), ("ERROR", 1), ("DEBUG", 5)];

const STEPS_PER_FULL_BUCKET: i64 = HISTOGRAM_INTERVAL_SECS / RECORD_INTERVAL_SECS; // 3

/// OpenObserve's date_bin() origin: 2001-01-01T00:00:00 UTC in microseconds.
/// 978307200 is divisible by 30, so 30s bucket boundaries from this origin
/// are identical to 30s boundaries from Unix epoch.
const ORIGIN_US: i64 = 978_307_200 * 1_000_000;

/// Total hours of ingested data. Must be > QUERY_WINDOW_HOURS.
/// Data runs from midnight today so records exist well before the query window.
const DATA_WINDOW_HOURS: i64 = 24;

/// Query window shown in the dashboard.
const QUERY_WINDOW_HOURS: i64 = 2;

/// How many seconds into a bucket boundary the query start is placed.
///
/// With 10s record steps, records within each 30s bucket land at T+0s, T+10s, T+20s.
///
/// 15s misalignment: query_start = T+15s → only record at T+20s falls in [15s,30s)
///   → first bucket gets 1 step instead of 3 → 33% of full count → clear visible drop.
///
/// 25s would give 0 records in first bucket → SQL returns no row → null fill → no drop.
const START_MISALIGNMENT_SECS: i64 = 15;

const OUTPUT_FILE: &str = "../output_histogram_edge_sim.json";
const INGEST_BATCH_SIZE: usize = 2_000;

// ── Record ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SimRecord {
    _timestamp: i64,
    service: &'static str,
    namespace: &'static str,
    scenario: &'static str,
    severity: &'static str,
    requests_per_second: f64,
    cpu_millicores: u32,
    memory_mb: u32,
    response_time_ms: f64,
    error_rate: f64,
    message: &'static str,
}

// ── Bucket math ───────────────────────────────────────────────────────────────

/// Mirror of OpenObserve's date_bin(interval, ts, '2001-01-01') snap-down logic.
fn snap_down(ts_us: i64) -> i64 {
    let offset = ts_us - ORIGIN_US;
    // Rust % can be negative for negative offsets — force positive remainder
    let remainder =
        ((offset % HISTOGRAM_INTERVAL_US) + HISTOGRAM_INTERVAL_US) % HISTOGRAM_INTERVAL_US;
    ts_us - remainder
}

fn ts_str(us: i64) -> String {
    let secs = us / 1_000_000;
    let ns = ((us % 1_000_000) * 1_000) as u32;
    match Utc.timestamp_opt(secs, ns) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        _ => format!("{}us", us),
    }
}

// ── Preview ───────────────────────────────────────────────────────────────────

fn preview(query_start_us: i64, query_end_us: i64) {
    let first_bucket_start = snap_down(query_start_us);
    let offset_secs = (query_start_us - first_bucket_start) / 1_000_000;
    let remaining_secs = HISTOGRAM_INTERVAL_SECS - offset_secs;
    let first_steps = remaining_secs / RECORD_INTERVAL_SECS;
    let first_bucket_pct = remaining_secs as f64 / HISTOGRAM_INTERVAL_SECS as f64 * 100.0;

    println!("\n╔══ Histogram Left-Edge Drop — Reproduction Preview ═══════════════════╗");
    println!("║  BUG: date_bin() snaps first bucket LEFT of query start_time.        ║");
    println!("║  First bucket is partial → lower count → visual drop at left edge.   ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝");

    println!("\n── Query Window ──────────────────────────────────────────────────────────");
    println!("  start    : {}", ts_str(query_start_us));
    println!("  end      : {}", ts_str(query_end_us));
    println!(
        "  interval : {}s ({} min)",
        HISTOGRAM_INTERVAL_SECS,
        HISTOGRAM_INTERVAL_SECS / 60
    );

    println!("\n── First Bucket ──────────────────────────────────────────────────────────");
    println!("  date_bin snaps to : {}", ts_str(first_bucket_start));
    println!("  query start_time  : {}", ts_str(query_start_us));
    println!("  offset into bucket: {}s", offset_secs);
    println!(
        "  first bucket fill : {:.1}%  ← this causes the drop",
        first_bucket_pct
    );

    println!("\n── Per-severity counts (first bucket vs full bucket) ─────────────────────");
    println!(
        "  {:<8}  {:>12}  {:>12}  {:>8}",
        "severity", "first_bucket", "full_bucket", "% full"
    );
    println!("  {}", "─".repeat(50));
    for (sev, rate) in SEVERITIES {
        let first = first_steps * rate;
        let full = STEPS_PER_FULL_BUCKET * rate;
        println!(
            "  {:<8}  {:>12}  {:>12}  {:>7.1}%",
            sev, first, full, first_bucket_pct
        );
    }

    println!("\n── Dashboard SQL (multi-line with severity breakdown) ────────────────────");
    println!("  SELECT histogram(_timestamp) AS zo_sql_key,");
    println!("         severity AS zo_sql_breakdown,");
    println!("         count(*) AS zo_sql_num");
    println!("  FROM \"{}\"", cfg_stream());
    println!("  GROUP BY zo_sql_key, zo_sql_breakdown");
    println!("  ORDER BY zo_sql_key");

    println!("\n── Single-line SQL (total, no breakdown) ─────────────────────────────────");
    println!("  SELECT histogram(_timestamp) AS zo_sql_key, count(*) AS zo_sql_num");
    println!("  FROM \"{}\"", cfg_stream());
    println!("  GROUP BY zo_sql_key ORDER BY zo_sql_key");

    println!("\n── Set time range in dashboard ───────────────────────────────────────────");
    println!("  from : {}", ts_str(query_start_us));
    println!("  to   : {}", ts_str(query_end_us));

    let total_records = DATA_WINDOW_HOURS * 3600 / RECORD_INTERVAL_SECS
        * SEVERITIES.iter().map(|(_, r)| r).sum::<i64>();
    println!(
        "\n── Data to ingest: {} records ({} h, records from midnight)",
        total_records, DATA_WINDOW_HOURS
    );
    println!(
        "── Query window  : {} h starting at T+25s into bucket → first bucket = 0",
        QUERY_WINDOW_HOURS
    );
    println!();
}

// ── Generate ──────────────────────────────────────────────────────────────────

// data_start_us/data_end_us = full 24h window to ingest
// Records are aligned to RECORD_INTERVAL from midnight, so within every 30s bucket
// there are records at T+0s, T+10s, T+20s — but NOT at T+25s (our query start).
fn generate(data_start_us: i64, data_end_us: i64) -> std::io::Result<usize> {
    let file = File::create(OUTPUT_FILE)?;
    let mut writer = BufWriter::new(file);
    let mut rng = rand::thread_rng();
    let mut count = 0usize;

    writer.write_all(b"[")?;

    let mut ts = data_start_us;
    let mut first = true;

    while ts < data_end_us {
        // Emit N records per severity at this timestamp — each severity has a
        // fixed rate so every full bucket shows the same count per line.
        for (severity, rate) in SEVERITIES {
            for _ in 0..*rate {
                let record = SimRecord {
                    _timestamp: ts,
                    service: "histogram-edge-demo",
                    namespace: "demo",
                    scenario: "left_edge_drop",
                    severity,
                    requests_per_second: 500.0 + rng.gen_range(-2.0_f64..2.0_f64),
                    cpu_millicores: 300,
                    memory_mb: 512,
                    response_time_ms: 50.0 + rng.gen_range(-1.0_f64..1.0_f64),
                    error_rate: 0.001,
                    message: "request processed",
                };
                if !first {
                    writer.write_all(b",")?;
                }
                serde_json::to_writer(&mut writer, &record)?;
                first = false;
                count += 1;
            }
        }
        ts += RECORD_INTERVAL_US;
    }

    writer.write_all(b"]")?;
    writer.flush()?;
    Ok(count)
}

// ── Ingest ────────────────────────────────────────────────────────────────────

async fn ingest() -> Result<(), Box<dyn std::error::Error>> {
    let api_base = cfg_api_base();
    let org = cfg_org();
    let stream = cfg_stream();
    let url = format!("{}/api/{}/{}/_json", api_base, org, stream);
    println!("Ingesting {} → {}", OUTPUT_FILE, url);

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let file = File::open(OUTPUT_FILE)?;
    let reader = BufReader::new(file);
    let records: Vec<serde_json::Value> = serde_json::from_reader(reader)?;
    let total = records.len();
    let mut sent = 0usize;

    for chunk in records.chunks(INGEST_BATCH_SIZE) {
        let resp = client
            .post(&url)
            .basic_auth(cfg_username(), Some(cfg_password()))
            .json(&chunk)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            eprintln!("Batch failed ({}): {}", status, text);
            std::process::exit(1);
        }
        sent += chunk.len();
        println!("  sent {}/{}", sent, total);
    }

    println!("Done. {} records ingested into {}/{}", sent, org, stream);
    Ok(())
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("preview");

    // Data runs from midnight today for DATA_WINDOW_HOURS.
    // Records are at 10s intervals aligned from midnight — so within every 30s
    // bucket there are records at T+0s, T+10s, T+20s from the bucket boundary.
    let now_us = Utc::now().timestamp_micros();
    let now_secs = now_us / 1_000_000;
    let midnight_secs = now_secs - (now_secs % 86400);
    let data_start_us = midnight_secs * 1_000_000;
    let data_end_us = data_start_us + DATA_WINDOW_HOURS * 3600 * 1_000_000;

    // Query window: start 8h into the day, then push forward by misalignment so
    // query_start lands at T+25s inside a bucket (between the T+20s and T+30s record).
    // → no records exist in [query_start, next_bucket_boundary) → first bucket = 0.
    let eight_hours_us = 8 * 3600 * 1_000_000_i64;
    let bucket_boundary = snap_down(data_start_us + eight_hours_us);
    let query_start_us = bucket_boundary + START_MISALIGNMENT_SECS * 1_000_000;
    let query_end_us = query_start_us + QUERY_WINDOW_HOURS * 3600 * 1_000_000;

    match cmd {
        "preview" => {
            preview(query_start_us, query_end_us);
        }
        "generate" => {
            preview(query_start_us, query_end_us);
            print!("Writing {} ...", OUTPUT_FILE);
            std::io::stdout().flush().unwrap();
            match generate(data_start_us, data_end_us) {
                Ok(n) => println!(" {} records written.", n),
                Err(e) => eprintln!(" error: {}", e),
            }
        }
        "ingest" => {
            if let Err(e) = ingest().await {
                eprintln!("Ingest error: {}", e);
                std::process::exit(1);
            }
        }
        "full" => {
            preview(query_start_us, query_end_us);
            print!("Writing {} ...", OUTPUT_FILE);
            std::io::stdout().flush().unwrap();
            match generate(data_start_us, data_end_us) {
                Ok(n) => {
                    println!(" {} records written.", n);
                    if let Err(e) = ingest().await {
                        eprintln!("Ingest error: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!(" error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("usage: histogram_edge_sim [preview|generate|ingest|full]");
            std::process::exit(1);
        }
    }
}
