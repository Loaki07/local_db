pub mod flows;
pub mod generate;
pub mod historical;
pub mod live;
pub mod otlp;
pub mod types;

pub use flows::generate_prod_trace;
pub use generate::generate_trace_spans;
pub use historical::run_historical_traces;
pub use live::{run_live_traces, run_live_traces_grpc};
pub use otlp::{prod_spans_to_resource_spans, traces_to_otlp_payload};
pub use types::{K8sTraceRecord, ProdSpan};
