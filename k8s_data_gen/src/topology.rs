pub struct Pod {
    pub namespace: &'static str,
    pub service: &'static str,
    pub container: &'static str,
    pub base_cpu: u32,
    pub base_mem: u32,
    pub base_rps: u32,
    pub base_rt: f64,
    pub base_err: f64,
}

pub const PODS: &[Pod] = &[
    Pod {
        namespace: "payments",
        service: "payments-api",
        container: "api",
        base_cpu: 350,
        base_mem: 512,
        base_rps: 220,
        base_rt: 45.0,
        base_err: 0.005,
    },
    Pod {
        namespace: "payments",
        service: "payments-worker",
        container: "worker",
        base_cpu: 180,
        base_mem: 256,
        base_rps: 80,
        base_rt: 30.0,
        base_err: 0.002,
    },
    Pod {
        namespace: "inventory",
        service: "inventory-service",
        container: "service",
        base_cpu: 280,
        base_mem: 384,
        base_rps: 150,
        base_rt: 60.0,
        base_err: 0.008,
    },
    Pod {
        namespace: "inventory",
        service: "inventory-db",
        container: "postgres",
        base_cpu: 420,
        base_mem: 768,
        base_rps: 50,
        base_rt: 12.0,
        base_err: 0.001,
    },
    Pod {
        namespace: "frontend",
        service: "web-server",
        container: "nginx",
        base_cpu: 120,
        base_mem: 128,
        base_rps: 800,
        base_rt: 8.0,
        base_err: 0.003,
    },
    Pod {
        namespace: "frontend",
        service: "static-cdn",
        container: "cdn",
        base_cpu: 90,
        base_mem: 96,
        base_rps: 600,
        base_rt: 5.0,
        base_err: 0.001,
    },
    Pod {
        namespace: "monitoring",
        service: "prometheus",
        container: "prometheus",
        base_cpu: 460,
        base_mem: 900,
        base_rps: 20,
        base_rt: 25.0,
        base_err: 0.000,
    },
    Pod {
        namespace: "monitoring",
        service: "grafana",
        container: "grafana",
        base_cpu: 200,
        base_mem: 320,
        base_rps: 40,
        base_rt: 120.0,
        base_err: 0.002,
    },
    Pod {
        namespace: "infra",
        service: "nginx-ingress",
        container: "controller",
        base_cpu: 310,
        base_mem: 256,
        base_rps: 1200,
        base_rt: 3.0,
        base_err: 0.004,
    },
    Pod {
        namespace: "infra",
        service: "coredns",
        container: "coredns",
        base_cpu: 150,
        base_mem: 192,
        base_rps: 400,
        base_rt: 2.0,
        base_err: 0.000,
    },
];

pub const CLUSTERS: &[&str] = &["prod-us-east-1", "prod-eu-west-1", "staging-us-west-2"];
pub const NODES: &[&str] = &["node-1", "node-2", "node-3", "node-4", "node-5"];

pub const TRACE_OPS: &[(&str, &[&str])] = &[
    (
        "payments-api",
        &[
            "POST /checkout",
            "GET /payment-methods",
            "POST /refund",
            "GET /balance",
        ],
    ),
    (
        "payments-worker",
        &["process_payment", "reconcile_batch", "send_notification"],
    ),
    (
        "inventory-service",
        &[
            "GET /products",
            "GET /stock",
            "PUT /reserve",
            "POST /restock",
        ],
    ),
    (
        "inventory-db",
        &["SELECT products", "UPDATE stock", "INSERT order_item"],
    ),
    (
        "web-server",
        &["GET /", "GET /products", "GET /cart", "POST /checkout"],
    ),
    (
        "static-cdn",
        &["GET /static/js", "GET /static/css", "GET /images"],
    ),
    (
        "prometheus",
        &["scrape_metrics", "evaluate_rules", "query_range"],
    ),
    (
        "grafana",
        &["dashboard_load", "panel_query", "alert_evaluate"],
    ),
    (
        "nginx-ingress",
        &["ROUTE /api", "ROUTE /static", "TLS_HANDSHAKE"],
    ),
    (
        "coredns",
        &["resolve_internal", "resolve_external", "cache_hit"],
    ),
];
