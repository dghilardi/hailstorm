use std::time::{Duration, Instant};

use actix::{Actor, Addr, Context, Handler, Recipient};
use rune::runtime::{Function, Ref, RuntimeError, Value, VmError, VmResult};
use rune::{Any, TypeHash};

use crate::agent::metrics::manager::message::StartedActionTimer;
use crate::agent::metrics::manager::message::{StartActionTimer, StopActionTimer};
use crate::agent::metrics::timer::{ActionOutcome, ExecutionInfo};

fn action_outcome(value: &Value, depth: usize) -> Result<i64, RuntimeError> {
    if depth >= 64 {
        return Ok(-1);
    }
    if value.type_hash() == Option::<Value>::HASH {
        return match &*value.borrow_ref::<Option<Value>>()? {
            Some(value) => action_outcome(value, depth + 1),
            None => Ok(-1),
        };
    }
    if value.type_hash() == Result::<Value, Value>::HASH {
        return match &*value.borrow_ref::<Result<Value, Value>>()? {
            Ok(value) | Err(value) => action_outcome(value, depth + 1),
        };
    }
    Ok(value
        .as_integer::<i64>()
        .or_else(|_| value.as_integer::<u64>().map(|value| value as i64))
        .unwrap_or(0))
}

#[derive(Any)]
#[rune(item = ::hailstorm::metrics)]
pub struct PerformanceRegistry {
    model: String,
    start_timer_recipient: Recipient<StartActionTimer>,
    stop_timer_recipient: Recipient<StopActionTimer>,
}

struct ObservationGuard {
    timer: Option<StartedActionTimer>,
    before: Instant,
    stop_timer_recipient: Recipient<StopActionTimer>,
}

