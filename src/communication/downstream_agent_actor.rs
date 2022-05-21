use actix::{Actor, Context, Handler};
use tokio::sync::mpsc::Sender;
use crate::communication::grpc::ControllerCommand;
use crate::communication::message::ControllerCommandMessage;

pub struct DownstreamAgentActor {
    cmd_sender: Sender<ControllerCommand>
}

impl Actor for DownstreamAgentActor {
    type Context = Context<Self>;
}

impl DownstreamAgentActor {
    pub fn new(
        cmd_sender: Sender<ControllerCommand>
    ) -> Self {
        Self { cmd_sender }
    }
}

impl Handler<ControllerCommandMessage> for DownstreamAgentActor {
    type Result = ();

    fn handle(&mut self, ControllerCommandMessage(msg): ControllerCommandMessage, _ctx: &mut Self::Context) -> Self::Result {
        self.cmd_sender.try_send(msg)
            .unwrap_or_else(|err| log::error!("Error sending command downstream {err}"))
    }
}