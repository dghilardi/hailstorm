use crate::simulation::actor::simulation::BotStateChange;
use crate::simulation::bot::scripted::ScriptedBot;
use crate::simulation::rune::types::value::OwnedValue;
use crate::utils::actix::weak_context::WeakContext;
use actix::{
    Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, AtomicResponse, Context, Handler,
    Message, Recipient, ResponseActFuture, WrapFuture,
};
use rand::Rng;
use rune::Hash;
use std::future::Future;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::watch;

#[derive(Clone, Copy)]
pub(crate) struct BotExecutionLimits {
    pub action_timeout: Duration,
    pub stop_timeout: Duration,
    pub instruction_budget: usize,
}

impl Default for BotExecutionLimits {
    fn default() -> Self {
        Self {
            action_timeout: Duration::from_secs(60),
            stop_timeout: Duration::from_secs(5),
            instruction_budget: 1_000_000,
        }
    }
}

async fn run_operation<T>(
    operation: impl Future<Output = Result<T, rune::runtime::VmError>>,
    timeout: Duration,
    instruction_budget: usize,
    mut stop_requested: Option<watch::Receiver<bool>>,
) -> Result<T, ActionExecutionError> {
    let execution = tokio::time::timeout(
        timeout,
        rune::runtime::budget::with(instruction_budget, operation),
    );
    let result = if let Some(stop_requested) = stop_requested.as_mut() {
        if *stop_requested.borrow() {
            return Err(ActionExecutionError::Cancelled);
        }
        tokio::select! {
            biased;
            _ = stop_requested.changed() => return Err(ActionExecutionError::Cancelled),
            result = execution => result,
        }
    } else {
        execution.await
    };
    result
        .map_err(|_| ActionExecutionError::Timeout)?
        .map_err(|err| ActionExecutionError::RuneError(err.to_string()))
}

/// Lifecycle state of a bot instance.
///
/// Bots transition through these states as they are spawned, execute actions,
/// and are eventually stopped. Custom states can be defined in Rune scripts.
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum BotState {
    /// Bot is idle and not yet participating in the simulation.
    Idle,
    /// Bot is initializing (running the enter-state hook).
    Initializing,
    /// Bot is actively executing actions.
    Running,
    /// Bot is shutting down (running the stopping hook).
    Stopping,
    /// Bot has fully stopped and will be cleaned up.
    Stopped,
    /// A user-defined custom state identified by a numeric ID.
    Custom(u32),
}

impl From<BotState> for u32 {
    fn from(state: BotState) -> Self {
        match state {
            BotState::Idle => 0,
            BotState::Initializing => 1,
            BotState::Running => 2,
            BotState::Stopping => 3,
            BotState::Stopped => 4,
            BotState::Custom(cst) => 100 + cst,
        }
    }
}

/// Actor representing a hailstorm bot
pub struct BotActor {
    bot_id: u64,
    state_change_recipient: Recipient<BotStateChange>,
    bot: Option<ScriptedBot>,
    execution_limits: BotExecutionLimits,
    stop_requested: Option<watch::Receiver<bool>>,
    stopping: bool,
}

impl BotActor {
    pub fn new<A>(bot_id: u64, simulation_addr: Addr<A>, bot: ScriptedBot) -> Self
    where
        A: Actor<Context = Context<A>> + Handler<BotStateChange>,
    {
        Self {
            bot_id,
            state_change_recipient: simulation_addr.recipient(),
            bot: Some(bot),
            execution_limits: BotExecutionLimits::default(),
            stop_requested: None,
            stopping: false,
        }
    }

    pub(crate) fn with_control(
        mut self,
        limits: BotExecutionLimits,
        stop_requested: watch::Receiver<bool>,
    ) -> Self {
        self.execution_limits = limits;
        self.stop_requested = Some(stop_requested);
        self
    }
}

