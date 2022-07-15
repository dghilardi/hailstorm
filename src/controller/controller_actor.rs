use std::collections::HashMap;
use std::future::Future;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::{Actor, ActorFutureExt, AtomicResponse, Context, Handler, Recipient, ResponseFuture, WrapFuture};
use actix::dev::RecipientRequest;

use crate::communication::message::{ControllerCommandMessage, MultiAgentUpdateMessage};
use crate::controller::model::simulation::{SimulationDef, SimulationState};
use crate::communication::protobuf::grpc;
use crate::communication::protobuf::grpc::{AgentGroup, AgentUpdate, CommandItem, ControllerCommand, LaunchCommand, LoadSimCommand, MultiAgent, StopCommand};
use crate::communication::protobuf::grpc::controller_command::Target;
use crate::communication::protobuf::grpc::command_item::Command;

#[derive(Clone, Debug)]
struct AgentState {
    timestamp: SystemTime,
    state: grpc::AgentSimulationState,
}

pub struct ControllerActor {
    command_sender: Recipient<ControllerCommandMessage>,
    metrics_storage: Recipient<MultiAgentUpdateMessage>,
    agents_state: HashMap<u64, AgentState>,
    simulation: SimulationState,
}

impl ControllerActor {
    pub fn new(
        command_sender: Recipient<ControllerCommandMessage>,
        metrics_storage: Recipient<MultiAgentUpdateMessage>,
    ) -> Self {
        Self {
            command_sender,
            metrics_storage,
            agents_state: Default::default(),
            simulation: SimulationState::Idle,
        }
    }
}

impl Actor for ControllerActor {
    type Context = Context<Self>;
}

impl Handler<MultiAgentUpdateMessage> for ControllerActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, MultiAgentUpdateMessage(agent_updates): MultiAgentUpdateMessage, _ctx: &mut Self::Context) -> Self::Result {
        let pre_handle_agents_count = self.count_agents();

        let agent_alignment_fut = self.align_agents_simulation_state(&agent_updates);
        let send_metrics_fut = self.metrics_storage.send(MultiAgentUpdateMessage(agent_updates));

        let post_handle_agents_count = self.count_agents();

        let cmd_fut = if pre_handle_agents_count != post_handle_agents_count {
            Some(self.send_broadcast(vec![Command::UpdateAgentsCount(post_handle_agents_count as u32)]))
        } else {
            None
        };

        Box::pin(async move {
            let (_, send_metrics_out) = futures:: join!(agent_alignment_fut, send_metrics_fut);

            if let Err(err) = send_metrics_out {
                log::error!("Error sending metrics - {err}");
            }

            if let Some(recipient_req) = cmd_fut {
                if let Err(err) = recipient_req.await {
                    log::error!("Error sending count update - {err}");
                }
            }
        })
    }
}

impl ControllerActor {
    fn count_agents(&self) -> usize {
        self.agents_state.len()
    }

    fn send_to_agent(&mut self, agent_id: u64, commands: Vec<Command>) -> RecipientRequest<ControllerCommandMessage> {
        self.command_sender.send(ControllerCommandMessage(ControllerCommand {
            commands: commands.into_iter().map(|cmd| CommandItem { command: Some(cmd) }).collect(),
            target: Some(Target::AgentId(agent_id)),
        }))
    }

    fn send_to_agents(&mut self, agent_ids: Vec<u64>, commands: Vec<Command>) -> RecipientRequest<ControllerCommandMessage> {
        self.command_sender.send(ControllerCommandMessage(ControllerCommand {
            commands: commands.into_iter().map(|cmd| CommandItem { command: Some(cmd) }).collect(),
            target: Some(Target::Agents(MultiAgent { agent_ids })),
        }))
    }

    fn send_broadcast(&self, commands: Vec<Command>) -> RecipientRequest<ControllerCommandMessage> {
        self.command_sender.send(ControllerCommandMessage(ControllerCommand {
            commands: commands.into_iter().map(|cmd| CommandItem { command: Some(cmd) }).collect(),
            target: Some(Target::Group(AgentGroup::All.into())),
        }))
    }

    fn broadcast_simulation_state(&mut self) -> RecipientRequest<ControllerCommandMessage> {
        self.send_broadcast(self.generate_simulation_state_commands())
    }

    fn misaligned_agents(&self) -> HashMap<u64, AgentState> {
        self.agents_state.iter()
            .filter(|(_, agent)| !self.simulation.is_aligned(&agent.state))
            .map(|(k, v)| (*k, v.clone()))
            .collect::<HashMap<_, _>>()
    }

    fn align_agents_simulation_state(&mut self, updates: &[AgentUpdate]) -> impl Future<Output=()> {
        for update in updates {
            if let Some(timestamp) = update.timestamp.clone().map(SystemTime::try_from).transpose().ok().flatten() {
                let entry = self.agents_state.entry(update.agent_id)
                    .or_insert(AgentState { timestamp, state: update.state() });

                if entry.timestamp < timestamp {
                    entry.timestamp = timestamp;
                    entry.state = update.state();
                }
            }
        }
        self.agents_state.retain(|_id, state| state.timestamp.add(Duration::from_secs(60)) > SystemTime::now());

        let misaligned = self.misaligned_agents();
        let commands = self.generate_simulation_state_commands();

        let maybe_send_fut = if misaligned.is_empty() {
            None
        } else {
            Some(self.send_to_agents(misaligned.keys().cloned().collect(), commands))
        };

        async move {
            if let Some(send_fut) = maybe_send_fut {
                if let Err(err) = send_fut.await {
                    log::error!("Error aligning simulation state - {err}");
                }
            }
        }
    }

    fn generate_simulation_state_commands(&self) -> Vec<Command> {
        let agents_count = self.count_agents();
        match &self.simulation {
            SimulationState::Idle => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::UpdateAgentsCount(agents_count as u32),
            ],
            SimulationState::Ready { simulation } => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::UpdateAgentsCount(agents_count as u32),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter()
                        .cloned().map(Into::into)
                        .collect(),
                    script: simulation.script.clone(),
                }),
            ],
            SimulationState::Launched { start_ts, simulation, } => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::UpdateAgentsCount(agents_count as u32),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter()
                        .cloned().map(Into::into)
                        .collect(),
                    script: simulation.script.clone(),
                }),
                Command::Launch(LaunchCommand { start_ts: Some((*start_ts).into()) }),
            ],
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

        AtomicResponse::new(Box::pin(async {}.into_actor(self)
            .then(|_, act, _ctx| act.broadcast_simulation_state().into_actor(act))
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

        AtomicResponse::new(Box::pin(async move {}.into_actor(self)
            .then(|_, act, _ctx| act.broadcast_simulation_state().into_actor(act))
            .map(|res, _, _| if let Err(err) = res {
                log::error!("Error sending load-sim command - {err}");
            })
        ))
    }
}