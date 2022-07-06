use actix::{Actor, Context, Handler, ResponseFuture};
use tokio::sync::mpsc::Sender;
use crate::communication::protobuf::grpc::ControllerCommand;
use crate::communication::message::ControllerCommandMessage;

pub struct DownstreamAgentActor {
    cmd_sender: Sender<ControllerCommand>,
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
    type Result = ResponseFuture<()>;

    fn handle(&mut self, ControllerCommandMessage(msg): ControllerCommandMessage, _ctx: &mut Self::Context) -> Self::Result {
        let sender = self.cmd_sender.clone();
        Box::pin(async move {
            let send_out = sender.send(msg).await;
            if let Err(err) = send_out {
                log::error!("Error sending command downstream {err}");
            }
        })
    }
}