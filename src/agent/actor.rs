use std::future::ready;
use std::time::Duration;

use actix::{Actor, Addr, AsyncContext, Context, Handler, StreamHandler};
use futures::StreamExt;
use rand::{Rng, thread_rng};
use tokio::sync::mpsc::Sender;
use tonic::Streaming;

use crate::communication::grpc::{AgentMessage, AgentUpdate, ControllerCommand};
use crate::communication::message::ControllerCommandMessage;
use crate::communication::notifier_actor::{AgentUpdateMessage, RegisterAgentUpdateSender, UpdatesNotifierActor};
use crate::communication::server_actor::HailstormServerActor;

pub struct AgentCoreActor {
    agent_id: u64,
    notifier_addr: Addr<UpdatesNotifierActor>,
    server_addr: Addr<HailstormServerActor>,
}

impl AgentCoreActor {
    pub fn new(
        agent_id: u64,
        notifier_addr: Addr<UpdatesNotifierActor>,
        server_addr: Addr<HailstormServerActor>,
    ) -> Self {
        Self {
            agent_id,
            notifier_addr,
            server_addr,
        }
    }


    fn send_data(&mut self) {
        self.notifier_addr.try_send(AgentUpdateMessage(AgentUpdate {
            agent_id: self.agent_id,
            stats: None,
            update_id: thread_rng().gen(),
            timestamp: None,
            name: "".to_string(),
            state: 0,
            simulation_id: "".to_string(),
        })).unwrap_or_else(|err| {
            log::error!("Error sending agent stats to notifier actor {err}");
        });
    }
}

impl Actor for AgentCoreActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_secs(3), |actor, _ctx| actor.send_data());
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct RegisterAgentClientMsg {
    pub cmd_stream: Streaming<ControllerCommand>,
    pub msg_sender: Sender<AgentMessage>,
}

impl Handler<RegisterAgentClientMsg> for AgentCoreActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterAgentClientMsg, ctx: &mut Self::Context) -> Self::Result {
        self.notifier_addr
            .try_send(RegisterAgentUpdateSender(msg.msg_sender))
            .unwrap_or_else(|err| log::error!("Error registering agent update sender - {err:?}"));
        ctx.add_stream(
            msg.cmd_stream
                .filter_map(move |result|
                    ready(
                        result
                            .map(|message| ConnectedClientMessage { message })
                            .map_err(|err| log::error!("Error during stream processing {err}"))
                            .ok()
                    )
                )
        );
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ConnectedClientMessage {
    message: ControllerCommand,
}

impl StreamHandler<ConnectedClientMessage> for AgentCoreActor {
    fn handle(&mut self, ConnectedClientMessage { message, .. }: ConnectedClientMessage, _ctx: &mut Self::Context) {
        log::debug!("message: {message:?}");
        self.server_addr.try_send(ControllerCommandMessage(message))
            .unwrap_or_else(|err| log::error!("Error sending command to server actor"));
    }

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler started")
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler finished")
    }
}