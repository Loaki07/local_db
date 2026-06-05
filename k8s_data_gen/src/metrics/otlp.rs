use super::types::K8sMetricRecord;

pub fn metric_record_to_resource_metrics(r: &K8sMetricRecord) -> serde_json::Value {
    let ts_ns = (r._timestamp * 1000).to_string();

    let attrs = serde_json::json!([
        {"key": "service",   "value": {"stringValue": r.service.as_str()}},
        {"key": "namespace", "value": {"stringValue": r.namespace.as_str()}},
        {"key": "pod",       "value": {"stringValue": r.pod.as_str()}},
        {"key": "node",      "value": {"stringValue": r.node.as_str()}},
        {"key": "cluster",   "value": {"stringValue": r.cluster.as_str()}},
    ]);

    serde_json::json!({
        "resource": {
            "attributes": [
                {"key": "service.name",        "value": {"stringValue": r.service.as_str()}},
                {"key": "k8s.namespace.name",  "value": {"stringValue": r.namespace.as_str()}},
                {"key": "k8s.pod.name",        "value": {"stringValue": r.pod.as_str()}},
                {"key": "k8s.node.name",       "value": {"stringValue": r.node.as_str()}},
                {"key": "k8s.cluster.name",    "value": {"stringValue": r.cluster.as_str()}},
            ]
        },
        "scopeMetrics": [{
            "scope": {"name": "k8s-data-gen"},
            "metrics": [
                {"name": "total_payment_req", "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts_ns, "asInt": r.total_payment_req}]}},
            ]
        }]
    })
}

pub fn metrics_to_otlp_payload(records: &[K8sMetricRecord]) -> serde_json::Value {
    serde_json::json!({
        "resourceMetrics": records.iter().map(metric_record_to_resource_metrics).collect::<Vec<_>>()
    })
}
