use actix::{Actor, ActorContext, Addr, Context, Handler, Message};
use crate::simulation::simulation_actor::{SimulationActor, UserStateChange};
use crate::simulation::user::registry::User;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum UserState {
    Running,
    Stopping,
    Stopped,
    Custom(u32),
}

impl From<UserState> for u32 {
    fn from(state: UserState) -> Self {
        match state {
            UserState::Running => 0,
            UserState::Stopping => 1,
            UserState::Stopped => 2,
            UserState::Custom(cst) => 100 + cst,
        }
    }
}

pub struct UserActor {
    user_id: u64,
    simulation_addr: Addr<SimulationActor>,
    user: User,
}

impl UserActor {
    pub fn new(
        user_id: u64,
        simulation_addr: Addr<SimulationActor>,
        user: User,
    ) -> Self {
        Self {
            user_id,
            simulation_addr,
            user,
        }
    }
}

impl Actor for UserActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("User actor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::debug!("User actor stopped");
        self.simulation_addr.try_send(UserStateChange {
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