use crate::agent::actor::AgentCoreActor;
use actix::{Actor, Addr, Context};
use std::error::Error;

pub trait UpstreamAgentActor: Actor<Context = Context<Self>> {
    type Config;
    type InitializationError: Error;

    fn new(
        cfg: Self::Config,
        core_addr: Addr<AgentCoreActor>,
    ) -> Result<Self, Self::InitializationError>;
}