impl Actor for BotActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Bot actor started");
        if let Some(mut stop_requested) = self.stop_requested.clone() {
            ctx.spawn(
                async move {
                    if !*stop_requested.borrow() {
                        let _ = stop_requested.changed().await;
                    }
                }
                .into_actor(self)
                .map(|_, _, ctx| ctx.notify(StopBot)),
            );
        }
        let bot = self.bot.as_ref().expect("bot not defined");
        if !bot.has_actions() {
            return;
        }
        let interval = bot.get_interval();
        let random_delay =
            Duration::from_millis(rand::rng().random_range(0..interval.as_millis() as u64));
        ctx.run_later(random_delay, move |_a, ctx| {
            ctx.run_interval_weak(interval, |addr| async move {
                match addr.send(DoAction).await {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => log::error!("Error executing DoAction - {err}"),
                    Err(err) => log::error!("Error sending DoAction - {err}"),
                }
            });
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::debug!("Bot actor stopped");
        self.state_change_recipient.do_send(BotStateChange {
            bot_id: self.bot_id,
            state: BotState::Stopped,
        });
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub(crate) struct StopBot;

impl Handler<StopBot> for BotActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _msg: StopBot, ctx: &mut Self::Context) -> Self::Result {
        if self.stopping {
            return Box::pin(futures::future::ready(()).into_actor(self));
        }
        self.stopping = true;
        let act_fut = ctx
            .address()
            .send(TriggerHook {
                state: BotState::Stopping,
            })
            .into_actor(self)
            .map(|res, _act, ctx| {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => log::error!("Error executing stopping hook - {err}"),
                    Err(err) => log::error!("Error triggering stopping hook - {err}"),
                }
                ctx.stop();
            });
        Box::pin(act_fut)
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), ActionExecutionError>")]
struct DoAction;

/// Error during bot action execution.
#[derive(Error, Debug)]
pub enum ActionExecutionError {
    #[error("Bot execution cancelled")]
    Cancelled,
    #[error("Bot execution timed out")]
    Timeout,
    #[error("Error during rune execution - {0}")]
    RuneError(String),
    #[error("Bot is currently occupied")]
    OccupiedBot,
    #[error("Internal Error - {0}")]
    Internal(String),
}

impl Handler<DoAction> for BotActor {
    type Result = AtomicResponse<Self, Result<(), ActionExecutionError>>;

