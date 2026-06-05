use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct K8sLogRecord {
    pub _timestamp: i64,
    pub cluster: String,
    pub namespace: String,
    pub pod: String,
    pub container: String,
    pub node: String,
    pub service: String,
    pub cpu_millicores: u32,
    pub memory_mb: u32,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub restarts: u32,
    pub response_time_ms: f64,
    pub error_rate: f64,
    pub requests_per_second: u32,
    pub log_level: String,
    pub event_type: String,
    pub status_code: u16,
    pub message: String,
    pub unique_id: String,
}

pub const LOG_LEVELS_NORMAL: &[(&str, u32)] =
    &[("DEBUG", 10), ("INFO", 75), ("WARN", 12), ("ERROR", 3)];
pub const LOG_LEVELS_ERROR: &[(&str, u32)] =
    &[("DEBUG", 2), ("INFO", 10), ("WARN", 20), ("ERROR", 68)];

pub const EVENT_TYPES: &[&str] = &[
    "request",
    "request",
    "request",
    "request",
    "request",
    "request",
    "request",
    "healthcheck",
    "pod_lifecycle",
    "login",
];

pub const MESSAGES_INFO: &[&str] = &[
    "Processed request successfully",
    "Cache hit for key",
    "Health check passed",
    "Database query completed",
    "gRPC call returned OK",
    "Scheduled job ran",
    "Config reload triggered",
];

pub const MESSAGES_WARN: &[&str] = &[
    "Slow query detected, took >500ms",
    "Retry attempt 1 of 3",
    "Connection pool near capacity",
    "Rate limit threshold approaching",
    "Certificate expiring in 14 days",
];

pub const MESSAGES_ERROR: &[&str] = &[
    "Upstream service returned 503",
    "Database connection timeout",
    "Failed to parse response body",
    "OOMKilled: container exceeded memory limit",
    "Panic: index out of bounds",
    "TLS handshake failed",
    "Request queue overflow, dropping request",
];

pub const MESSAGES_LOGIN_OK: &[&str] = &[
    "User login successful",
    "Service account authenticated",
    "OAuth token validated",
    "API key auth successful",
];

pub const MESSAGES_LOGIN_ERROR: &[&str] = &[
    "login error: invalid credentials for user",
    "login error: account locked after repeated failures",
    "login error: token expired or revoked",
    "login error: IP blocked after too many attempts",
    "login error: MFA verification failed",
    "login error: service account rejected",
    "login error: brute force attempt detected",
];

/// ~0.5% of live records → ~3 login errors/min background noise for RCF training
pub const LOGIN_ERROR_BACKGROUND_PROB: f64 = 0.005;

/// 60% during historical so every 10s bucket has non-zero login error count
pub const HISTORICAL_LOGIN_ERROR_PROB: f64 = 0.60;
