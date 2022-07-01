use std::cmp::min;
use std::collections::{BTreeMap, HashMap};
use std::ops::Div;
use std::time::SystemTime;
use actix::{Actor, Context, Handler, Message, MessageResult};
use crate::agent::metrics::timer::{ActionOutcome, ExecutionInfo};
use super::timer::Timer;

#[derive(Default)]
pub struct Metrics {
    histogram: [u64; 20],
    sum: u64,
    count: u64,
}

#[derive(Default)]
pub struct MetricsStorageActor {
    histogram: HashMap<ActionOutcome, Metrics>,
    pending: BTreeMap<SystemTime, Vec<Timer>>,
}

impl Actor for MetricsStorageActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("MetricsStorageActor started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        log::debug!("MetricsStorageActor stopped");
    }
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
                if timers.iter().any(|t| t.get_execution().is_none()) {
                    fst_incomplete_ts.insert(*ts);
                    true
                } else {
                    for timer in timers {
                        if let Some(execution) = timer.get_execution() {
                            let status = self.histogram.entry(execution.outcome).or_default();
                            let cs = execution.elapsed.as_millis().div(10) as u64;
                            let idx = Some(cs)
                                .filter(|cs| *cs > 0)
                                .map(|cs| min(127 - cs.leading_zeros(), 19) as usize)
                                .unwrap_or(0);

                            status.histogram[idx] += 1;
                            status.count += 1;
                            status.sum += cs;
                        } else {
                            log::error!("Non executed timer found during executed timers processing!");
                        }
                    }
                    false
                }
            } else {
                true
            }
        });
    }
}

pub struct StartedTimer { pub id: u32, pub timestamp: SystemTime }

#[derive(Message)]
#[rtype(result = "StartedTimer")]
pub struct StartTimer;

impl Handler<StartTimer> for MetricsStorageActor {
    type Result = MessageResult<StartTimer>;

    fn handle(&mut self, _: StartTimer, _ctx: &mut Self::Context) -> Self::Result {
        let now = SystemTime::now();
        let timers = self.pending
            .entry(now)
            .or_insert_with(Vec::new);
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

    fn handle(&mut self, msg: StopTimer, ctx: &mut Self::Context) -> Self::Result {
        if let Some(timer) = self.get_timer_mut(msg.timer.timestamp, msg.timer.id) {
            timer.set_execution(msg.execution.elapsed, msg.execution.outcome);
            self.process_pending();
        } else {
            log::error!("No timer found with ts {:?} and id {}", msg.timer.timestamp, msg.timer.id);
        }
    }
}