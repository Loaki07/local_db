use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct K8sMetricRecord {
    pub _timestamp: i64,
    pub cluster: String,
    pub namespace: String,
    pub pod: String,
    pub node: String,
    pub service: String,
    pub cpu_millicores: u32,
    pub cpu_percent: f64,
    pub memory_mb: u32,
    pub memory_percent: f64,
    pub network_rx_bytes_per_sec: u64,
    pub network_tx_bytes_per_sec: u64,
    pub requests_per_second: f64,
    pub request_latency_ms: f64,
    pub error_rate: f64,
    pub restarts: u32,
}
