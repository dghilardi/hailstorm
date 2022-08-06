use std::error::Error;
use actix::{Actor, Addr, Context};
use crate::agent::actor::AgentCoreActor;

pub trait UpstreamAgentActor
    : Actor<Context=Context<Self>>
{
    type Config;
    type InitializationError: Error;

    fn new(cfg: Self::Config, core_addr: Addr<AgentCoreActor>) -> Result<Self, Self::InitializationError>;
}