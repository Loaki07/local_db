/// K8s Data Generator — Kubernetes observability data for OpenObserve.
///
/// USAGE:
///   cargo run -- historical [--days N] [--stream logs|metrics|traces|all]
///   cargo run -- ingest [FILE] [--org ORG] [--stream STREAM]
///   cargo run -- live [--stream logs|metrics|traces] [--anomaly TYPE] [--grpc]
///   cargo run -- corr          # one-shot correlatable logs+metrics+traces
///   cargo run -- repro         # reproduce issue #1848
///
/// ANOMALY TYPES: cpu | memory | errors | restarts | latency | login
mod anomaly;
mod client;
mod config;
mod corr;
mod ingest;
mod logs;
mod metrics;
mod repro;
mod topology;
mod traces;
mod utils;

use anomaly::AnomalyType;
use config::{DEFAULT_ORG, DEFAULT_STREAM_LOGS};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "historical" => {
            let days = parse_flag_u32(&args, "--days").unwrap_or(7);
            let stream = parse_flag_str(&args, "--stream").unwrap_or_else(|| "logs".to_string());

            let result = match stream.as_str() {
                "logs" => logs::run_historical_logs(days),
                "metrics" => metrics::run_historical_metrics(days),
                "traces" => traces::run_historical_traces(days),
                "all" => logs::run_historical_logs(days)
                    .and_then(|_| metrics::run_historical_metrics(days))
                    .and_then(|_| traces::run_historical_traces(days)),
                other => {
                    eprintln!(
                        "Unknown stream '{}'. Valid: logs, metrics, traces, all",
                        other
                    );
                    std::process::exit(1);
                }
            };
            if let Err(e) = result {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        "ingest" => {
            let file_path = if args.len() > 2 && !args[2].starts_with("--") {
                args[2].as_str()
            } else {
                "../output_k8s.json"
            };
            let org = parse_flag_str(&args, "--org").unwrap_or_else(|| DEFAULT_ORG.to_string());
            let stream = parse_flag_str(&args, "--stream")
                .unwrap_or_else(|| DEFAULT_STREAM_LOGS.to_string());
            if let Err(e) = ingest::run_ingest(file_path, &org, &stream).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        "live" => {
            let stream = parse_flag_str(&args, "--stream").unwrap_or_else(|| "logs".to_string());
            let anomaly_type =
                parse_flag_str(&args, "--anomaly").and_then(|s| AnomalyType::from_str(&s));
            let use_grpc = args.contains(&"--grpc".to_string());

            if let Some(ref s) = parse_flag_str(&args, "--anomaly") {
                if AnomalyType::from_str(s).is_none() {
                    eprintln!("Unknown anomaly type '{}'. Valid: cpu, memory, errors, restarts, latency, login", s);
                    std::process::exit(1);
                }
            }

            let result = match stream.as_str() {
                "logs" => logs::run_live_logs(anomaly_type).await,
                "metrics" => metrics::run_live_metrics(anomaly_type).await,
                "traces" if use_grpc => traces::run_live_traces_grpc(anomaly_type).await,
                "traces" => traces::run_live_traces(anomaly_type).await,
                other => {
                    eprintln!("Unknown stream '{}'. Valid: logs, metrics, traces", other);
                    std::process::exit(1);
                }
            };
            if let Err(e) = result {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        "corr" => {
            if let Err(e) = corr::run_corr().await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        "repro" => {
            if let Err(e) = repro::run_repro().await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        "help" | "--help" | "-h" => print_usage(),
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}

fn parse_flag_u32(args: &[String], flag: &str) -> Option<u32> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w[1].parse().ok())
}

fn parse_flag_str(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn print_usage() {
    println!("k8s_data_gen — Kubernetes observability data generator\n");
    println!("USAGE:");
    println!("  cargo run -- historical [--days N] [--stream logs|metrics|traces|all]");
    println!("    logs    → ../output_k8s.json");
    println!("    metrics → ../output_k8s_metrics.json");
    println!("    traces  → ../output_k8s_traces.json");
    println!("    all     → all three\n");
    println!("  cargo run -- ingest [FILE] [--org ORG] [--stream STREAM]");
    println!("    k8s_logs    → /_json           (stream_type=logs)");
    println!("    k8s_metrics → /v1/metrics OTLP (stream_type=metrics)");
    println!("    k8s_traces  → /v1/traces OTLP  (stream_type=traces)\n");
    println!("  cargo run -- live [--stream logs|metrics|traces] [--anomaly TYPE] [--grpc]");
    println!("    --grpc: use gRPC OTLP for traces (port 5081, prod service flows)\n");
    println!("  cargo run -- corr");
    println!("    One-shot: correlatable logs+metrics+traces for 3 services.");
    println!("    All types share service.name+namespace — verifies normal correlation.\n");
    println!("  cargo run -- repro");
    println!("    Reproduce issue #1848: ECS-style logs (no service.name) + traces");
    println!("    with service.name, same namespace — shows split in Discovered Services.\n");
    println!("ANOMALY TYPES: cpu | memory | errors | restarts | latency | login\n");
    println!("EXAMPLES:");
    println!("  cargo run -- historical --days 7 --stream all");
    println!("  cargo run -- ingest ../output_k8s_metrics.json --stream k8s_metrics");
    println!("  cargo run -- ingest ../output_k8s_traces.json --stream k8s_traces");
    println!("  cargo run -- live --stream metrics --anomaly cpu");
    println!("  cargo run -- live --stream traces --grpc --anomaly latency");
    println!("  cargo run -- live --stream logs --anomaly login\n");
    println!("ANOMALY DETECTION CONFIGS:");
    println!("  Logs/CPU:           k8s_logs    → logs    → custom SQL AVG(cpu_millicores)");
    println!("  Logs/Errors:        k8s_logs    → logs    → count(*) filter log_level=ERROR");
    println!("  Logs/Latency:       k8s_logs    → logs    → custom SQL AVG(response_time_ms)");
    println!("  Logs/Restarts:      k8s_logs    → logs    → custom SQL SUM(restarts)");
    println!("  Logs/Login:         k8s_logs    → logs    → count(*) filter message~'login error'");
    println!("  Metrics/CPU:        cpu_percent        → metrics → avg(value)");
    println!("  Metrics/Memory:     memory_percent     → metrics → avg(value)");
    println!("  Metrics/Latency:    request_latency_ms → metrics → avg(value)");
    println!("  Metrics/ErrorRate:  error_rate         → metrics → avg(value)");
    println!("  Traces/Latency:     k8s_traces  → traces  → avg(duration_ms)");
    println!("  Traces/Errors:      k8s_traces  → traces  → count(*) filter status=ERROR");
}
