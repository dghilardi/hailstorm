use crate::communication::protobuf::grpc::AgentMessage;
use crate::communication::protobuf::grpc::{AgentUpdate, ControllerCommand};

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ControllerCommandMessage(pub ControllerCommand);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MultiAgentUpdateMessage(pub Vec<AgentUpdate>);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct SendAgentMessage(pub AgentMessage);
