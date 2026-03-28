use crate::communication::protobuf::grpc::AgentMessage;
use crate::communication::protobuf::grpc::{AgentUpdate, ControllerCommand};

/// Wraps a [`ControllerCommand`] for delivery via actix messaging.
#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ControllerCommandMessage(pub ControllerCommand);

/// Wraps a batch of [`AgentUpdate`]s for delivery via actix messaging.
#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MultiAgentUpdateMessage(pub Vec<AgentUpdate>);

/// Wraps an [`AgentMessage`] for sending upstream via actix messaging.
#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct SendAgentMessage(pub AgentMessage);
