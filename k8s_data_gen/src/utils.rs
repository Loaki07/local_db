use rand::Rng;

use crate::anomaly::AnomalyState;
use crate::topology::PODS;

pub fn weighted_choice<'a>(choices: &[(&'a str, u32)], rng: &mut impl Rng) -> &'a str {
    let total: u32 = choices.iter().map(|(_, w)| w).sum();
    let mut pick = rng.gen_range(0..total);
    for (item, weight) in choices {
        if pick < *weight {
            return item;
        }
        pick -= weight;
    }
    choices.last().unwrap().0
}

/// Sinusoidal daily multiplier — peaks at noon, troughs at 4am.
pub fn daily_seasonal(timestamp_us: i64, amplitude: f64) -> f64 {
    use std::f64::consts::PI;
    let secs = timestamp_us / 1_000_000;
    let hour_f = (secs % 86400) as f64 / 3600.0;
    1.0 + amplitude * (2.0 * PI * (hour_f - 6.0) / 24.0).sin()
}

pub fn pod_name(pod_idx: usize) -> String {
    format!("{}-{:06x}", PODS[pod_idx].service, pod_idx * 0x1a3f + 0x4b7)
}

pub fn print_anomaly_header(anomaly_state: &Option<AnomalyState>) {
    match anomaly_state {
        Some(a) => println!(
            "Anomaly: {} (10% chance/sec to trigger 2–5 min spike)",
            a.anomaly_type.label()
        ),
        None => println!("No anomaly injection. Add --anomaly <type> to inject."),
    }
}
