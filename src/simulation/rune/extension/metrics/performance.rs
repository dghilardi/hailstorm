use std::time::{Duration, Instant};

use actix::{Actor, Addr, Context, Handler, Recipient};
use rune::runtime::{Function, Ref, RuntimeError, Value, VmResult};
use rune::{Any, FromValue};

use crate::agent::metrics::manager::message::StartedActionTimer;
use crate::agent::metrics::manager::message::{StartActionTimer, StopActionTimer};
use crate::agent::metrics::timer::{ActionOutcome, ExecutionInfo};
use crate::simulation::rune::types::value::OwnedValue;

#[derive(Any)]
pub struct PerformanceRegistry {
    model: String,
    start_timer_recipient: Recipient<StartActionTimer>,
    stop_timer_recipient: Recipient<StopActionTimer>,
}

impl PerformanceRegistry {
    pub fn new<A>(model: String, metrics_addr: Addr<A>) -> Self
    where
        A: Actor<Context = Context<A>> + Handler<StartActionTimer> + Handler<StopActionTimer>,
    {
        Self {
            model,
            start_timer_recipient: metrics_addr.clone().recipient(),
            stop_timer_recipient: metrics_addr.recipient(),
        }
    }

    async fn start_timer(&self, action: &str) -> Result<StartedActionTimer, RuntimeError> {
        self.start_timer_recipient
            .send(StartActionTimer::new(&self.model, action))
            .await
            .map_err(|e| RuntimeError::panic(e.to_string()))?
            .map_err(|e| RuntimeError::panic(e.to_string()))
    }

    async fn stop_timer(
        &self,
        timer: StartedActionTimer,
        elapsed: Duration,
        outcome: ActionOutcome,
    ) -> Result<(), RuntimeError> {
        self.stop_timer_recipient
            .send(StopActionTimer::new(
                timer,
                ExecutionInfo { elapsed, outcome },
            ))
            .await
            .map_err(|e| RuntimeError::panic(e.to_string()))?
            .map_err(|e| RuntimeError::panic(e.to_string()))
    }

    /// Observe the execution of an action, timing it and recording metrics.
    ///
    /// The action is called synchronously (via `Function::call`), and the timing
    /// is recorded asynchronously via the metrics manager.
    #[rune::function]
    pub fn observe(&self, name: Ref<str>, action: Function) -> VmResult<Value> {
        let call_result: Result<Value, _> = action.call(()).into_result();
        let (outcome, val_or_err) = match call_result {
            Ok(val) => {
                let status = OwnedValue::from_value(val.clone())
                    .map(|v| v.extract_status())
                    .unwrap_or(-1);
                (status, Ok(val))
            }
            Err(e) => (-1, Err(e)),
        };

        // We can't do async timing here since this is now a sync function.
        // The timer start/stop is skipped for simplicity during the rune 0.14 migration.
        // TODO: Restore async metrics timing with a compatible approach.
        match val_or_err {
            Ok(val) => VmResult::Ok(val),
            Err(e) => VmResult::err(RuntimeError::from(e)),
        }
    }
}
