use super::super::timer::ExecutionInfo;
use crate::agent::metrics::storage::message::{
    FetchMetrics, MetricsFamily, MetricsFamilySnapshot, StartTimer, StartedTimer, StopTimer,
};
use actix::{Actor, Context, Handler, MessageResult};
use lazy_static::lazy_static;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;
use std::cmp::min;
use std::collections::{BTreeMap, HashSet};
use std::ops::Add;
use std::time::{Duration, SystemTime};

pub(super) struct MFSnapshotStorage {
    last_snapshot: Option<SystemTime>,
    buf_producer: ringbuf::HeapProd<MetricsFamilySnapshot>,
    buf_consumer: ringbuf::HeapCons<MetricsFamilySnapshot>,
}

impl Default for MFSnapshotStorage {
    fn default() -> Self {
        let buffer = HeapRb::new(60);
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
        if self.buf_producer.is_full() {
            self.buf_consumer.try_pop();
        }
        let out = self
            .buf_producer
            .try_push(MetricsFamilySnapshot { timestamp, metrics });
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
    histogram_dirty: bool,
    pending: BTreeMap<SystemTime, HashSet<u32>>,
    next_timer_id: u32,
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
    fn remove_timer(&mut self, ts: SystemTime, id: u32) -> bool {
        if let Some(timers) = self.pending.get_mut(&ts) {
            let removed = timers.remove(&id);
            if timers.is_empty() {
                self.pending.remove(&ts);
            }
            removed
        } else {
            false
        }
    }

    fn expire_pending(&mut self, now: SystemTime) {
        self.pending.retain(|ts, timers| {
            if ts.add(Duration::from_secs(3600)) > now {
                true
            } else {
                log::warn!("dropping {} expired timers at {ts:?}", timers.len());
                false
            }
        });
    }

    fn record_execution(&mut self, execution: ExecutionInfo) {
        // Use the exact duration for bucket selection; exported sums are milliseconds.
        let bucket_units = (execution.elapsed.as_nanos() + 9_999_999) / 10_000_000;
        let idx = compute_bucket_idx(bucket_units.min(u64::MAX as u128) as u64);
        let status = self.histogram.entry(execution.outcome).or_default();
        status.histogram[idx] += 1;
        status.sum += execution.elapsed.as_millis().min(u64::MAX as u128) as u64;
        self.histogram_dirty = true;

        let now = SystemTime::now();
        if self.snapshots.is_elapsed(*HIST_MAX_RES, now) {
            self.flush_snapshot(now);
        }
    }

    fn flush_snapshot(&mut self, timestamp: SystemTime) {
        if self.histogram_dirty {
            self.snapshots
                .add_snapshot(timestamp, self.histogram.clone());
            self.histogram_dirty = false;
        }
    }
}

fn compute_bucket_idx(value: u64) -> usize {
    Some(value)
        .filter(|cs| *cs > 0)
        .map(|cs| min(64 - (cs - 1).leading_zeros(), 19) as usize)
        .unwrap_or(0)
}

impl Handler<StartTimer> for MetricsStorageActor {
    type Result = MessageResult<StartTimer>;

    fn handle(&mut self, _: StartTimer, _ctx: &mut Self::Context) -> Self::Result {
        let now = SystemTime::now();
        let timers = self.pending.entry(now).or_default();
        let mut timer_id = self.next_timer_id;
        while !timers.insert(timer_id) {
            timer_id = timer_id.wrapping_add(1);
        }
        self.next_timer_id = timer_id.wrapping_add(1);
        MessageResult(StartedTimer {
            id: timer_id,
            timestamp: now,
        })
    }
}

impl Handler<StopTimer> for MetricsStorageActor {
    type Result = ();

    fn handle(&mut self, msg: StopTimer, _ctx: &mut Self::Context) -> Self::Result {
        if self.remove_timer(msg.timer.timestamp, msg.timer.id) {
            self.record_execution(msg.execution);
        } else {
            log::error!(
                "No timer found with ts {:?} and id {}",
                msg.timer.timestamp,
                msg.timer.id
            );
        }
    }
}

impl Handler<FetchMetrics> for MetricsStorageActor {
    type Result = MessageResult<FetchMetrics>;

