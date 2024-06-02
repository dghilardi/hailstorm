use crate::agent::metrics::timer::{ActionOutcome, ExecutionInfo};
use actix::Message;
use std::collections::HashMap;
use std::time::SystemTime;
#[derive(Clone, Default)]
pub struct Metrics {
    pub histogram: [u64; 20],
    pub sum: u64,
}

pub type MetricsFamily = HashMap<ActionOutcome, Metrics>;
pub struct MetricsFamilySnapshot {
    pub timestamp: SystemTime,
    pub metrics: MetricsFamily,
}

pub struct StartedTimer {
    pub id: u32,
    pub timestamp: SystemTime,
}

#[derive(Message)]
#[rtype(result = "StartedTimer")]
pub struct StartTimer;

#[derive(Message)]
#[rtype(result = "()")]
pub struct StopTimer {
    pub timer: StartedTimer,
    pub execution: ExecutionInfo,
}

#[derive(Message)]
#[rtype(result = "Vec<MetricsFamilySnapshot>")]
pub struct FetchMetrics;
