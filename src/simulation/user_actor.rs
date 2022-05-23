use actix::{Actor, ActorContext, Addr, Context, Handler, Message};
use crate::simulation::simulation_actor::SimulationActor;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum UserState {
    Running,
    Stopping,
    Custom(u32),
}

impl From<UserState> for u32 {
    fn from(state: UserState) -> Self {
        match state {
            UserState::Running => 0,
            UserState::Stopping => 1,
            UserState::Custom(cst) => 100 + cst,
        }
    }
}

pub struct UserActor {
    user_id: u64,
    simulation_addr: Addr<SimulationActor>,
}

impl UserActor {
    pub fn new(
        user_id: u64,
        simulation_addr: Addr<SimulationActor>,
    ) -> Self {
        Self {
            user_id,
            simulation_addr
        }
    }
}

impl Actor for UserActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("User actor started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        log::debug!("User actor stopped");
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StopUser;

impl Handler<StopUser> for UserActor {
    type Result = ();

    fn handle(&mut self, msg: StopUser, ctx: &mut Self::Context) -> Self::Result {
        ctx.stop();
    }
}