use crate::communication::grpc::{ControllerCommand, AgentUpdate};
use crate::grpc::AgentMessage;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ControllerCommandMessage(pub ControllerCommand);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MultiAgentUpdateMessage(pub Vec<AgentUpdate>);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct SendAgentMessage(pub AgentMessage);