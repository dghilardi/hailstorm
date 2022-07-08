use std::time::Duration;
use actix::{Actor, ActorContext, Addr, AsyncContext, AtomicResponse, Context, Handler, Message, WrapFuture, ActorFutureExt, Recipient};
use rand::{Rng, thread_rng};
use crate::simulation::simulation_actor::UserStateChange;
use crate::simulation::user::scripted_user::ScriptedUser;
use crate::utils::actix::weak_context::WeakContext;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum UserState {
    Idle,
    Initializing,
    Running,
    Stopping,
    Stopped,
    Custom(u32),
}

impl From<UserState> for u32 {
    fn from(state: UserState) -> Self {
        match state {
            UserState::Idle => 0,
            UserState::Initializing => 1,
            UserState::Running => 2,
            UserState::Stopping => 3,
            UserState::Stopped => 4,
            UserState::Custom(cst) => 100 + cst,
        }
    }
}

pub struct UserActor {
    user_id: u64,
    state_change_recipient: Recipient<UserStateChange>,
    user: Option<ScriptedUser>,
}

impl UserActor {
    pub fn new<A>(
        user_id: u64,
        simulation_addr: Addr<A>,
        user: ScriptedUser,
    ) -> Self
    where A: Actor<Context = Context<A>>
    + Handler<UserStateChange>
    {
        Self {
            user_id,
            state_change_recipient: simulation_addr.recipient(),
            user: Some(user),
        }
    }
}

impl Actor for UserActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("User actor started");
        let interval = self.user.as_ref().expect("user not defined").get_interval();
        let random_delay = Duration::from_millis(thread_rng().gen_range(0..interval.as_millis() as u64));
        ctx.run_later(random_delay, move |_a, ctx| {
            ctx.run_interval_weak(interval, |addr| addr.try_send(DoAction).unwrap_or_else(|e| log::error!("Error sending DoAction - {e}")));
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::debug!("User actor stopped");
        self.state_change_recipient.try_send(UserStateChange {
            user_id: self.user_id,
            state: UserState::Stopped,
        }).unwrap_or_else(|e| log::error!("Error sending stopped user state - {e}"));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StopUser;

impl Handler<StopUser> for UserActor {
    type Result = ();

    fn handle(&mut self, _msg: StopUser, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct DoAction;

impl Handler<DoAction> for UserActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, _msg: DoAction, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut user) = self.user.take() {
            AtomicResponse::new(Box::pin(async {
                user.run_random_action().await;
                user
            }
                .into_actor(self)
                .map(|u, a, _c| a.user = Some(u))
            ))
        } else {
            log::warn!("User is occupied");
            AtomicResponse::new(Box::pin(futures::future::ready(()).into_actor(self)))
        }
    }
}


#[derive(Message)]
#[rtype(result = "()")]
pub struct TriggerHook {
    pub state: UserState,
}

impl Handler<TriggerHook> for UserActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, TriggerHook { state }: TriggerHook, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut user) = self.user.take() {
            AtomicResponse::new(Box::pin(async move {
                user.trigger_hook(state).await;
                user
            }
                .into_actor(self)
                .map(|u, a, _c| a.user = Some(u))
            ))
        } else {
            log::warn!("User is occupied");
            AtomicResponse::new(Box::pin(futures::future::ready(()).into_actor(self)))
        }
    }
}