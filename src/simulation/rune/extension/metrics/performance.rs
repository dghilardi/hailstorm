use std::time::{Duration, Instant};
use actix::{Actor, Addr, Context, Handler, Recipient};
use rune::{Any, Value};
use rune::runtime::{Function, Shared, VmError};
use crate::agent::metrics::manager_actor::{StartActionTimer, StartedActionTimer, StopActionTimer};
use crate::agent::metrics::timer::{ActionOutcome, ExecutionInfo};

#[derive(Any)]
pub struct PerformanceRegistry {
    model: String,
    start_timer_recipient: Recipient<StartActionTimer>,
    stop_timer_recipient: Recipient<StopActionTimer>,
}

impl PerformanceRegistry {
    pub fn new<A>(model: String, metrics_addr: Addr<A>) -> Self
    where A: Actor<Context=Context<A>> +
        Handler<StartActionTimer> +
        Handler<StopActionTimer>
    {
        Self {
            model,
            start_timer_recipient: metrics_addr.clone().recipient(),
            stop_timer_recipient: metrics_addr.recipient(),
        }
    }

    async fn start_timer(&self, action: &str) -> Result<StartedActionTimer, VmError> {
        self.start_timer_recipient.send(StartActionTimer {
            model: self.model.clone(),
            action: action.to_string(),
        })
            .await
            .map_err(|err| VmError::panic(err))?
            .map_err(|err| VmError::panic(err))
    }

    async fn stop_timer(&self, timer: StartedActionTimer, elapsed: Duration, outcome: ActionOutcome) -> Result<(), VmError> {
        self.stop_timer_recipient.send(StopActionTimer {
            timer,
            execution: ExecutionInfo { elapsed, outcome },
        })
            .await
            .map_err(|err| VmError::panic(err))?
            .map_err(|err| VmError::panic(err))
    }

    pub async fn observe(&self, name: &str, action: Function) -> Result<i64, VmError> {
        let timer = self.start_timer(name).await?;
        let before = Instant::now();
        let res = action.async_send_call(()).await;
        let elapsed = before.elapsed();
        self.stop_timer(timer, elapsed, *res.as_ref().unwrap_or(&-1)).await?;
        res
    }
}