use super::timer::Timer;
use crate::agent::metrics::timer::{ActionOutcome, ExecutionInfo};
use actix::{Actor, Context, Handler, Message, MessageResult};
use lazy_static::lazy_static;
use ringbuf::RingBuffer;
use std::cmp::min;
use std::collections::{BTreeMap, HashMap};
use std::ops::{Add, Div};
use std::time::{Duration, SystemTime};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Clone, Default)]
pub struct Metrics {
    pub histogram: [u64; 20],
    pub sum: u64,
}

type MetricsFamily = HashMap<ActionOutcome, Metrics>;

pub struct MetricsFamilySnapshot {
    pub timestamp: SystemTime,
    pub metrics: MetricsFamily,
}

pub struct MFSnapshotStorage {
    last_snapshot: Option<SystemTime>,
    buf_producer: ringbuf::Producer<MetricsFamilySnapshot>,
    buf_consumer: ringbuf::Consumer<MetricsFamilySnapshot>,
}

impl Default for MFSnapshotStorage {
    fn default() -> Self {
        let buffer = RingBuffer::new(60);
        let (buf_producer, buf_consumer) = buffer.split();
        Self {
            last_snapshot: None,
            buf_producer,
            buf_consumer,
        }
    }
}

impl MFSnapshotStorage {
    pub fn add_snapshot(&mut self, timestamp: SystemTime, metrics: MetricsFamily) {
        let out = self
            .buf_producer
            .push(MetricsFamilySnapshot { timestamp, metrics });
        if let Err(MetricsFamilySnapshot { timestamp, .. }) = out {
            log::error!("Error saving metrics snapshot {:?}", timestamp);
        } else {
            self.last_snapshot = Some(timestamp);
        }
    }

    pub fn is_elapsed(&self, delta: Duration, query_ts: SystemTime) -> bool {
        if let Some(ref last_ts) = self.last_snapshot {
            last_ts.add(delta) < query_ts
        } else {
            true
        }
    }
}

#[derive(Default)]
pub struct MetricsStorageActor {
    snapshots: MFSnapshotStorage,
    histogram: MetricsFamily,
    pending: BTreeMap<SystemTime, Vec<Timer>>,
}

impl Actor for MetricsStorageActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("MetricsStorageActor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::debug!("MetricsStorageActor stopped");
    }
}

lazy_static! {
    static ref HIST_MAX_RES: Duration = Duration::from_secs(5);
}

impl MetricsStorageActor {
    fn get_timer_mut(&mut self, ts: SystemTime, id: u32) -> Option<&mut Timer> {
        self.pending
            .get_mut(&ts)
            .and_then(|v| v.iter_mut().find(|t| id.eq(&t.get_id())))
    }

    fn process_pending(&mut self) {
        let mut fst_incomplete_ts: Option<SystemTime> = None;
        self.pending.retain(|ts, timers| {
            if fst_incomplete_ts.map(|fst_ts| fst_ts > *ts).unwrap_or(true) {
                if ts.add(Duration::from_secs(3600)) > SystemTime::now()
                    && timers.iter().any(|t| t.get_execution().is_none())
                {
                    fst_incomplete_ts = Some(*ts);
                    true
                } else {
                    for timer in timers {
                        if let Some(execution) = timer.get_execution() {
                            let status = self.histogram.entry(execution.outcome).or_default();
                            let cs = execution.elapsed.as_millis().div(10) as u64;
                            let idx = compute_bucket_idx(cs);

                            status.histogram[idx] += 1;
                            status.sum += cs;
                        } else {
                            log::warn!(
                                "dropping pending timer '{}'",
                                OffsetDateTime::from(*ts)
                                    .format(&Rfc3339)
                                    .unwrap_or_default()
                            );
                        }
                    }
                    if self.snapshots.is_elapsed(*HIST_MAX_RES, *ts) {
                        self.snapshots.add_snapshot(*ts, self.histogram.clone());
                    }
                    false
                }
            } else {
                true
            }
        });
    }
}

fn compute_bucket_idx(value: u64) -> usize {
    Some(value)
        .filter(|cs| *cs > 0)
        .map(|cs| min(64 - (cs - 1).leading_zeros(), 19) as usize)
        .unwrap_or(0)
}

pub struct StartedTimer {
    pub id: u32,
    pub timestamp: SystemTime,
}

#[derive(Message)]
#[rtype(result = "StartedTimer")]
pub struct StartTimer;

impl Handler<StartTimer> for MetricsStorageActor {
    type Result = MessageResult<StartTimer>;

    fn handle(&mut self, _: StartTimer, _ctx: &mut Self::Context) -> Self::Result {
        let now = SystemTime::now();
        let timers = self.pending.entry(now).or_insert_with(Vec::new);
        let timer_id = timers.len() as u32;
        timers.push(Timer::empty(timer_id));
        MessageResult(StartedTimer {
            id: timer_id,
            timestamp: now,
        })
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StopTimer {
    pub timer: StartedTimer,
    pub execution: ExecutionInfo,
}

impl Handler<StopTimer> for MetricsStorageActor {
    type Result = ();

    fn handle(&mut self, msg: StopTimer, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(timer) = self.get_timer_mut(msg.timer.timestamp, msg.timer.id) {
            timer.set_execution(msg.execution.elapsed, msg.execution.outcome);
            self.process_pending();
        } else {
            log::error!(
                "No timer found with ts {:?} and id {}",
                msg.timer.timestamp,
                msg.timer.id
            );
        }
    }
}

#[derive(Message)]
#[rtype(result = "Vec<MetricsFamilySnapshot>")]
pub struct FetchMetrics;

impl Handler<FetchMetrics> for MetricsStorageActor {
    type Result = MessageResult<FetchMetrics>;

    fn handle(&mut self, _msg: FetchMetrics, _ctx: &mut Self::Context) -> Self::Result {
        let mut res = Vec::with_capacity(self.snapshots.buf_consumer.len());
        while let Some(snapshot) = self.snapshots.buf_consumer.pop() {
            res.push(snapshot)
        }
        MessageResult(res)
    }
}

#[cfg(test)]
mod test {
    use crate::agent::metrics::storage_actor::compute_bucket_idx;

    #[test]
    fn test_compute_bucket_idx() {
        for v in 0..100 {
            let idx = compute_bucket_idx(v);
            assert!(v <= 2u64.pow(idx as u32), "v = {v}, idx = {idx}");
            assert!(
                idx == 0 || v > 2u64.pow(idx as u32 - 1),
                "v = {v}, idx = {idx}"
            );
        }
    }
}
