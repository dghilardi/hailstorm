use std::collections::HashMap;
use std::future::Future;
use std::time::SystemTime;
use actix::{Actor, Addr, Context, Handler, MailboxError, Message, MessageResult, ResponseFuture};
use thiserror::Error;
use crate::agent::metrics::storage_actor::{MetricsStorageActor, StartedTimer, StartTimer};
use crate::agent::metrics::timer::ExecutionInfo;

#[derive(Eq, Hash, PartialEq, Clone)]
pub struct StorageKey {
    model: String,
    action: String,
}

pub struct MetricsStorage {
    ts_last_received_metric: SystemTime,
    addr: Addr<MetricsStorageActor>
}

impl MetricsStorage {
    pub fn start_timer(&mut self) -> impl Future<Output=Result<StartedTimer, MailboxError>> {
        self.ts_last_received_metric = SystemTime::now();
        self.addr.send(StartTimer)
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

pub struct MetricsManagerActor {
    storages: HashMap<StorageKey, MetricsStorage>
}

impl Actor for MetricsManagerActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("MetricsManagerActor started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        log::debug!("MetricsManagerActor stopped");
    }
}

impl MetricsManagerActor {
    pub fn new() -> Self {
        Self { storages: Default::default() }
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

#[derive(Message)]
#[rtype(result = "Result<StartedActionTimer, ActionTimerError>")]
pub struct StartActionTimer {
    model: String,
    action: String,
}

impl Handler<StartActionTimer> for MetricsManagerActor {
    type Result = ResponseFuture<Result<StartedActionTimer, ActionTimerError>>;

    fn handle(&mut self, msg: StartActionTimer, ctx: &mut Self::Context) -> Self::Result {
        let key = StorageKey {
            model: msg.model,
            action: msg.action,
        };
        let metrics_storage = self.storages.entry(key.clone()).or_insert_with(Default::default);
        let out = metrics_storage.start_timer();
        Box::pin(async move {
            match out.await {
                Ok(StartedTimer { id, timestamp }) => Ok(StartedActionTimer {
                    key,
                    id,
                    timestamp,
                }),
                Err(err) => Err(ActionTimerError::InternalError(err.to_string())),
            }
        })
    }
}


#[derive(Message)]
#[rtype(result = "()")]
pub struct StopActionTimer {
    timer: StartedActionTimer,
    execution: ExecutionInfo,
}

impl Handler<StopActionTimer> for MetricsManagerActor {
    type Result = ();

    fn handle(&mut self, msg: StopActionTimer, ctx: &mut Self::Context) -> Self::Result {
        todo!()
    }
}