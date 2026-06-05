use rand::Rng;

use super::types::ProdSpan;
use crate::anomaly::{AnomalyState, AnomalyType};

pub fn rspan_id(rng: &mut impl Rng) -> Vec<u8> {
    (0..8).map(|_| rng.gen::<u8>()).collect()
}

pub fn rtrace_id(rng: &mut impl Rng) -> Vec<u8> {
    (0..16).map(|_| rng.gen::<u8>()).collect()
}

pub fn lat(base_us: u64, jitter_us: u64, mult: f64, rng: &mut impl Rng) -> u64 {
    ((base_us + rng.gen_range(0..=jitter_us)) as f64 * mult).max(1.0) as u64
}

#[allow(clippy::too_many_arguments)]
pub fn mk(
    tid: Vec<u8>,
    sid: Vec<u8>,
    pid: Vec<u8>,
    svc: &'static str,
    ns: &'static str,
    op: &'static str,
    start_us: u64,
    dur_us: u64,
    kind: i32,
    error: bool,
    http_method: Option<&'static str>,
    http_status: u32,
    db_statement: Option<&'static str>,
    db_system: Option<&'static str>,
) -> ProdSpan {
    ProdSpan {
        trace_id: tid,
        span_id: sid,
        parent_span_id: pid,
        service_name: svc,
        namespace: ns,
        operation: op,
        http_method,
        http_status: if error && http_method.is_some() {
            500
        } else {
            http_status
        },
        db_statement,
        db_system,
        start_ns: start_us * 1000,
        end_ns: (start_us + dur_us) * 1000,
        status_code: if error { 2 } else { 1 },
        kind,
    }
}

/// Dispatch one request to the appropriate flow.
/// checkout 35% | search 30% | login 15% | browse 20%
pub fn generate_prod_trace(
    now_us: u64,
    anomaly: Option<&AnomalyState>,
    rng: &mut impl Rng,
) -> Vec<ProdSpan> {
    let tid = rtrace_id(rng);
    let lm = if anomaly
        .map(|a| matches!(a.anomaly_type, AnomalyType::Latency) && a.is_active())
        .unwrap_or(false)
    {
        rng.gen_range(15.0_f64..40.0)
    } else {
        1.0
    };
    let err = anomaly
        .map(|a| matches!(a.anomaly_type, AnomalyType::Errors) && a.is_active())
        .unwrap_or(false);

    match rng.gen_range(0u8..100) {
        0..=34 => flow_checkout(tid, now_us, lm, err, rng),
        35..=64 => flow_search(tid, now_us, lm, err, rng),
        65..=79 => flow_login(tid, now_us, lm, err, rng),
        _ => flow_browse(tid, now_us, lm, err, rng),
    }
}

// ── Flow: checkout ────────────────────────────────────────────────────────────
// api-gateway → auth → cart → inventory → payment → order → notification

