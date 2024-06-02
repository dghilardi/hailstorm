use super::actor::MetricsStorageActor;
use crate::agent::metrics::storage::message::{StartTimer, StartedTimer, StopTimer};
use crate::agent::metrics::timer::ExecutionInfo;
use actix::{Actor, Addr, MailboxError};
use std::future::Future;
use std::time::SystemTime;

pub(in crate::agent::metrics) struct MetricsStorage {
    pub(in crate::agent::metrics) ts_last_received_metric: SystemTime,
    pub(in crate::agent::metrics) addr: Addr<MetricsStorageActor>,
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
