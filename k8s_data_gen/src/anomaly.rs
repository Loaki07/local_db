use rand::Rng;

#[derive(Debug, Clone, PartialEq)]
pub enum AnomalyType {
    Cpu,
    Memory,
    Errors,
    Restarts,
    Latency,
    Login,
}

impl AnomalyType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "cpu" => Some(AnomalyType::Cpu),
            "memory" => Some(AnomalyType::Memory),
            "errors" => Some(AnomalyType::Errors),
            "restarts" => Some(AnomalyType::Restarts),
            "latency" => Some(AnomalyType::Latency),
            "login" => Some(AnomalyType::Login),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AnomalyType::Cpu => "cpu",
            AnomalyType::Memory => "memory",
            AnomalyType::Errors => "errors",
            AnomalyType::Restarts => "restarts",
            AnomalyType::Latency => "latency",
            AnomalyType::Login => "login",
        }
    }
}

pub struct AnomalyState {
    pub anomaly_type: AnomalyType,
    pub remaining_secs: u32,
    pub cooldown_secs: u32,
}

impl AnomalyState {
    pub fn new(anomaly_type: AnomalyType) -> Self {
        AnomalyState {
            anomaly_type,
            remaining_secs: 0,
            cooldown_secs: 30,
        }
    }

    pub fn tick(&mut self, rng: &mut impl Rng) {
        if self.remaining_secs > 0 {
            self.remaining_secs -= 1;
            return;
        }
        if self.cooldown_secs > 0 {
            self.cooldown_secs -= 1;
            return;
        }
        if rng.gen_bool(0.10) {
            self.remaining_secs = rng.gen_range(120..=300);
            self.cooldown_secs = 120;
            println!(
                "[ANOMALY] {} spike started — {}s",
                self.anomaly_type.label(),
                self.remaining_secs
            );
        }
    }

    pub fn is_active(&self) -> bool {
        self.remaining_secs > 0
    }
}
