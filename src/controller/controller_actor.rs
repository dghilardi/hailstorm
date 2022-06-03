use std::collections::{HashMap, HashSet};
use std::ops::Add;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use actix::{Actor, AsyncContext, Context, Handler};
use futures::{Stream, StreamExt};
use futures::future::ready;
use tokio::sync::mpsc::{self, Sender};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Response;
use tonic::Status;
use crate::controller::metrics_storage::MetricsStorage;

use crate::controller::model::simulation::{SimulationDef, SimulationState};
use crate::grpc::{AgentMessage, ControllerCommand, LaunchCommand};
use crate::grpc::controller_command::Command;
use crate::server::RegisterConnectedAgentMsg;

struct ConnectedAgent {
    last_received_update: SystemTime,
    agent_id: u64,
}

struct DownstreamConnection {
    agent_ids: HashMap<u64, ConnectedAgent>,
    sender: Sender<ControllerCommand>,
}

impl DownstreamConnection {
    pub fn new(sender: Sender<ControllerCommand>) -> Self {
        Self {
            agent_ids: Default::default(),
            sender,
        }
    }
}

pub struct ControllerActor {
    downstream_agents: Vec<DownstreamConnection>,
    simulation: SimulationState,
    metrics_storage: Box<dyn MetricsStorage>,
}

impl ControllerActor {
    pub fn new(metrics_storage: impl MetricsStorage + 'static) -> Self {
        Self {
            metrics_storage: Box::new(metrics_storage),
            downstream_agents: vec![],
            simulation: SimulationState::Idle,
        }
    }
}

impl Actor for ControllerActor {
    type Context = Context<Self>;
}

type ResponseStream<T> = Pin<Box<dyn Stream<Item=Result<T, Status>> + Send>>;

impl Handler<RegisterConnectedAgentMsg> for ControllerActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterConnectedAgentMsg, ctx: &mut Self::Context) -> Self::Result {
        match &self.simulation {
            SimulationState::Idle => {}
            SimulationState::Ready { simulation } => {
                tokio::spawn(initialize_agents(None, simulation.clone(), msg.cmd_sender.clone()));
            }
            SimulationState::Launched { start_ts, simulation } => {
                tokio::spawn(initialize_agents(Some(*start_ts), simulation.clone(), msg.cmd_sender.clone()));
            }
        }
        let address = ctx.address();
        let stream_idx = self.downstream_agents.len();
        self.downstream_agents.push(DownstreamConnection::new(msg.cmd_sender));
        ctx.add_message_stream(
            msg.states_stream
                .filter_map(move |result| ready(result
                    .map(|agent_msg| ReceivedAgentMessage(agent_msg, stream_idx))
                    .map_err(|err| log::error!("Error receiving agent message - {err}"))
                    .ok()
                ))
        );
    }
}

async fn initialize_agents(
    maybe_start_ts: Option<SystemTime>,
    simulation: SimulationDef,
    sender: Sender<ControllerCommand>,
) {
    sender.send(ControllerCommand { command: Some(Command::Load(simulation.into())) })
        .await
        .expect("Error sending load command");

    if let Some(start_ts) = maybe_start_ts {
        sender.send(ControllerCommand {
            command: Some(Command::Launch(LaunchCommand { start_ts: Some(start_ts.into()) }))
        })
            .await
            .expect("Error sending launch command");
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct TerminatedStream(usize);

impl Handler<TerminatedStream> for ControllerActor {
    type Result = ();

    fn handle(&mut self, TerminatedStream(terminated_idx): TerminatedStream, _ctx: &mut Self::Context) -> Self::Result {
        self.downstream_agents.remove(terminated_idx);
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ReceivedAgentMessage(AgentMessage, usize);

impl Handler<ReceivedAgentMessage> for ControllerActor {
    type Result = ();

    fn handle(&mut self, ReceivedAgentMessage(msg, idx): ReceivedAgentMessage, _ctx: &mut Self::Context) -> Self::Result {
        let pre_handle_agents_count = self.count_agents();

        let ref mut agent_ids = self.downstream_agents
            .get_mut(idx)
            .expect(&format!("No downstream agent with index {idx}"))
            .agent_ids;

        for agent_update in msg.updates {
            self.metrics_storage.store(&agent_update);

            let timestamp =  agent_update.timestamp
                .map(SystemTime::try_from)
                .transpose()
                .ok()
                .flatten()
                .unwrap_or_else(SystemTime::now);

            let entry = agent_ids
                .entry(agent_update.agent_id)
                .or_insert(ConnectedAgent {
                    last_received_update: timestamp,
                    agent_id: agent_update.agent_id,
                });

            if entry.last_received_update < timestamp {
                entry.last_received_update = timestamp;
            }
        }

        for da in self.downstream_agents.iter_mut() {
            da.agent_ids.retain(|_k, v| v.last_received_update.add(Duration::from_secs(60)) > SystemTime::now())
        }
        let post_handle_agents_count = self.count_agents();

        if pre_handle_agents_count != post_handle_agents_count {
            for da in self.downstream_agents.iter() {
                da.sender.try_send(ControllerCommand {
                    command: Some(Command::UpdateAgentsCount(post_handle_agents_count as u32))
                });
            }
        }
    }
}

impl ControllerActor {
    fn count_agents(&self) -> usize {
        self.downstream_agents
            .iter()
            .flat_map(|conn| conn.agent_ids.keys())
            .map(ToOwned::to_owned)
            .collect::<HashSet<u64>>()
            .len()
    }
}