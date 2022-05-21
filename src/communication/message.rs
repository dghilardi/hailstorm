use crate::communication::grpc::ControllerCommand;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ControllerCommandMessage(pub ControllerCommand);