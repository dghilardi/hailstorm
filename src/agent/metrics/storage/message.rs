use crate::agent::metrics::timer::{ActionOutcome, ExecutionInfo};
use actix::Message;
use std::collections::HashMap;
use std::time::SystemTime;
/// Histogram-based metrics for a single action outcome.
///
/// The `histogram` array contains 20 logarithmically-spaced buckets tracking
/// response time distribution. `sum` accumulates the total centiseconds across
/// all observations.
#[derive(Clone, Default)]
pub struct Metrics {
    /// 20-bucket histogram of response times (logarithmic scale, units of 10ms).
    pub histogram: [u64; 20],
    /// Sum of all observed values in centiseconds.
    pub sum: u64,
}

/// A collection of [`Metrics`] keyed by action outcome (e.g., HTTP status code).
pub type MetricsFamily = HashMap<ActionOutcome, Metrics>;

/// A time-stamped snapshot of a [`MetricsFamily`].
pub struct MetricsFamilySnapshot {
    /// When this snapshot was captured.
    pub timestamp: SystemTime,
    /// The metrics data at the time of capture.
    pub metrics: MetricsFamily,
}

/// Identifies a started timer by its ID and creation timestamp.
pub struct StartedTimer {
    /// Timer identifier (unique within a given timestamp).
    pub id: u32,
    /// When the timer was started.
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
