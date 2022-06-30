use std::time::Instant;
use actix::{Actor, Addr, Context, Handler, Recipient};
use rune::{Any, Value};
use rune::runtime::{Function, Shared, VmError};
use crate::agent::metrics::manager_actor::{StartActionTimer, StopActionTimer};

#[derive(Any)]
pub struct PerformanceRegistry {
    start_timer_recipient: Recipient<StartActionTimer>,
    stop_timer_recipient: Recipient<StopActionTimer>,
}

impl PerformanceRegistry {
    pub fn new<A>(metrics_addr: Addr<A>) -> Self
    where A: Actor<Context=Context<A>> +
        Handler<StartActionTimer> +
        Handler<StopActionTimer>
    {
        Self {
            start_timer_recipient: metrics_addr.clone().recipient(),
            stop_timer_recipient: metrics_addr.recipient(),
        }
    }

    pub async fn observe(&self, name: &str, action: Function) -> Result<i64, VmError> {
        let before = Instant::now();
        let res = action.async_send_call(()).await;
        let elapsed = before.elapsed().as_millis();
        res
    }
}