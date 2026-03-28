use crate::agent::actor::AgentCoreActor;
use actix::{Actor, Addr, Context};
use std::error::Error;

/// Trait defining the interface for upstream agent connections.
///
/// Implementors establish and maintain a connection to a parent agent or controller,
/// forwarding commands downstream and metrics upstream. The framework provides a gRPC
/// implementation via [`GrpcUpstreamAgentActor`](super::grpc::GrpcUpstreamAgentActor).
pub trait UpstreamAgentActor: Actor<Context = Context<Self>> {
    /// Configuration type needed to establish the upstream connection (e.g., a URL string).
    type Config;
    /// Error type returned if initialization fails.
    type InitializationError: Error;

    /// Create a new upstream actor from the given configuration and core actor address.
    fn new(
        cfg: Self::Config,
        core_addr: Addr<AgentCoreActor>,
    ) -> Result<Self, Self::InitializationError>;
}