fn flow_checkout(
    tid: Vec<u8>,
    base_us: u64,
    lm: f64,
    err: bool,
    rng: &mut impl Rng,
) -> Vec<ProdSpan> {
    let mut out = Vec::new();
    let mut t = base_us;

    let root_id = rspan_id(rng);
    let root_dur = lat(600_000, 500_000, lm, rng);
    let root_err = err && rng.gen_bool(0.45);
    out.push(mk(
        tid.clone(),
        root_id.clone(),
        vec![],
        "api-gateway",
        "gateway",
        "POST /api/v1/checkout",
        t,
        root_dur,
        2,
        root_err,
        Some("POST"),
        200,
        None,
        None,
    ));
    t += 4_000;

    let auth_id = rspan_id(rng);
    let auth_dur = lat(22_000, 18_000, lm, rng);
    out.push(mk(
        tid.clone(),
        auth_id.clone(),
        root_id.clone(),
        "auth-service",
        "auth",
        "ValidateJWT",
        t,
        auth_dur,
        2,
        false,
        None,
        0,
        None,
        None,
    ));
    out.push(mk(
        tid.clone(),
        rspan_id(rng),
        auth_id,
        "redis-cache",
        "infra",
        "GET session:*",
        t + 1_000,
        lat(1_500, 1_500, lm, rng),
        3,
        false,
        None,
        0,
        Some("GET session:{token}"),
        Some("redis"),
    ));
    t += auth_dur + 4_000;

    let cart_id = rspan_id(rng);
    let cache_hit = rng.gen_bool(0.45);
    let cart_dur = if cache_hit {
        lat(18_000, 8_000, lm, rng)
    } else {
        lat(55_000, 35_000, lm, rng)
    };
    out.push(mk(
        tid.clone(),
        cart_id.clone(),
        root_id.clone(),
        "cart-service",
        "commerce",
        "GetCart",
        t,
        cart_dur,
        2,
        false,
        None,
        0,
        None,
        None,
    ));
    let redis_dur = lat(1_500, 1_000, lm, rng);
    out.push(mk(
        tid.clone(),
        rspan_id(rng),
        cart_id.clone(),
        "redis-cache",
        "infra",
        "GET cart:*",
        t + 1_000,
        redis_dur,
        3,
        false,
        None,
        0,
        Some("GET cart:{user_id}"),
        Some("redis"),
    ));
    if !cache_hit {
        out.push(mk(
            tid.clone(),
            rspan_id(rng),
            cart_id,
            "postgres-primary",
            "infra",
            "SELECT cart_items",
            t + redis_dur + 2_000,
            lat(14_000, 10_000, lm, rng),
            3,
            false,
            None,
            0,
            Some("SELECT * FROM cart_items WHERE user_id = $1"),
            Some("postgresql"),
        ));
    }
    t += cart_dur + 4_000;

    let inv_id = rspan_id(rng);
    let inv_dur = lat(35_000, 25_000, lm, rng);
    let inv_err = err && rng.gen_bool(0.2);
    out.push(mk(
        tid.clone(),
        inv_id.clone(),
        root_id.clone(),
        "inventory-service",
        "commerce",
        "CheckStock",
        t,
        inv_dur,
        2,
        inv_err,
        None,
        0,
        None,
        None,
    ));
    out.push(mk(
        tid.clone(),
        rspan_id(rng),
        inv_id,
        "postgres-primary",
        "infra",
        "SELECT inventory",
        t + 2_000,
        lat(9_000, 7_000, lm, rng),
        3,
        false,
        None,
        0,
        Some("SELECT qty FROM inventory WHERE sku = $1 FOR UPDATE"),
        Some("postgresql"),
    ));
    t += inv_dur + 4_000;

    let pay_id = rspan_id(rng);
    let pay_dur = lat(140_000, 220_000, lm, rng);
    let pay_err = err && rng.gen_bool(0.6);
    out.push(mk(
        tid.clone(),
        pay_id.clone(),
        root_id.clone(),
        "payment-service",
        "payments",
        "ProcessPayment",
        t,
        pay_dur,
        2,
        pay_err,
        Some("POST"),
        200,
        None,
        None,
    ));
    out.push(mk(
        tid.clone(),
        rspan_id(rng),
        pay_id,
        "stripe-api",
        "external",
        "POST /v1/charges",
        t + 5_000,
        lat(110_000, 200_000, lm, rng),
        3,
        pay_err,
        Some("POST"),
        if pay_err { 402 } else { 200 },
        None,
        None,
    ));
    t += pay_dur + 4_000;

    if !pay_err {
        let ord_id = rspan_id(rng);
        let ord_dur = lat(55_000, 35_000, lm, rng);
        out.push(mk(
            tid.clone(),
            ord_id.clone(),
            root_id.clone(),
            "order-service",
            "commerce",
            "CreateOrder",
            t,
            ord_dur,
            2,
            false,
            None,
            0,
            None,
            None,
        ));
        out.push(mk(
            tid.clone(),
            rspan_id(rng),
            ord_id,
            "postgres-primary",
            "infra",
            "INSERT orders",
            t + 2_000,
            lat(11_000, 8_000, lm, rng),
            3,
            false,
            None,
            0,
            Some("INSERT INTO orders (user_id, items, total) VALUES ($1,$2,$3)"),
            Some("postgresql"),
        ));
        t += ord_dur + 4_000;

        out.push(mk(
            tid.clone(),
            rspan_id(rng),
            root_id.clone(),
            "notification-service",
            "notify",
            "SendOrderConfirmation",
            t,
            lat(18_000, 12_000, 1.0, rng),
            3,
            false,
            None,
            0,
            None,
            None,
        ));
    }

    out
}

