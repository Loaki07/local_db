use std::collections::HashMap;

use opentelemetry_proto::tonic::{
    common::v1::{any_value::Value, AnyValue, InstrumentationScope, KeyValue},
    resource::v1::Resource,
    trace::v1::{ResourceSpans, ScopeSpans, Span, Status},
};

use super::types::{K8sTraceRecord, ProdSpan};

pub fn kv_str(k: &str, v: &str) -> KeyValue {
    KeyValue {
        key: k.to_string(),
        value: Some(AnyValue {
            value: Some(Value::StringValue(v.to_string())),
        }),
    }
}

pub fn kv_int(k: &str, v: i64) -> KeyValue {
    KeyValue {
        key: k.to_string(),
        value: Some(AnyValue {
            value: Some(Value::IntValue(v)),
        }),
    }
}

pub fn trace_record_to_resource_spans(s: &K8sTraceRecord) -> serde_json::Value {
    let start = (s._timestamp * 1000).to_string();
    let end = ((s._timestamp + s.duration_us) * 1000).to_string();
    let status_code: u8 = if s.status == "ERROR" { 2 } else { 1 };
    let kind: u8 = if s.is_root { 2 } else { 3 };
    serde_json::json!({
        "resource": {
            "attributes": [
                {"key": "service.name", "value": {"stringValue": s.service_name.as_str()}},
                {"key": "namespace",    "value": {"stringValue": s.namespace.as_str()}},
                {"key": "cluster",      "value": {"stringValue": s.cluster.as_str()}},
            ]
        },
        "scopeSpans": [{
            "scope": {"name": "k8s-data-gen"},
            "spans": [{
                "traceId":          s.trace_id.as_str(),
                "spanId":           s.span_id.as_str(),
                "parentSpanId":     s.parent_span_id.as_str(),
                "name":             s.operation_name.as_str(),
                "kind":             kind,
                "startTimeUnixNano": &start,
                "endTimeUnixNano":   &end,
                "status": {"code": status_code},
                "attributes": [
                    {"key": "http.status_code", "value": {"intValue": s.http_status_code as i64}},
                    {"key": "duration_ms",      "value": {"stringValue": s.duration_ms.to_string()}},
                    {"key": "status",           "value": {"stringValue": s.status.as_str()}},
                ]
            }]
        }]
    })
}

pub fn traces_to_otlp_payload(spans: &[K8sTraceRecord]) -> serde_json::Value {
    serde_json::json!({
        "resourceSpans": spans.iter().map(trace_record_to_resource_spans).collect::<Vec<_>>()
    })
}

/// Convert ProdSpan vec to protobuf ResourceSpans grouped by service.
pub fn prod_spans_to_resource_spans(spans: Vec<ProdSpan>) -> Vec<ResourceSpans> {
    let mut by_svc: HashMap<String, Vec<Span>> = HashMap::new();

    for s in &spans {
        let mut attrs = vec![
            kv_str("service.name", s.service_name),
            kv_str("k8s.namespace.name", s.namespace),
        ];
        if let Some(m) = s.http_method {
            attrs.push(kv_str("http.method", m));
            if s.http_status > 0 {
                attrs.push(kv_int("http.status_code", s.http_status as i64));
            }
        }
        if let Some(stmt) = s.db_statement {
            attrs.push(kv_str("db.statement", stmt));
            if let Some(sys) = s.db_system {
                attrs.push(kv_str("db.system", sys));
            }
        }

        let proto_span = Span {
            trace_id: s.trace_id.clone(),
            span_id: s.span_id.clone(),
            parent_span_id: s.parent_span_id.clone(),
            name: s.operation.to_string(),
            kind: s.kind,
            start_time_unix_nano: s.start_ns,
            end_time_unix_nano: s.end_ns,
            attributes: attrs,
            status: Some(Status {
                code: s.status_code,
                message: if s.status_code == 2 {
                    "Internal Error".to_string()
                } else {
                    String::new()
                },
            }),
            ..Default::default()
        };
        by_svc
            .entry(s.service_name.to_string())
            .or_default()
            .push(proto_span);
    }

    by_svc
        .into_iter()
        .map(|(svc, proto_spans)| ResourceSpans {
            resource: Some(Resource {
                attributes: vec![kv_str("service.name", &svc)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "k8s-data-gen".to_string(),
                    version: "0.1.0".to_string(),
                    ..Default::default()
                }),
                spans: proto_spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        })
        .collect()
}
