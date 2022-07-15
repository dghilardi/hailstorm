use std::time::Duration;
use actix::{Actor, ActorContext, Addr, AsyncContext, AtomicResponse, Context, Handler, Message, WrapFuture, ActorFutureExt, Recipient};
use rand::{Rng, thread_rng};
use rune::Hash;
use thiserror::Error;
use crate::simulation::rune::types::value::OwnedValue;
use crate::simulation::actor::simulation::BotStateChange;
use crate::simulation::bot::scripted::ScriptedBot;
use crate::utils::actix::weak_context::WeakContext;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum BotState {
    Idle,
    Initializing,
    Running,
    Stopping,
    Stopped,
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

pub struct BotActor {
    bot_id: u64,
    state_change_recipient: Recipient<BotStateChange>,
    bot: Option<ScriptedBot>,
}

impl BotActor {
    pub fn new<A>(
        bot_id: u64,
        simulation_addr: Addr<A>,
        bot: ScriptedBot,
    ) -> Self
        where A: Actor<Context=Context<A>>
        + Handler<BotStateChange>
    {
        Self {
            bot_id,
            state_change_recipient: simulation_addr.recipient(),
            bot: Some(bot),
        }
    }
}

impl Actor for BotActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Bot actor started");
        let interval = self.bot.as_ref().expect("bot not defined").get_interval();
        let random_delay = Duration::from_millis(thread_rng().gen_range(0..interval.as_millis() as u64));
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
        self.state_change_recipient.try_send(BotStateChange {
            bot_id: self.bot_id,
            state: BotState::Stopped,
        }).unwrap_or_else(|e| log::error!("Error sending stopped bot state - {e}"));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StopBot;

impl Handler<StopBot> for BotActor {
    type Result = ();

    fn handle(&mut self, _msg: StopBot, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), ActionExecutionError>")]
pub struct DoAction;

#[derive(Error, Debug)]
pub enum ActionExecutionError {
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
        if let Some(mut bot) = self.bot.take() {
            AtomicResponse::new(Box::pin(async {
                let res = bot.run_random_action().await;
                (bot, res)
            }
                .into_actor(self)
                .map(|(u, res), a, _c| {
                    a.bot = Some(u);
                    res.map_err(|e| ActionExecutionError::RuneError(e.to_string()))
                })
            ))
        } else {
            log::warn!("Bot is occupied");
            AtomicResponse::new(Box::pin(futures::future::err(ActionExecutionError::OccupiedBot).into_actor(self)))
        }
    }
}


#[derive(Message)]
#[rtype(result = "Result<(), ActionExecutionError>")]
pub struct TriggerHook {
    pub state: BotState,
}

impl Handler<TriggerHook> for BotActor {
    type Result = AtomicResponse<Self, Result<(), ActionExecutionError>>;

    fn handle(&mut self, TriggerHook { state }: TriggerHook, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut bot) = self.bot.take() {
            AtomicResponse::new(Box::pin(async move {
                let res = bot.trigger_hook(state).await;
                (bot, res)
            }
                .into_actor(self)
                .map(|(u, res), a, _c| {
                    a.bot = Some(u);
                    res.map_err(|e| ActionExecutionError::RuneError(e.to_string()))
                })
            ))
        } else {
            log::warn!("Bot is occupied");
            AtomicResponse::new(Box::pin(futures::future::err(ActionExecutionError::OccupiedBot).into_actor(self)))
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<OwnedValue, ActionExecutionError>")]
pub struct ExecuteHandler {
    pub id: Hash,
    pub args: OwnedValue,
}

impl Handler<ExecuteHandler> for BotActor {
    type Result = AtomicResponse<Self, Result<OwnedValue, ActionExecutionError>>;

    fn handle(&mut self, msg: ExecuteHandler, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut bot) = self.bot.take() {
            AtomicResponse::new(Box::pin(async move {
                let out = bot.execute_handler(msg.id, msg.args).await;
                (bot, out)
            }
                .into_actor(self)
                .map(|(u, out), a, _c| {
                    a.bot = Some(u);
                    out.map_err(|e| ActionExecutionError::RuneError(e.to_string()))
                })
            ))
        } else {
            log::warn!("Bot is occupied");
            AtomicResponse::new(Box::pin(futures::future::err(ActionExecutionError::OccupiedBot).into_actor(self)))
        }
    }
}