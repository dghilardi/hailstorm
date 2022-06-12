use crate::communication::grpc::{ControllerCommand, AgentUpdate};

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ControllerCommandMessage(pub ControllerCommand);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MultiAgentUpdateMessage(pub Vec<AgentUpdate>);