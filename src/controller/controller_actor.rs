use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::{Actor, ActorFutureExt, AsyncContext, AtomicResponse, Context, Handler, WrapFuture};
use futures::future::ready;
use futures::StreamExt;
use rand::{RngCore, thread_rng};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::Sender;

use crate::controller::metrics_storage::MetricsStorage;
use crate::controller::model::simulation::{SimulationDef, SimulationState};
use crate::grpc::{AgentMessage, AgentUpdate, ControllerCommand, LaunchCommand};
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
    downstream_agents: HashMap<u64, DownstreamConnection>,
    simulation: SimulationState,
    metrics_storage: Box<dyn MetricsStorage>,
}

impl ControllerActor {
    pub fn new(metrics_storage: impl MetricsStorage + 'static) -> Self {
        Self {
            metrics_storage: Box::new(metrics_storage),
            downstream_agents: Default::default(),
            simulation: SimulationState::Idle,
        }
    }
}

impl Actor for ControllerActor {
    type Context = Context<Self>;
}

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

        let channel_id = thread_rng().next_u64();
        self.downstream_agents.insert(channel_id, DownstreamConnection::new(msg.cmd_sender));
        ctx.add_message_stream(
            msg.states_stream
                .filter_map(move |result| ready(result
                    .map(|agent_msg| ReceivedAgentMessage(agent_msg, channel_id))
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
) -> Result<(), SendError<ControllerCommand>>{
    sender.send(ControllerCommand { command: Some(Command::Load(simulation.into())) }).await?;

    if let Some(start_ts) = maybe_start_ts {
        sender.send(ControllerCommand {
            command: Some(Command::Launch(LaunchCommand { start_ts: Some(start_ts.into()) }))
        }).await?;
    }
    Ok(())
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct TerminatedStream(u64);

impl Handler<TerminatedStream> for ControllerActor {
    type Result = ();

    fn handle(&mut self, TerminatedStream(terminated_idx): TerminatedStream, _ctx: &mut Self::Context) -> Self::Result {
        self.downstream_agents.remove(&terminated_idx);
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ReceivedAgentMessage(AgentMessage, u64);

impl Handler<ReceivedAgentMessage> for ControllerActor {
    type Result = ();

    fn handle(&mut self, ReceivedAgentMessage(msg, idx): ReceivedAgentMessage, _ctx: &mut Self::Context) -> Self::Result {
        let pre_handle_agents_count = self.count_agents();

        self.update_downstream_data(idx, &msg.updates);

        for agent_update in msg.updates {
            self.metrics_storage.store(&agent_update);
        }

        for (_, da) in self.downstream_agents.iter_mut() {
            da.agent_ids.retain(|_k, v| v.last_received_update.add(Duration::from_secs(60)) > SystemTime::now())
        }
        let post_handle_agents_count = self.count_agents();

        if pre_handle_agents_count != post_handle_agents_count {
            self.broadcast_downstream(ControllerCommand {
                command: Some(Command::UpdateAgentsCount(post_handle_agents_count as u32))
            });
            for (_, da) in self.downstream_agents.iter() {
                let send_outcome = da.sender.try_send(ControllerCommand {
                    command: Some(Command::UpdateAgentsCount(post_handle_agents_count as u32))
                });

                if let Err(send_err) = send_outcome {
                    log::error!("Error sending update-agents-count downstream - {send_err}");
                }
            }
        }
    }
}

impl ControllerActor {
    fn count_agents(&self) -> usize {
        self.downstream_agents
            .values()
            .flat_map(|conn| conn.agent_ids.keys())
            .map(ToOwned::to_owned)
            .collect::<HashSet<u64>>()
            .len()
    }

    fn get_valid_downstream(&mut self) -> &mut HashMap<u64, DownstreamConnection> {
        self.downstream_agents.retain(|_, da| !da.sender.is_closed());
        &mut self.downstream_agents
    }

    fn update_downstream_data(&mut self, channel_id: u64, agent_updates: &[AgentUpdate]) {
        if let Some(channel_data) = self.downstream_agents.get_mut(&channel_id) {
            for agent_update in agent_updates {
                let timestamp = agent_update.timestamp
                    .clone()
                    .map(SystemTime::try_from)
                    .transpose()
                    .ok()
                    .flatten()
                    .unwrap_or_else(SystemTime::now);

                let entry = channel_data.agent_ids
                    .entry(agent_update.agent_id)
                    .or_insert(ConnectedAgent {
                        last_received_update: timestamp,
                        agent_id: agent_update.agent_id,
                    });

                if entry.last_received_update < timestamp {
                    entry.last_received_update = timestamp;
                }
            }
        } else {
            log::warn!("Received data for channel {channel_id} but upstream is not connected anymore");
        }
    }

    fn broadcast_downstream(&mut self, command: ControllerCommand) {
        for (id, connection) in self.get_valid_downstream() {
            let outcome = connection.sender.try_send(command.clone());
            if let Err(err) = outcome {
                log::error!("Error sending command to channel with id {id} - {err}");
            }
        }
    }

    fn align_agents_simulation_state(&mut self, state: SimulationState) -> impl Future<Output=Result<(), SendError<ControllerCommand>>> {
        let senders = self.get_valid_downstream()
            .values()
            .map(|dc| dc.sender.clone())
            .collect::<Vec<_>>();

        async move {
            for sender in senders {
                match &state {
                    SimulationState::Idle => {}
                    SimulationState::Ready { simulation } => initialize_agents(None, simulation.clone(), sender.clone()).await?,
                    SimulationState::Launched { start_ts, simulation } => initialize_agents(Some(*start_ts), simulation.clone(), sender.clone()).await?,
                }
            }
            Ok(())
        }
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct LoadSimulation(pub SimulationDef);

impl Handler<LoadSimulation> for ControllerActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, LoadSimulation(simulation): LoadSimulation, _ctx: &mut Self::Context) -> Self::Result {
        self.simulation = SimulationState::Ready {
            simulation
        };

        let simulation_state = self.simulation.clone();
        AtomicResponse::new(Box::pin(async {}.into_actor(self)
            .then(|_, act, _ctx| act.align_agents_simulation_state(simulation_state).into_actor(act))
            .map(|res, _, _| if let Err(err) = res {
                log::error!("Error sending load-sim command - {err}");
            })
        ))
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct StartSimulation(pub SystemTime);

impl Handler<StartSimulation> for ControllerActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, StartSimulation(start_ts): StartSimulation, _ctx: &mut Self::Context) -> Self::Result {
        self.simulation = match &self.simulation {
            SimulationState::Idle => {
                log::warn!("Ignoring StartSimulation command as state is idle");
                SimulationState::Idle
            }
            SimulationState::Ready { simulation } => SimulationState::Launched { start_ts, simulation: simulation.clone() },
            SimulationState::Launched { simulation, .. } => SimulationState::Launched { start_ts, simulation: simulation.clone() },
        };

        let simulation_state = self.simulation.clone();
        AtomicResponse::new(Box::pin(async move {}.into_actor(self)
            .then(|_, act, _ctx| act.align_agents_simulation_state(simulation_state).into_actor(act))
            .map(|res, _, _| if let Err(err) = res {
                log::error!("Error sending load-sim command - {err}");
            })
        ))
    }
}