use std::collections::HashMap;

use actix::{Actor, Context, Handler, ResponseFuture};
use futures::future::join_all;
use futures::FutureExt;

use crate::agent::metrics::manager::message::{
    ActionMetricsFamilySnapshot, ActionTimerError, FetchActionMetrics, StartActionTimer,
    StartedActionTimer, StopActionTimer, StorageKey,
};
use crate::agent::metrics::storage::facade::MetricsStorage;
use crate::agent::metrics::storage::message::{FetchMetrics, StartedTimer};

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

impl Handler<StartActionTimer> for MetricsManagerActor {
    type Result = ResponseFuture<Result<StartedActionTimer, ActionTimerError>>;

    fn handle(&mut self, msg: StartActionTimer, _ctx: &mut Self::Context) -> Self::Result {
        let key = StorageKey {
            model: msg.model,
            action: msg.action,
        };
        let metrics_storage = self.storages.entry(key.clone()).or_default();
        let out = metrics_storage.start_timer();
        Box::pin(async move {
            match out.await {
                Ok(StartedTimer { id, timestamp }) => Ok(StartedActionTimer { key, id, timestamp }),
                Err(err) => Err(ActionTimerError::InternalError(err.to_string())),
            }
        })
    }
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
