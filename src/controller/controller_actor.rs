use std::collections::HashMap;
use std::future::Future;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::{Actor, ActorFutureExt, AtomicResponse, Context, Handler, MailboxError, Recipient, ResponseFuture, WrapFuture};
use actix::dev::RecipientRequest;
use futures::future::{join_all, try_join_all};

use crate::communication::message::ControllerCommandMessage;
use crate::communication::notifier_actor::AgentUpdateMessage;
use crate::controller::model::simulation::{SimulationDef, SimulationState};
use crate::grpc;
use crate::grpc::{AgentGroup, AgentUpdate, ControllerCommand, LaunchCommand, LoadSimCommand, StopCommand};
use crate::grpc::controller_command::{Command, Target};

#[derive(Clone, Debug)]
struct AgentState {
    timestamp: SystemTime,
    state: grpc::AgentSimulationState,
}

pub struct ControllerActor {
    command_sender: Recipient<ControllerCommandMessage>,
    metrics_storage: Recipient<AgentUpdateMessage>,
    agents_state: HashMap<u64, AgentState>,
    simulation: SimulationState,
}

impl ControllerActor {
    pub fn new(
        command_sender: Recipient<ControllerCommandMessage>,
        metrics_storage: Recipient<AgentUpdateMessage>,
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

impl Handler<AgentUpdateMessage> for ControllerActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, AgentUpdateMessage(agent_update): AgentUpdateMessage, _ctx: &mut Self::Context) -> Self::Result {
        let pre_handle_agents_count = self.count_agents();

        let agent_alignment_fut = self.align_agents_simulation_state(&agent_update);
        let send_metrics_fut = self.metrics_storage.send(AgentUpdateMessage(agent_update));

        let post_handle_agents_count = self.count_agents();

        let cmd_fut = if pre_handle_agents_count != post_handle_agents_count {
            Some(self.send_broadcast(Command::UpdateAgentsCount(post_handle_agents_count as u32)))
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

    fn send_to_agent(&mut self, agent_id: u64, command: Command) -> RecipientRequest<ControllerCommandMessage> {
        self.command_sender.send(ControllerCommandMessage(ControllerCommand {
            command: Some(command),
            target: Some(Target::AgentId(agent_id)),
        }))
    }

    fn send_broadcast(&self, command: Command) -> RecipientRequest<ControllerCommandMessage> {
        self.command_sender.send(ControllerCommandMessage(ControllerCommand {
            command: Some(command),
            target: Some(Target::Group(AgentGroup::All.into())),
        }))
    }

    fn broadcast_simulation_state(&mut self) -> impl Future<Output=Result<Vec<()>, MailboxError>> {
        let recipient_reqs = self.generate_simulation_state_commands()
            .into_iter()
            .map(|cmd| self.send_broadcast(cmd));

        try_join_all(recipient_reqs)
    }

    fn misaligned_agents(&self) -> HashMap<u64, AgentState> {
        self.agents_state.iter()
            .filter(|(_, agent)| !self.simulation.is_aligned(&agent.state))
            .map(|(k, v)| (*k, v.clone()))
            .collect::<HashMap<_, _>>()
    }

    fn align_agents_simulation_state(&mut self, update: &AgentUpdate) -> impl Future<Output=()> {
        if let Some(timestamp) = update.timestamp.clone().map(SystemTime::try_from).transpose().ok().flatten() {
            let entry = self.agents_state.entry(update.agent_id)
                .or_insert(AgentState { timestamp, state: update.state() });

            if entry.timestamp < timestamp {
                entry.timestamp = timestamp;
                entry.state = update.state();
            }
        }
        self.agents_state.retain(|_id, state| state.timestamp.add(Duration::from_secs(60)) > SystemTime::now());

        let misaligned = self.misaligned_agents();
        let commands = self.generate_simulation_state_commands();

        let send_futures = misaligned.into_iter()
            .flat_map(|(agent_id, _)| commands.iter().map(move |c| (agent_id, c)))
            .map(|(agent_id, cmd)| self.send_to_agent(agent_id, cmd.clone()))
            .collect::<Vec<_>>();

        async move {
            let results = join_all(send_futures).await;
            for res in results {
                if let Err(err) = res {
                    log::error!("Error aligning simulation state - {err}");
                }
            }
        }
    }

    fn generate_simulation_state_commands(&self) -> Vec<Command> {
        match &self.simulation {
            SimulationState::Idle => vec![Command::Stop(StopCommand { reset: true })],
            SimulationState::Ready { simulation } => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.users.iter()
                        .cloned().map(Into::into)
                        .collect(),
                    script: simulation.script.clone(),
                }),
            ],
            SimulationState::Launched { start_ts, simulation, } => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.users.iter()
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