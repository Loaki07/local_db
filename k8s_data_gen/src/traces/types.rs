use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct K8sTraceRecord {
    pub _timestamp: i64,
    pub trace_id: String,
    pub span_id: String,
    /// Empty string = root span
    pub parent_span_id: String,
    pub service_name: String,
    pub namespace: String,
    pub cluster: String,
    pub operation_name: String,
    pub duration_us: i64,
    pub duration_ms: f64,
    pub status: String,
    pub http_status_code: u16,
    pub is_root: bool,
}

/// Span model for production distributed traces (gRPC OTLP path).
pub struct ProdSpan {
    pub trace_id: Vec<u8>,
    pub span_id: Vec<u8>,
    pub parent_span_id: Vec<u8>,
    pub service_name: &'static str,
    pub namespace: &'static str,
    pub operation: &'static str,
    pub http_method: Option<&'static str>,
    pub http_status: u32,
    pub db_statement: Option<&'static str>,
    pub db_system: Option<&'static str>,
    pub start_ns: u64,
    pub end_ns: u64,
    pub status_code: i32, // 1=OK 2=ERROR
    pub kind: i32,        // 2=SERVER 3=CLIENT
}
