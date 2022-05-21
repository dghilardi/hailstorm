use actix::{Actor, Addr, AsyncContext, Context, Handler, StreamHandler};
use futures::future::ready;
use futures::StreamExt;
use tokio::sync::mpsc::Sender;
use tonic::Streaming;
use crate::communication::grpc::{AgentMessage, ControllerCommand};
use crate::communication::notifier_actor::{AgentUpdateMessage, UpdatesNotifierActor};

pub struct HailstormServerActor {
    updater_addr: Addr<UpdatesNotifierActor>,
}

impl Actor for HailstormServerActor {
    type Context = Context<Self>;
}

impl HailstormServerActor {
    pub fn new(updater_addr: Addr<UpdatesNotifierActor>) -> Self {
        Self {
            updater_addr,
        }
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct RegisterConnectedAgentMsg {
    pub states_stream: Streaming<AgentMessage>,
    pub cmd_sender: Sender<ControllerCommand>,
}

impl Handler<RegisterConnectedAgentMsg> for HailstormServerActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterConnectedAgentMsg, ctx: &mut Self::Context) -> Self::Result {
        ctx.add_stream(
            msg.states_stream
                .filter_map(move |result|
                    ready(
                        result
                            .map(|message| ConnectedAgentMessage { message })
                            .map_err(|err| log::error!("Error during stream processing {err}"))
                            .ok()
                    )
                )
        );
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ConnectedAgentMessage {
    message: AgentMessage,
}

impl StreamHandler<ConnectedAgentMessage> for HailstormServerActor {
    fn handle(&mut self, ConnectedAgentMessage { message }: ConnectedAgentMessage, _ctx: &mut Self::Context) {
        for update_item in message.updates {
            self.updater_addr
                .try_send(AgentUpdateMessage(update_item))
                .unwrap_or_else(|err| log::error!("Error sending update message {err:?}"));
        }
    }

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler started")
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler finished")
    }
}