// ── Flow: product-search ──────────────────────────────────────────────────────
// api-gateway → search-service → [redis | product-catalog → postgres-replica]

fn flow_search(
    tid: Vec<u8>,
    base_us: u64,
    lm: f64,
    err: bool,
    rng: &mut impl Rng,
) -> Vec<ProdSpan> {
    let mut out = Vec::new();
    let t = base_us;

    let root_id = rspan_id(rng);
    let cache_hit = rng.gen_bool(0.55);
    let root_dur = if cache_hit {
        lat(45_000, 25_000, lm, rng)
    } else {
        lat(120_000, 80_000, lm, rng)
    };
    out.push(mk(
        tid.clone(),
        root_id.clone(),
        vec![],
        "api-gateway",
        "gateway",
        "GET /api/v1/search",
        t,
        root_dur,
        2,
        false,
        Some("GET"),
        200,
        None,
        None,
    ));

    let srch_id = rspan_id(rng);
    let srch_dur = if cache_hit {
        lat(35_000, 15_000, lm, rng)
    } else {
        lat(100_000, 60_000, lm, rng)
    };
    let srch_err = err && rng.gen_bool(0.3);
    out.push(mk(
        tid.clone(),
        srch_id.clone(),
        root_id,
        "search-service",
        "search",
        "Search",
        t + 3_000,
        srch_dur,
        2,
        srch_err,
        None,
        0,
        None,
        None,
    ));

    let redis_dur = lat(1_500, 1_000, lm, rng);
    out.push(mk(
        tid.clone(),
        rspan_id(rng),
        srch_id.clone(),
        "redis-cache",
        "infra",
        "GET search:*",
        t + 4_000,
        redis_dur,
        3,
        false,
        None,
        0,
        Some("GET search:{query_hash}"),
        Some("redis"),
    ));

    if !cache_hit {
        let cat_id = rspan_id(rng);
        let cat_dur = lat(55_000, 40_000, lm, rng);
        out.push(mk(
            tid.clone(),
            cat_id.clone(),
            srch_id,
            "product-catalog",
            "catalog",
            "ListProducts",
            t + 4_000 + redis_dur + 2_000,
            cat_dur,
            2,
            false,
            None,
            0,
            None,
            None,
        ));
        out.push(mk(tid.clone(), rspan_id(rng), cat_id, "postgres-replica", "infra", "SELECT products",
            t + 4_000 + redis_dur + 4_000, lat(18_000, 14_000, lm, rng), 3, false, None, 0,
            Some("SELECT id,name,price,stock FROM products WHERE tsv @@ plainto_tsquery($1) LIMIT 50"),
            Some("postgresql")));
    }

    out
}

// ── Flow: login ───────────────────────────────────────────────────────────────
// api-gateway → auth-service → user-service → postgres + redis (session write)

