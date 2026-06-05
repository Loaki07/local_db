use super::types::K8sMetricRecord;

pub fn metric_record_to_resource_metrics(r: &K8sMetricRecord) -> serde_json::Value {
    let ts = (r._timestamp * 1000).to_string(); // microseconds → nanoseconds
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
                {"key": "service.name", "value": {"stringValue": r.service.as_str()}},
                {"key": "namespace",    "value": {"stringValue": r.namespace.as_str()}},
                {"key": "pod",          "value": {"stringValue": r.pod.as_str()}},
                {"key": "node",         "value": {"stringValue": r.node.as_str()}},
                {"key": "cluster",      "value": {"stringValue": r.cluster.as_str()}},
            ]
        },
        "scopeMetrics": [{
            "scope": {"name": "k8s-data-gen"},
            "metrics": [
                {"name": "cpu_millicores",           "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.cpu_millicores as f64}]}},
                {"name": "cpu_percent",              "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.cpu_percent}]}},
                {"name": "memory_mb",                "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.memory_mb as f64}]}},
                {"name": "memory_percent",           "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.memory_percent}]}},
                {"name": "request_latency_ms",       "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.request_latency_ms}]}},
                {"name": "error_rate",               "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.error_rate}]}},
                {"name": "requests_per_second",      "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.requests_per_second}]}},
                {"name": "network_rx_bytes_per_sec", "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.network_rx_bytes_per_sec as f64}]}},
                {"name": "network_tx_bytes_per_sec", "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.network_tx_bytes_per_sec as f64}]}},
                {"name": "restarts",                 "gauge": {"dataPoints": [{"attributes": &attrs, "timeUnixNano": &ts, "asDouble": r.restarts as f64}]}},
            ]
        }]
    })
}

pub fn metrics_to_otlp_payload(records: &[K8sMetricRecord]) -> serde_json::Value {
    serde_json::json!({
        "resourceMetrics": records.iter().map(metric_record_to_resource_metrics).collect::<Vec<_>>()
    })
}
