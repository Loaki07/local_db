use rand::{seq::SliceRandom, Rng};
use uuid::Uuid;

use super::types::K8sTraceRecord;
use crate::anomaly::{AnomalyState, AnomalyType};
use crate::topology::{CLUSTERS, PODS, TRACE_OPS};
use crate::utils::daily_seasonal;

/// Generates a root span + optional child spans for one synthetic request.
pub fn generate_trace_spans(
    pod_idx: usize,
    timestamp_us: i64,
    anomaly: Option<&AnomalyState>,
    rng: &mut impl Rng,
) -> Vec<K8sTraceRecord> {
    let pod = &PODS[pod_idx];
    let cluster = CLUSTERS[rng.gen_range(0..CLUSTERS.len())];

    let is_anomaly = anomaly.map(|a| a.is_active()).unwrap_or(false);
    let anomaly_type = anomaly.map(|a| &a.anomaly_type);
    let season = daily_seasonal(timestamp_us, 0.20);

    let ops = TRACE_OPS
        .iter()
        .find(|(svc, _)| *svc == pod.service)
        .map(|(_, ops)| *ops)
        .unwrap_or(&["unknown"]);
    let operation = ops[rng.gen_range(0..ops.len())];

    let base_duration = if is_anomaly && anomaly_type == Some(&AnomalyType::Latency) {
        pod.base_rt * rng.gen_range(15.0..40.0) * (1.0 + rng.gen_range(-0.30_f64..=0.30))
    } else {
        (pod.base_rt * season * (1.0 + rng.gen_range(-0.25_f64..=0.25))).max(0.5)
    };

    let is_error = if is_anomaly && anomaly_type == Some(&AnomalyType::Errors) {
        rng.gen_bool(0.60)
    } else {
        rng.gen_bool(pod.base_err.min(0.05))
    };

    let status = if is_error { "ERROR" } else { "OK" };
    let http_status: u16 = if is_error {
        *[500u16, 502, 503, 504][..].choose(rng).unwrap()
    } else {
        *[200u16, 200, 200, 201, 204][..].choose(rng).unwrap()
    };

    let trace_id = Uuid::new_v4().simple().to_string();
    let root_span_id = Uuid::new_v4().simple().to_string()[..16].to_string();
    let duration_us = (base_duration * 1000.0) as i64;

    let mut spans = vec![K8sTraceRecord {
        _timestamp: timestamp_us,
        trace_id: trace_id.clone(),
        span_id: root_span_id.clone(),
        parent_span_id: String::new(),
        service_name: pod.service.to_string(),
        namespace: pod.namespace.to_string(),
        cluster: cluster.to_string(),
        operation_name: operation.to_string(),
        duration_us,
        duration_ms: (base_duration * 10.0).round() / 10.0,
        status: status.to_string(),
        http_status_code: http_status,
        is_root: true,
    }];

    let has_children = matches!(
        pod.service,
        "payments-api" | "web-server" | "inventory-service" | "nginx-ingress"
    );
    if has_children && rng.gen_bool(0.7) {
        for _ in 0..rng.gen_range(1..=2_usize) {
            let child_dur = base_duration * rng.gen_range(0.3..0.8);
            let child_offset_us = rng.gen_range(0..duration_us / 2);
            let child_span_id = Uuid::new_v4().simple().to_string()[..16].to_string();
            let downstream = &PODS[rng.gen_range(0..PODS.len())];

            spans.push(K8sTraceRecord {
                _timestamp: timestamp_us + child_offset_us,
                trace_id: trace_id.clone(),
                span_id: child_span_id,
                parent_span_id: root_span_id.clone(),
                service_name: downstream.service.to_string(),
                namespace: downstream.namespace.to_string(),
                cluster: cluster.to_string(),
                operation_name: format!("call_{}", downstream.service.replace('-', "_")),
                duration_us: (child_dur * 1000.0) as i64,
                duration_ms: (child_dur * 10.0).round() / 10.0,
                status: if is_error && rng.gen_bool(0.5) {
                    "ERROR"
                } else {
                    "OK"
                }
                .to_string(),
                http_status_code: if is_error && rng.gen_bool(0.5) {
                    500
                } else {
                    200
                },
                is_root: false,
            });
        }
    }

    spans
}
