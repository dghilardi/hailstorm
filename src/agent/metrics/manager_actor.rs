use actix::{Actor, Addr, Context, Handler, MailboxError, Message, ResponseFuture};
use futures::future::join_all;
use futures::FutureExt;
use std::collections::HashMap;
use std::future::Future;
use std::time::SystemTime;

use crate::agent::metrics::storage_actor::{
    FetchMetrics, MetricsFamilySnapshot, MetricsStorageActor, StartTimer, StartedTimer, StopTimer,
};
use crate::agent::metrics::timer::ExecutionInfo;
use thiserror::Error;

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct StorageKey {
    pub model: String,
    pub action: String,
}

pub struct MetricsStorage {
    ts_last_received_metric: SystemTime,
    addr: Addr<MetricsStorageActor>,
}

impl MetricsStorage {
    pub fn start_timer(&mut self) -> impl Future<Output = Result<StartedTimer, MailboxError>> {
        self.ts_last_received_metric = SystemTime::now();
        self.addr.send(StartTimer)
    }
    pub fn stop_timer(
        &mut self,
        timer: StartedTimer,
        execution: ExecutionInfo,
    ) -> impl Future<Output = Result<(), MailboxError>> {
        self.ts_last_received_metric = SystemTime::now();
        self.addr.send(StopTimer { timer, execution })
    }
}

impl Default for MetricsStorage {
    fn default() -> Self {
        Self {
            ts_last_received_metric: SystemTime::now(),
            addr: MetricsStorageActor::start_default(),
        }
    }
}

#[derive(Default)]
pub struct MetricsManagerActor {
    storages: HashMap<StorageKey, MetricsStorage>,
}

impl Actor for MetricsManagerActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("MetricsManagerActor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::debug!("MetricsManagerActor stopped");
    }
}

#[derive(Debug, Error)]
pub enum ActionTimerError {
    #[error("Internal Error - {0}")]
    InternalError(String),
}

pub struct StartedActionTimer {
    id: u32,
    key: StorageKey,
    timestamp: SystemTime,
}

impl From<StartedActionTimer> for StartedTimer {
    fn from(t: StartedActionTimer) -> Self {
        Self {
            id: t.id,
            timestamp: t.timestamp,
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<StartedActionTimer, ActionTimerError>")]
pub struct StartActionTimer {
    pub model: String,
    pub action: String,
}

impl Handler<StartActionTimer> for MetricsManagerActor {
    type Result = ResponseFuture<Result<StartedActionTimer, ActionTimerError>>;

    fn handle(&mut self, msg: StartActionTimer, _ctx: &mut Self::Context) -> Self::Result {
        let key = StorageKey {
            model: msg.model,
            action: msg.action,
        };
        let metrics_storage = self
            .storages
            .entry(key.clone())
            .or_insert_with(Default::default);
        let out = metrics_storage.start_timer();
        Box::pin(async move {
            match out.await {
                Ok(StartedTimer { id, timestamp }) => Ok(StartedActionTimer { key, id, timestamp }),
                Err(err) => Err(ActionTimerError::InternalError(err.to_string())),
            }
        })
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), ActionTimerError>")]
pub struct StopActionTimer {
    pub timer: StartedActionTimer,
    pub execution: ExecutionInfo,
}

impl Handler<StopActionTimer> for MetricsManagerActor {
    type Result = ResponseFuture<Result<(), ActionTimerError>>;

    fn handle(
        &mut self,
        StopActionTimer { timer, execution }: StopActionTimer,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let stop_req = self
            .storages
            .get_mut(&timer.key)
            .map(|ms| ms.stop_timer(timer.into(), execution));
        Box::pin(async move {
            if let Some(fut) = stop_req {
                fut.await
                    .map_err(|err| ActionTimerError::InternalError(err.to_string()))
            } else {
                Err(ActionTimerError::InternalError(String::from(
                    "Metrics storage not found",
                )))
            }
        })
    }
}

pub struct ActionMetricsFamilySnapshot {
    pub key: StorageKey,
    pub metrics: Vec<MetricsFamilySnapshot>,
}

#[derive(Message)]
#[rtype(result = "Vec<ActionMetricsFamilySnapshot>")]
pub struct FetchActionMetrics;

impl Handler<FetchActionMetrics> for MetricsManagerActor {
    type Result = ResponseFuture<Vec<ActionMetricsFamilySnapshot>>;

    fn handle(&mut self, _msg: FetchActionMetrics, _ctx: &mut Self::Context) -> Self::Result {
        let fut_metrics = self
            .storages
            .iter()
            .map(|(key, storage)| {
                let key = key.clone();
                let fut = storage.addr.send(FetchMetrics);
                fut.map(|f| (key, f))
            })
            .collect::<Vec<_>>();
        Box::pin(async move {
            join_all(fut_metrics)
                .await
                .into_iter()
                .filter_map(|(key, res)| match res {
                    Ok(metrics) => Some((key, metrics)),
                    Err(err) => {
                        log::warn!("Error fetching perf metrics for {key:?} - {err}");
                        None
                    }
                })
                .map(|(key, metrics)| ActionMetricsFamilySnapshot { key, metrics })
                .collect()
        })
    }
}
