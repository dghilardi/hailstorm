use crate::communication::downstream_agent_actor::DownstreamAgentActor;
use crate::communication::message::{ControllerCommandMessage, MultiAgentUpdateMessage};
use crate::communication::protobuf::grpc::controller_command::Target;
use crate::communication::protobuf::grpc::AgentMessage;
use crate::communication::protobuf::grpc::MultiAgent;
use crate::server::RegisterConnectedAgentMsg;
use actix::{
    Actor, Addr, AsyncContext, Context, Handler, Recipient, ResponseFuture, StreamHandler,
};
use futures::future::ready;
use futures::StreamExt;
use rand::{thread_rng, RngCore};
use std::collections::HashMap;
use std::ops::Add;
use std::time::{Duration, SystemTime};

struct ConnectedAgent {
    last_received_update: SystemTime,
}

struct DownstreamConnection {
    agent_ids: HashMap<u32, ConnectedAgent>,
    sender: Addr<DownstreamAgentActor>,
}

pub struct GrpcServerActor {
    agent_update_recipient: Recipient<MultiAgentUpdateMessage>,
    downstream_agents: HashMap<u64, DownstreamConnection>,
}

impl Actor for GrpcServerActor {
    type Context = Context<Self>;
}

impl GrpcServerActor {
    pub fn new(agent_update_recipient: Recipient<MultiAgentUpdateMessage>) -> Self {
        Self {
            agent_update_recipient,
            downstream_agents: Default::default(),
        }
    }

    fn connections_cleanup(&mut self) {
        self.downstream_agents
            .retain(|_id, conn| conn.sender.connected())
    }
}

impl Handler<RegisterConnectedAgentMsg> for GrpcServerActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterConnectedAgentMsg, ctx: &mut Self::Context) -> Self::Result {
        let ca_addr = DownstreamAgentActor::create(|_| DownstreamAgentActor::new(msg.cmd_sender));
        let connection_id = thread_rng().next_u64();
        let connection = DownstreamConnection {
            agent_ids: Default::default(),
            sender: ca_addr,
        };
        self.downstream_agents.insert(connection_id, connection);
        ctx.add_stream(msg.states_stream.filter_map(move |result| {
            ready(
                result
                    .map(|message| ConnectedAgentMessage {
                        connection_id,
                        message,
                    })
                    .map_err(|err| log::error!("Error during stream processing {err}"))
                    .ok(),
            )
        }));
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ConnectedAgentMessage {
    connection_id: u64,
    message: AgentMessage,
}

impl StreamHandler<ConnectedAgentMessage> for GrpcServerActor {
    fn handle(
        &mut self,
        ConnectedAgentMessage {
            connection_id,
            message,
        }: ConnectedAgentMessage,
        _ctx: &mut Self::Context,
    ) {
        let connection = self
            .downstream_agents
            .get_mut(&connection_id)
            .expect("Connection not defined");
        for update_item in message.updates.iter() {
            let last_state_update_ts = update_item
                .stats
                .iter()
                .flat_map(|stats| stats.states.iter())
                .filter_map(|states| states.timestamp.clone())
                .filter_map(|ts| SystemTime::try_from(ts).ok())
                .max()
                .unwrap_or_else(SystemTime::now);

            let agent_entry =
                connection
                    .agent_ids
                    .entry(update_item.agent_id)
                    .or_insert(ConnectedAgent {
                        last_received_update: last_state_update_ts,
                    });

            if last_state_update_ts > agent_entry.last_received_update {
                agent_entry.last_received_update = last_state_update_ts;
            }
        }

        self.agent_update_recipient
            .try_send(MultiAgentUpdateMessage(message.updates))
            .unwrap_or_else(|err| log::error!("Error sending update message {err:?}"));

        for (_, da) in self.downstream_agents.iter_mut() {
            da.agent_ids.retain(|_k, v| {
                v.last_received_update.add(Duration::from_secs(60)) > SystemTime::now()
            })
        }
    }

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler started")
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler finished")
    }
}

impl Handler<ControllerCommandMessage> for GrpcServerActor {
    type Result = ResponseFuture<()>;

    fn handle(
        &mut self,
        ControllerCommandMessage(msg): ControllerCommandMessage,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.connections_cleanup();
        let connections = self
            .downstream_agents
            .values()
            .filter(|conn| match msg.target {
                None => true,
                Some(Target::Group(_)) => true,
                Some(Target::AgentId(agent_id)) => conn.agent_ids.contains_key(&agent_id),
                Some(Target::Agents(MultiAgent { ref agent_ids })) => {
                    agent_ids.iter().any(|id| conn.agent_ids.contains_key(id))
                }
            })
            .map(|da| da.sender.clone())
            .collect::<Vec<_>>();

        if connections.is_empty() && !matches!(msg.target, None | Some(Target::Group(_))) {
            log::warn!("No connection available for target {:?}", msg.target);
        }

        Box::pin(async move {
            for downstream_agent in connections {
                let send_out = downstream_agent
                    .send(ControllerCommandMessage(msg.clone()))
                    .await;
                if let Err(err) = send_out {
                    log::error!("Error sending command to downstream agent client {err}");
                }
            }
        })
    }
}