    fn handle(&mut self, _msg: DoAction, _ctx: &mut Self::Context) -> Self::Result {
        if self.stopping {
            return AtomicResponse::new(Box::pin(
                futures::future::err(ActionExecutionError::Cancelled).into_actor(self),
            ));
        }
        let limits = self.execution_limits;
        let stop_requested = self.stop_requested.clone();
        if let Some(mut bot) = self.bot.take() {
            AtomicResponse::new(Box::pin(
                async move {
                    let res = run_operation(
                        bot.run_random_action(),
                        limits.action_timeout,
                        limits.instruction_budget,
                        stop_requested,
                    )
                    .await;
                    if res.is_err() {
                        bot.reset_execution();
                    }
                    (bot, res)
                }
                .into_actor(self)
                .map(|(u, res), a, _c| {
                    a.bot = Some(u);
                    res
                }),
            ))
        } else {
            log::warn!("Bot is occupied");
            AtomicResponse::new(Box::pin(
                futures::future::err(ActionExecutionError::OccupiedBot).into_actor(self),
            ))
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), ActionExecutionError>")]
pub(crate) struct TriggerHook {
    pub state: BotState,
}

impl Handler<TriggerHook> for BotActor {
    type Result = AtomicResponse<Self, Result<(), ActionExecutionError>>;

    fn handle(
        &mut self,
        TriggerHook { state }: TriggerHook,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.stopping && state != BotState::Stopping {
            return AtomicResponse::new(Box::pin(
                futures::future::err(ActionExecutionError::Cancelled).into_actor(self),
            ));
        }
        let limits = self.execution_limits;
        let (timeout, stop_requested) = if state == BotState::Stopping {
            (limits.stop_timeout, None)
        } else {
            (limits.action_timeout, self.stop_requested.clone())
        };
        if let Some(mut bot) = self.bot.take() {
            AtomicResponse::new(Box::pin(
                async move {
                    let res = run_operation(
                        bot.trigger_hook(state),
                        timeout,
                        limits.instruction_budget,
                        stop_requested,
                    )
                    .await;
                    if res.is_err() {
                        bot.reset_execution();
                    }
                    (bot, res)
                }
                .into_actor(self)
                .map(|(u, res), a, _c| {
                    a.bot = Some(u);
                    res
                }),
            ))
        } else {
            log::warn!("Bot is occupied");
            AtomicResponse::new(Box::pin(
                futures::future::err(ActionExecutionError::OccupiedBot).into_actor(self),
            ))
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<OwnedValue, ActionExecutionError>")]
/// Message to ask BotActor to execute a handler
pub struct ExecuteHandler {
    pub(crate) id: Hash,
    pub(crate) args: OwnedValue,
}

impl ExecuteHandler {
    pub fn new(id: Hash, args: OwnedValue) -> Self {
        Self { id, args }
    }
}

impl Handler<ExecuteHandler> for BotActor {
    type Result = AtomicResponse<Self, Result<OwnedValue, ActionExecutionError>>;

    fn handle(&mut self, msg: ExecuteHandler, _ctx: &mut Self::Context) -> Self::Result {
        if self.stopping {
            return AtomicResponse::new(Box::pin(
                futures::future::err(ActionExecutionError::Cancelled).into_actor(self),
            ));
        }
        let limits = self.execution_limits;
        let stop_requested = self.stop_requested.clone();
        if let Some(mut bot) = self.bot.take() {
            AtomicResponse::new(Box::pin(
                async move {
                    let out = run_operation(
                        bot.execute_handler(msg.id, msg.args),
                        limits.action_timeout,
                        limits.instruction_budget,
                        stop_requested,
                    )
                    .await;
                    if out.is_err() {
                        bot.reset_execution();
                    }
                    (bot, out)
                }
                .into_actor(self)
                .map(|(u, out), a, _c| {
                    a.bot = Some(u);
                    out
                }),
            ))
        } else {
            log::warn!("Bot is occupied");
            AtomicResponse::new(Box::pin(
                futures::future::err(ActionExecutionError::OccupiedBot).into_actor(self),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::metrics::manager::actor::MetricsManagerActor;
    use crate::simulation::bot::registry::BotRegistry;
    use crate::simulation::compound_id::CompoundId;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    struct StateCapture(tokio::sync::mpsc::UnboundedSender<BotState>);

    impl Actor for StateCapture {
        type Context = Context<Self>;
    }

    impl Handler<BotStateChange> for StateCapture {
        type Result = ();

        fn handle(&mut self, msg: BotStateChange, _: &mut Self::Context) {
            let _ = self.0.send(msg.state);
        }
    }

    fn scripted_bot(
        action: &str,
        stop_hook: &str,
        entered: Arc<AtomicBool>,
        stopped: Arc<AtomicBool>,
    ) -> ScriptedBot {
        let mut context = rune::Context::with_default_modules().unwrap();
        let mut module = rune::Module::with_crate("test").unwrap();
        module
            .function("wait_forever", move || {
                let entered = entered.clone();
                async move {
                    entered.store(true, Ordering::SeqCst);
                    std::future::pending::<()>().await;
                }
            })
            .build()
            .unwrap();
        module
            .function("stopped", move || {
                stopped.store(true, Ordering::SeqCst);
            })
            .build()
            .unwrap();
        context.install(module).unwrap();
        let mut registry = BotRegistry::new(context, MetricsManagerActor::start_default()).unwrap();
        registry
            .load_script(&format!(
                r#"
            use hailstorm::bot::{{ActionTrigger, BotState}};
            struct Bot;
            impl Bot {{
                pub fn new(args) {{ Self }}
                pub fn register_bot(bot) {{
                    bot.set_interval_millis(60000);
                    bot.register_action(ActionTrigger::alive(1.0), Self::action);
                    bot.register_action(ActionTrigger::enter_state(BotState::Stopping), Self::stop);
                }}
                pub async fn action(self) {{ {action} }}
                pub async fn stop(self) {{ {stop_hook} }}
            }}
        "#
            ))
            .unwrap();
        registry.build_bot(CompoundId::new(1, 0, 1), "Bot").unwrap()
    }

    fn actor(
        bot: ScriptedBot,
        limits: BotExecutionLimits,
    ) -> (
        Addr<BotActor>,
        watch::Sender<bool>,
        tokio::sync::mpsc::UnboundedReceiver<BotState>,
    ) {
        let (sender, states) = tokio::sync::mpsc::unbounded_channel();
        let capture = StateCapture(sender).start();
        let (stop, stop_receiver) = watch::channel(false);
        let addr = BotActor::new(1, capture, bot)
            .with_control(limits, stop_receiver)
            .start();
        (addr, stop, states)
    }

    #[actix::test]
    async fn stop_cancels_an_in_flight_action_and_runs_the_stopping_hook() {
        let entered = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let bot = scripted_bot(
            "test::wait_forever().await",
            "test::stopped()",
            entered.clone(),
            stopped.clone(),
        );
        let (addr, stop, mut states) = actor(bot, BotExecutionLimits::default());
        let execution_addr = addr.clone();
        let execution = actix::spawn(async move { execution_addr.send(DoAction).await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while !entered.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        stop.send_replace(true);
        assert!(matches!(
            execution.await.unwrap().unwrap(),
            Err(ActionExecutionError::Cancelled)
        ));
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), states.recv())
                .await
                .unwrap(),
            Some(BotState::Stopped)
        );
        assert!(stopped.load(Ordering::SeqCst));
        assert!(!addr.connected());
    }

    #[actix::test]
    async fn an_action_timeout_restores_the_vm_for_the_stopping_hook() {
        let entered = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let bot = scripted_bot(
            "test::wait_forever().await",
            "test::stopped()",
            entered,
            stopped.clone(),
        );
        let (addr, _stop, mut states) = actor(
            bot,
            BotExecutionLimits {
                action_timeout: Duration::from_millis(40),
                ..Default::default()
            },
        );
        assert!(matches!(
            addr.send(DoAction).await.unwrap(),
            Err(ActionExecutionError::Timeout)
        ));
        addr.send(StopBot).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), states.recv())
                .await
                .unwrap(),
            Some(BotState::Stopped)
        );
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[actix::test]
    async fn an_unfinished_stopping_hook_does_not_keep_the_actor_alive() {
        let entered = Arc::new(AtomicBool::new(false));
        let bot = scripted_bot(
            "()",
            "test::wait_forever().await",
            entered.clone(),
            Arc::new(AtomicBool::new(false)),
        );
        let (addr, _stop, mut states) = actor(
            bot,
            BotExecutionLimits {
                stop_timeout: Duration::from_millis(40),
                ..Default::default()
            },
        );
        addr.send(StopBot).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), states.recv())
                .await
                .unwrap(),
            Some(BotState::Stopped)
        );
        assert!(entered.load(Ordering::SeqCst));
        assert!(!addr.connected());
    }

    #[actix::test]
    async fn cpu_bound_scripts_exhaust_the_instruction_budget() {
        let stopped = Arc::new(AtomicBool::new(false));
        let bot = scripted_bot(
            "loop {}",
            "test::stopped()",
            Arc::new(AtomicBool::new(false)),
            stopped.clone(),
        );
        let (addr, _stop, mut states) = actor(
            bot,
            BotExecutionLimits {
                instruction_budget: 1000,
                ..Default::default()
            },
        );
        assert!(matches!(
            addr.send(DoAction).await.unwrap(),
            Err(ActionExecutionError::RuneError(_))
        ));
        addr.send(StopBot).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), states.recv())
                .await
                .unwrap(),
            Some(BotState::Stopped)
        );
        assert!(stopped.load(Ordering::SeqCst));
    }
}
