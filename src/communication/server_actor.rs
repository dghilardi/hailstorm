use actix::{Actor, Addr, AsyncContext, Context, Handler, StreamHandler};
use futures::future::ready;
use futures::StreamExt;
use crate::communication::downstream_agent_actor::DownstreamAgentActor;
use crate::communication::grpc::AgentMessage;
use crate::communication::message::ControllerCommandMessage;
use crate::communication::notifier_actor::{AgentUpdateMessage, UpdatesNotifierActor};
use crate::server::RegisterConnectedAgentMsg;

pub struct HailstormServerActor {
    updater_addr: Addr<UpdatesNotifierActor>,
    downstream_agents: Vec<Addr<DownstreamAgentActor>>,
}

impl Actor for HailstormServerActor {
    type Context = Context<Self>;
}

impl HailstormServerActor {
    pub fn new(updater_addr: Addr<UpdatesNotifierActor>) -> Self {
        Self {
            updater_addr,
            downstream_agents: vec![]
        }
    }
}

impl Handler<RegisterConnectedAgentMsg> for HailstormServerActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterConnectedAgentMsg, ctx: &mut Self::Context) -> Self::Result {
        let ca_addr = DownstreamAgentActor::create(|_| DownstreamAgentActor::new(msg.cmd_sender));
        self.downstream_agents.push(ca_addr);
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

impl Handler<ControllerCommandMessage> for HailstormServerActor {
    type Result = ();

    fn handle(&mut self, ControllerCommandMessage(msg): ControllerCommandMessage, _ctx: &mut Self::Context) -> Self::Result {
        for downstream_agent in self.downstream_agents.iter() {
            downstream_agent.try_send(ControllerCommandMessage(msg.clone()))
                .unwrap_or_else(|err| log::error!("Error sending command to downstream agent client {err}"))
        }
    }
}