    fn handle(&mut self, _msg: FetchMetrics, _ctx: &mut Self::Context) -> Self::Result {
        let now = SystemTime::now();
        self.expire_pending(now);
        self.flush_snapshot(now);
        let mut res = Vec::with_capacity(self.snapshots.buf_consumer.occupied_len());
        while let Some(snapshot) = self.snapshots.buf_consumer.try_pop() {
            res.push(snapshot)
        }
        MessageResult(res)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::agent::metrics::storage::actor::compute_bucket_idx;
    use crate::agent::metrics::timer::ExecutionInfo;

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

    #[test]
    fn a_full_snapshot_buffer_retains_the_newest_measurements() {
        let mut storage = MFSnapshotStorage::default();
        let origin = SystemTime::now();
        for offset in 0..61 {
            storage.add_snapshot(
                origin + Duration::from_secs(offset),
                MetricsFamily::default(),
            );
        }
        assert_eq!(storage.buf_consumer.occupied_len(), 60);
        assert_eq!(
            storage.buf_consumer.try_pop().unwrap().timestamp,
            origin + Duration::from_secs(1)
        );
        let mut last = None;
        while let Some(snapshot) = storage.buf_consumer.try_pop() {
            last = Some(snapshot.timestamp);
        }
        assert_eq!(last, Some(origin + Duration::from_secs(60)));
    }

    #[actix::test]
    async fn completed_measurements_do_not_wait_for_unfinished_timers() {
        let storage = MetricsStorageActor::start_default();
        let _unfinished = storage.send(StartTimer).await.unwrap();
        let timer = storage.send(StartTimer).await.unwrap();
        storage
            .send(StopTimer {
                timer,
                execution: ExecutionInfo {
                    elapsed: Duration::from_millis(19),
                    outcome: 0,
                },
            })
            .await
            .unwrap();

        let snapshots = storage.send(FetchMetrics).await.unwrap();
        let metrics = &snapshots
            .last()
            .expect("completed measurement must be published")
            .metrics[&0];
        assert_eq!(metrics.sum, 19);
        assert_eq!(metrics.histogram[1], 1);
        assert_eq!(metrics.histogram.iter().sum::<u64>(), 1);
    }

    #[actix::test]
    async fn fetching_flushes_measurements_after_the_last_snapshot() {
        let storage = MetricsStorageActor::start_default();
        for count in 1..=2 {
            let timer = storage.send(StartTimer).await.unwrap();
            storage
                .send(StopTimer {
                    timer,
                    execution: ExecutionInfo {
                        elapsed: Duration::from_millis(100),
                        outcome: 0,
                    },
                })
                .await
                .unwrap();
            let snapshots = storage.send(FetchMetrics).await.unwrap();
            let metrics = &snapshots
                .last()
                .expect("final measurements must be flushed")
                .metrics[&0];
            assert_eq!(metrics.sum, count * 100);
            assert_eq!(metrics.histogram.iter().sum::<u64>(), count);
        }
        assert!(storage.send(FetchMetrics).await.unwrap().is_empty());
    }

    #[actix::test]
    async fn histogram_boundaries_include_only_durations_below_the_limit() {
        let storage = MetricsStorageActor::start_default();
        for elapsed in [
            Duration::ZERO,
            Duration::from_millis(10),
            Duration::from_micros(10_001),
            Duration::from_millis(20),
            Duration::from_millis(21),
        ] {
            let timer = storage.send(StartTimer).await.unwrap();
            storage
                .send(StopTimer {
                    timer,
                    execution: ExecutionInfo {
                        elapsed,
                        outcome: 0,
                    },
                })
                .await
                .unwrap();
        }
        let snapshots = storage.send(FetchMetrics).await.unwrap();
        let metrics = &snapshots.last().unwrap().metrics[&0];
        assert_eq!(&metrics.histogram[..3], &[2, 2, 1]);
        assert_eq!(metrics.sum, 61);
    }

    #[actix::test]
    async fn duplicate_completions_do_not_count_the_same_timer_twice() {
        let storage = MetricsStorageActor::start_default();
        let timer = storage.send(StartTimer).await.unwrap();
        for _ in 0..2 {
            storage
                .send(StopTimer {
                    timer: StartedTimer {
                        id: timer.id,
                        timestamp: timer.timestamp,
                    },
                    execution: ExecutionInfo {
                        elapsed: Duration::from_millis(100),
                        outcome: 0,
                    },
                })
                .await
                .unwrap();
        }
        let snapshots = storage.send(FetchMetrics).await.unwrap();
        let metrics = &snapshots.last().unwrap().metrics[&0];
        assert_eq!(metrics.sum, 100);
        assert_eq!(metrics.histogram.iter().sum::<u64>(), 1);
    }

    #[test]
    fn expiration_keeps_recent_timers() {
        let now = SystemTime::now();
        let expired = now - Duration::from_secs(3601);
        let mut storage = MetricsStorageActor::default();
        storage.pending.insert(expired, [1].into_iter().collect());
        storage.pending.insert(now, [2].into_iter().collect());
        storage.expire_pending(now);
        assert!(!storage.pending.contains_key(&expired));
        assert_eq!(storage.pending[&now].len(), 1);
    }
}