impl Drop for ObservationGuard {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            self.stop_timer_recipient.do_send(StopActionTimer::new(
                timer,
                ExecutionInfo {
                    elapsed: self.before.elapsed(),
                    outcome: -2,
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::metrics::manager::actor::MetricsManagerActor;
    use crate::agent::metrics::manager::message::FetchActionMetrics;
    use crate::simulation::rune::extension::metrics::module;
    use actix::Actor;
    use rune::FromValue;
    use std::sync::Arc;

    fn vm(script: &str, manager: Addr<MetricsManagerActor>) -> rune::Vm {
        vm_with_entered(script, manager, Arc::new(tokio::sync::Notify::new()))
    }

    fn vm_with_entered(
        script: &str,
        manager: Addr<MetricsManagerActor>,
        entered: Arc<tokio::sync::Notify>,
    ) -> rune::Vm {
        let mut context = rune::Context::with_default_modules().unwrap();
        context.install(module::module(manager).unwrap()).unwrap();
        let mut native = rune::Module::with_crate("test").unwrap();
        native
            .function("pause", || async {
                tokio::time::sleep(Duration::from_millis(40)).await;
            })
            .build()
            .unwrap();
        native
            .function("wait_forever", move || {
                let entered = entered.clone();
                async move {
                    entered.notify_one();
                    std::future::pending::<()>().await;
                }
            })
            .build()
            .unwrap();
        context.install(native).unwrap();
        let mut sources = rune::Sources::new();
        sources
            .insert(rune::Source::new("test", script).unwrap())
            .unwrap();
        let unit = rune::prepare(&mut sources)
            .with_context(&context)
            .build()
            .unwrap();
        rune::Vm::new(Arc::new(context.runtime().unwrap()), Arc::new(unit))
    }

    #[actix::test]
    async fn asynchronous_observations_wait_for_the_action_and_record_its_outcome() {
        let manager = MetricsManagerActor::start_default();
        let mut vm = vm(
            r#"
            pub async fn action() { test::pause().await; 201 }
            pub async fn main() {
                let perf = hailstorm::metrics::PerformanceRegistry::new("Bot");
                perf.observe("request", action).await
            }
        "#,
            manager.clone(),
        );
        let result = vm.async_call(["main"], ()).await.unwrap();
        assert_eq!(i64::from_value(result).unwrap(), 201);
        let snapshots = manager.send(FetchActionMetrics).await.unwrap();
        assert_eq!(
            snapshots.len(),
            1,
            "observation must emit performance metrics"
        );
        let metrics = &snapshots[0].metrics.last().unwrap().metrics[&201];
        assert_eq!(metrics.histogram.iter().sum::<u64>(), 1);
        assert!(
            metrics.sum > 0,
            "duration must include the awaited operation"
        );
    }

    #[actix::test]
    async fn synchronous_callbacks_preserve_structured_return_values() {
        let manager = MetricsManagerActor::start_default();
        let mut vm = vm(
            r#"
            pub fn action() { #{ status: 7, payload: "response" } }
            pub async fn main() {
                let perf = hailstorm::metrics::new("Bot");
                let result = perf.observe("request", action).await;
                assert_eq!(result.payload, "response");
                result.status
            }
        "#,
            manager.clone(),
        );
        assert_eq!(
            i64::from_value(vm.async_call(["main"], ()).await.unwrap()).unwrap(),
            7
        );
        let snapshots = manager.send(FetchActionMetrics).await.unwrap();
        assert_eq!(snapshots.len(), 1);
        let metrics = &snapshots[0].metrics.last().unwrap().metrics[&0];
        assert_eq!(metrics.histogram.iter().sum::<u64>(), 1);
    }

    #[actix::test]
    async fn action_errors_are_recorded_and_propagated_to_the_script() {
        let manager = MetricsManagerActor::start_default();
        let mut vm = vm(
            r#"
            pub async fn action() { panic("failed") }
            pub async fn main() {
                let perf = hailstorm::metrics::new("Bot");
                perf.observe("request", action).await
            }
        "#,
            manager.clone(),
        );
        assert!(vm.async_call(["main"], ()).await.is_err());
        let snapshots = manager.send(FetchActionMetrics).await.unwrap();
        assert_eq!(snapshots.len(), 1);
        let metrics = &snapshots[0].metrics.last().unwrap().metrics[&-1];
        assert_eq!(metrics.histogram.iter().sum::<u64>(), 1);
    }

    #[actix::test]
    async fn nested_option_and_result_values_remain_readable_after_status_extraction() {
        let manager = MetricsManagerActor::start_default();
        let mut vm = vm(
            r#"
            pub fn action() { Some(Err(503)) }
            pub async fn main() {
                let perf = hailstorm::metrics::new("Bot");
                match perf.observe("request", action).await {
                    Some(Err(code)) => code,
                    _ => panic("response changed"),
                }
            }
        "#,
            manager.clone(),
        );
        assert_eq!(
            i64::from_value(vm.async_call(["main"], ()).await.unwrap()).unwrap(),
            503
        );
        let snapshots = manager.send(FetchActionMetrics).await.unwrap();
        let metrics = &snapshots[0].metrics.last().unwrap().metrics[&503];
        assert_eq!(metrics.histogram.iter().sum::<u64>(), 1);
    }
    #[actix::test]
    async fn cancelling_an_observation_records_one_cancelled_measurement() {
        let manager = MetricsManagerActor::start_default();
        let entered = Arc::new(tokio::sync::Notify::new());
        let mut vm = vm_with_entered(
            r#"
            pub async fn action() { test::wait_forever().await }
            pub async fn main() {
                let perf = hailstorm::metrics::new("Bot");
                perf.observe("wait", action).await
            }
            "#,
            manager.clone(),
            entered.clone(),
        );
        let execution = actix::spawn(async move { vm.async_call(["main"], ()).await });
        tokio::time::timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        execution.abort();
        match execution.await {
            Err(err) => assert!(err.is_cancelled()),
            Ok(_) => panic!("observation should have been cancelled"),
        }
        let snapshots = manager.send(FetchActionMetrics).await.unwrap();
        assert_eq!(snapshots.len(), 1);
        let metrics = &snapshots[0].metrics.last().unwrap().metrics;
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[&-2].histogram.iter().sum::<u64>(), 1);
    }

    #[actix::test]
    async fn completed_observations_preserve_the_outcome_without_counting_cancellation() {
        let manager = MetricsManagerActor::start_default();
        let mut vm = vm(
            r#"
            pub async fn action() { 7 }
            pub async fn main() {
                let perf = hailstorm::metrics::new("Bot");
                perf.observe("done", action).await
            }
            "#,
            manager.clone(),
        );
        assert_eq!(
            i64::from_value(vm.async_call(["main"], ()).await.unwrap()).unwrap(),
            7
        );
        drop(vm);
        let snapshots = manager.send(FetchActionMetrics).await.unwrap();
        let metrics = &snapshots[0].metrics.last().unwrap().metrics;
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[&7].histogram.iter().sum::<u64>(), 1);
    }
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

    /// Time a synchronous or asynchronous callback, recording its completed outcome.
    /// The returned Rune value is preserved, including structured responses.
    #[rune::function(instance, path = Self::observe)]
    pub async fn observe(this: Ref<Self>, name: Ref<str>, action: Function) -> VmResult<Value> {
        match this.observe_action(&name, action).await {
            Ok(val) => VmResult::Ok(val),
            Err(err) => VmResult::Err(err),
        }
    }

    async fn observe_action(&self, name: &str, action: Function) -> Result<Value, VmError> {
        let timer = self.start_timer(name).await?;
        let before = Instant::now();
        let mut guard = ObservationGuard {
            timer: Some(timer.clone()),
            before,
            stop_timer_recipient: self.stop_timer_recipient.clone(),
        };
        let result = async {
            let value: Value = action.call(()).into_result()?;
            if value.type_hash() == rune::runtime::Future::HASH {
                value
                    .borrow_mut::<rune::runtime::Future>()?
                    .await
                    .into_result()
            } else {
                Ok(value)
            }
        }
        .await;
        let outcome = result
            .as_ref()
            .map(|value| action_outcome(value, 0).unwrap_or(-1))
            .unwrap_or(-1);
        self.stop_timer(timer, before.elapsed(), outcome).await?;
        guard.timer = None;
        result
    }
}