fn flow_login(tid: Vec<u8>, base_us: u64, lm: f64, err: bool, rng: &mut impl Rng) -> Vec<ProdSpan> {
    let mut out = Vec::new();
    let t = base_us;

    let root_id = rspan_id(rng);
    let root_dur = lat(70_000, 50_000, lm, rng);
    let login_err = err && rng.gen_bool(0.35);
    out.push(mk(
        tid.clone(),
        root_id.clone(),
        vec![],
        "api-gateway",
        "gateway",
        "POST /api/v1/auth/login",
        t,
        root_dur,
        2,
        login_err,
        Some("POST"),
        if login_err { 401 } else { 200 },
        None,
        None,
    ));

    let auth_id = rspan_id(rng);
    let auth_dur = lat(55_000, 35_000, lm, rng);
    out.push(mk(
        tid.clone(),
        auth_id.clone(),
        root_id,
        "auth-service",
        "auth",
        "Login",
        t + 3_000,
        auth_dur,
        2,
        login_err,
        None,
        0,
        None,
        None,
    ));

    let usr_id = rspan_id(rng);
    let usr_dur = lat(20_000, 12_000, lm, rng);
    out.push(mk(
        tid.clone(),
        usr_id.clone(),
        auth_id.clone(),
        "user-service",
        "users",
        "GetUserByEmail",
        t + 4_000,
        usr_dur,
        2,
        false,
        None,
        0,
        None,
        None,
    ));
    out.push(mk(
        tid.clone(),
        rspan_id(rng),
        usr_id,
        "postgres-primary",
        "infra",
        "SELECT users",
        t + 5_000,
        lat(8_000, 6_000, lm, rng),
        3,
        false,
        None,
        0,
        Some("SELECT id,email,password_hash,role FROM users WHERE email = $1"),
        Some("postgresql"),
    ));

    if !login_err {
        out.push(mk(
            tid.clone(),
            rspan_id(rng),
            auth_id,
            "redis-cache",
            "infra",
            "SET session:*",
            t + 4_000 + usr_dur + 2_000,
            lat(1_500, 1_000, lm, rng),
            3,
            false,
            None,
            0,
            Some("SET session:{token} {user_json} EX 86400"),
            Some("redis"),
        ));
    }

    out
}

// ── Flow: browse-product ──────────────────────────────────────────────────────
// api-gateway → product-catalog → [redis hit | postgres-replica]

fn flow_browse(
    tid: Vec<u8>,
    base_us: u64,
    lm: f64,
    _err: bool,
    rng: &mut impl Rng,
) -> Vec<ProdSpan> {
    let mut out = Vec::new();
    let t = base_us;

    let root_id = rspan_id(rng);
    let cache_hit = rng.gen_bool(0.65);
    let root_dur = if cache_hit {
        lat(22_000, 10_000, lm, rng)
    } else {
        lat(65_000, 40_000, lm, rng)
    };
    out.push(mk(
        tid.clone(),
        root_id.clone(),
        vec![],
        "api-gateway",
        "gateway",
        "GET /api/v1/products/:id",
        t,
        root_dur,
        2,
        false,
        Some("GET"),
        200,
        None,
        None,
    ));

    let cat_id = rspan_id(rng);
    let cat_dur = if cache_hit {
        lat(14_000, 6_000, lm, rng)
    } else {
        lat(48_000, 28_000, lm, rng)
    };
    out.push(mk(
        tid.clone(),
        cat_id.clone(),
        root_id,
        "product-catalog",
        "catalog",
        "GetProduct",
        t + 3_000,
        cat_dur,
        2,
        false,
        None,
        0,
        None,
        None,
    ));

    let redis_dur = lat(1_500, 1_000, lm, rng);
    out.push(mk(
        tid.clone(),
        rspan_id(rng),
        cat_id.clone(),
        "redis-cache",
        "infra",
        "GET product:*",
        t + 4_000,
        redis_dur,
        3,
        false,
        None,
        0,
        Some("GET product:{id}"),
        Some("redis"),
    ));

    if !cache_hit {
        out.push(mk(
            tid,
            rspan_id(rng),
            cat_id,
            "postgres-replica",
            "infra",
            "SELECT product",
            t + 4_000 + redis_dur + 2_000,
            lat(10_000, 8_000, lm, rng),
            3,
            false,
            None,
            0,
            Some("SELECT * FROM products WHERE id = $1"),
            Some("postgresql"),
        ));
    }

    out
}
