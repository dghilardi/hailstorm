use actix::{Actor, ActorContext, Context, Handler, ResponseFuture};
use futures::future;
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

    fn handle(&mut self, ControllerCommandMessage(msg): ControllerCommandMessage, ctx: &mut Self::Context) -> Self::Result {
        if self.cmd_sender.is_closed() {
            log::warn!("Downstream channel is closed. Stopping actor");
            ctx.stop();
            Box::pin(future::ready(()))
        } else {
            let sender = self.cmd_sender.clone();
            Box::pin(async move {
                let send_out = sender.send(msg).await;
                if let Err(err) = send_out {
                    log::error!("Error sending command downstream {err}");
                }
            })
        }
    }
}