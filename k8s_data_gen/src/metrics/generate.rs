use rand::Rng;

use super::types::K8sMetricRecord;
use crate::anomaly::{AnomalyState, AnomalyType};
use crate::config::NODE_MEMORY_MB;
use crate::topology::{CLUSTERS, NODES, PODS};
use crate::utils::{daily_seasonal, pod_name};

pub fn generate_metric_record(
    pod_idx: usize,
    timestamp_us: i64,
    anomaly: Option<&AnomalyState>,
    rng: &mut impl Rng,
) -> K8sMetricRecord {
    let pod = &PODS[pod_idx];
    let cluster = CLUSTERS[rng.gen_range(0..CLUSTERS.len())];
    let node = NODES[pod_idx % NODES.len()];

    let is_anomaly = anomaly.map(|a| a.is_active()).unwrap_or(false);
    let anomaly_type = anomaly.map(|a| &a.anomaly_type);
    let season = daily_seasonal(timestamp_us, 0.25);

    let cpu_mc = if is_anomaly && anomaly_type == Some(&AnomalyType::Cpu) {
        (pod.base_cpu as f64 * rng.gen_range(4.0..7.0)) as u32
    } else {
        (pod.base_cpu as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))).max(1.0) as u32
    };

    let memory_mb = if is_anomaly && anomaly_type == Some(&AnomalyType::Memory) {
        (pod.base_mem as f64 * rng.gen_range(3.5..5.0)) as u32
    } else {
        (pod.base_mem as f64 * (1.0 + rng.gen_range(-0.15_f64..=0.15))) as u32
    };

    let latency = if is_anomaly && anomaly_type == Some(&AnomalyType::Latency) {
        pod.base_rt * rng.gen_range(15.0..40.0) * (1.0 + rng.gen_range(-0.30_f64..=0.30))
    } else {
        (pod.base_rt * season * (1.0 + rng.gen_range(-0.25_f64..=0.25))).max(0.5)
    };

    let error_rate = if is_anomaly && anomaly_type == Some(&AnomalyType::Errors) {
        rng.gen_range(0.30..0.80_f64)
    } else {
        (pod.base_err * (1.0 + rng.gen_range(-0.50_f64..=0.50))).clamp(0.0, 0.05)
    };

    let restarts: u32 = if is_anomaly && anomaly_type == Some(&AnomalyType::Restarts) {
        rng.gen_range(5..=15)
    } else if rng.gen_bool(0.002) {
        1
    } else {
        0
    };

    let rps = (pod.base_rps as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))).max(0.0);
    let net_rx = (rps as u64).saturating_mul(rng.gen_range(800..1200));
    let net_tx = (rps as u64).saturating_mul(rng.gen_range(400..800));

    K8sMetricRecord {
        _timestamp: timestamp_us,
        cluster: cluster.to_string(),
        namespace: pod.namespace.to_string(),
        pod: pod_name(pod_idx),
        node: node.to_string(),
        service: pod.service.to_string(),
        cpu_millicores: cpu_mc,
        cpu_percent: ((cpu_mc as f64 / 10.0) * 100.0).round() / 100.0,
        memory_mb,
        memory_percent: ((memory_mb as f64 / NODE_MEMORY_MB) * 10000.0).round() / 100.0,
        network_rx_bytes_per_sec: net_rx,
        network_tx_bytes_per_sec: net_tx,
        requests_per_second: (rps * 10.0).round() / 10.0,
        request_latency_ms: (latency * 10.0).round() / 10.0,
        error_rate: (error_rate * 10000.0).round() / 10000.0,
        restarts,
    }
}
