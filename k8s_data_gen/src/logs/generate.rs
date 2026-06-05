use rand::{seq::SliceRandom, Rng};
use uuid::Uuid;

use super::types::*;
use crate::anomaly::{AnomalyState, AnomalyType};
use crate::topology::{CLUSTERS, NODES, PODS};
use crate::utils::{daily_seasonal, pod_name, weighted_choice};

pub fn generate_log_record(
    pod_idx: usize,
    timestamp_us: i64,
    anomaly: Option<&AnomalyState>,
    login_error_prob: f64,
    rng: &mut impl Rng,
) -> K8sLogRecord {
    let pod = &PODS[pod_idx];
    let cluster = CLUSTERS[rng.gen_range(0..CLUSTERS.len())];
    let node = NODES[pod_idx % NODES.len()];
    let pname = pod_name(pod_idx);

    let is_anomaly = anomaly.map(|a| a.is_active()).unwrap_or(false);
    let anomaly_type = anomaly.map(|a| &a.anomaly_type);
    let season = daily_seasonal(timestamp_us, 0.20);

    let cpu = if is_anomaly && anomaly_type == Some(&AnomalyType::Cpu) {
        (pod.base_cpu as f64 * rng.gen_range(4.0..7.0)) as u32
    } else {
        (pod.base_cpu as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))) as u32
    };

    let memory = if is_anomaly && anomaly_type == Some(&AnomalyType::Memory) {
        (pod.base_mem as f64 * rng.gen_range(3.5..5.0)) as u32
    } else {
        (pod.base_mem as f64 * (1.0 + rng.gen_range(-0.15_f64..=0.15))) as u32
    };

    let restarts: u32 = if is_anomaly && anomaly_type == Some(&AnomalyType::Restarts) {
        rng.gen_range(5..=15)
    } else if rng.gen_bool(0.002) {
        1
    } else {
        0
    };

    let response_time = if is_anomaly && anomaly_type == Some(&AnomalyType::Latency) {
        pod.base_rt * rng.gen_range(15.0..40.0) * (1.0 + rng.gen_range(-0.30_f64..=0.30))
    } else {
        (pod.base_rt * season * (1.0 + rng.gen_range(-0.25_f64..=0.25))).max(1.0)
    };

    let error_rate = if is_anomaly && anomaly_type == Some(&AnomalyType::Errors) {
        rng.gen_range(0.30..0.80_f64)
    } else {
        (pod.base_err * (1.0 + rng.gen_range(-0.50_f64..=0.50))).clamp(0.0, 0.05)
    };

    let rps = (pod.base_rps as f64 * season * (1.0 + rng.gen_range(-0.20_f64..=0.20))) as u32;
    let net_rx = rps as u64 * rng.gen_range(800..1200);
    let net_tx = rps as u64 * rng.gen_range(400..800);

    let is_error_anomaly = is_anomaly && anomaly_type == Some(&AnomalyType::Errors);
    let is_login_anomaly = is_anomaly && anomaly_type == Some(&AnomalyType::Login);

    let log_level = weighted_choice(
        if is_error_anomaly {
            LOG_LEVELS_ERROR
        } else {
            LOG_LEVELS_NORMAL
        },
        rng,
    );

    let (event_type, message) = if is_login_anomaly {
        (
            "login_error",
            MESSAGES_LOGIN_ERROR[rng.gen_range(0..MESSAGES_LOGIN_ERROR.len())].to_string(),
        )
    } else if rng.gen_bool(login_error_prob) {
        (
            "login_error",
            MESSAGES_LOGIN_ERROR[rng.gen_range(0..MESSAGES_LOGIN_ERROR.len())].to_string(),
        )
    } else {
        let et = EVENT_TYPES[rng.gen_range(0..EVENT_TYPES.len())];
        let msg = if et == "login" {
            MESSAGES_LOGIN_OK[rng.gen_range(0..MESSAGES_LOGIN_OK.len())].to_string()
        } else {
            match log_level {
                "ERROR" => MESSAGES_ERROR[rng.gen_range(0..MESSAGES_ERROR.len())].to_string(),
                "WARN" => MESSAGES_WARN[rng.gen_range(0..MESSAGES_WARN.len())].to_string(),
                _ => MESSAGES_INFO[rng.gen_range(0..MESSAGES_INFO.len())].to_string(),
            }
        };
        (et, msg)
    };

    let status_code: u16 = if error_rate > 0.20 {
        *[500u16, 502, 503, 504][..].choose(rng).unwrap()
    } else if error_rate > 0.05 {
        if rng.gen_bool(0.5) {
            429
        } else {
            500
        }
    } else {
        *[200u16, 200, 200, 200, 201, 204, 304][..]
            .choose(rng)
            .unwrap()
    };

    K8sLogRecord {
        _timestamp: timestamp_us,
        cluster: cluster.to_string(),
        namespace: pod.namespace.to_string(),
        pod: pname,
        container: pod.container.to_string(),
        node: node.to_string(),
        service: pod.service.to_string(),
        cpu_millicores: cpu,
        memory_mb: memory,
        network_rx_bytes: net_rx,
        network_tx_bytes: net_tx,
        restarts,
        response_time_ms: (response_time * 10.0).round() / 10.0,
        error_rate: (error_rate * 1000.0).round() / 1000.0,
        requests_per_second: rps,
        log_level: log_level.to_string(),
        event_type: event_type.to_string(),
        status_code,
        message,
        unique_id: Uuid::new_v4().to_string(),
    }
}
