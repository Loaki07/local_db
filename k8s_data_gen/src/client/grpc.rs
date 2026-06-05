use base64::Engine as _;
use opentelemetry_proto::tonic::{
    collector::trace::v1::{trace_service_client::TraceServiceClient, ExportTraceServiceRequest},
    trace::v1::ResourceSpans,
};
use tonic::transport::Channel;

use crate::config::{password, username};

pub async fn grpc_client(
    endpoint: &str,
) -> Result<TraceServiceClient<Channel>, Box<dyn std::error::Error>> {
    let tls = tonic::transport::ClientTlsConfig::new().with_webpki_roots();
    let ch = Channel::from_shared(endpoint.to_string())
        .map_err(|e| format!("invalid endpoint '{}': {}", endpoint, e))?
        .tls_config(tls)
        .map_err(|e| format!("tls config failed: {}", e))?
        .connect()
        .await
        .map_err(|e| format!("connect to '{}' failed: {}", endpoint, e))?;
    Ok(TraceServiceClient::new(ch))
}

pub async fn send_grpc_traces(
    client: &mut TraceServiceClient<Channel>,
    resource_spans: Vec<ResourceSpans>,
    org: &str,
    stream_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username(), password()))
    );
    let mut req = tonic::Request::new(ExportTraceServiceRequest { resource_spans });
    let md = req.metadata_mut();
    md.insert("organization", org.parse()?);
    md.insert("authorization", auth.parse()?);
    md.insert("stream-name", stream_name.parse()?);
    client.export(req).await?;
    Ok(())
}
