use std::collections::HashMap;
use std::future::Future;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::dev::RecipientRequest;
use actix::{
    Actor, ActorFutureExt, AtomicResponse, Context, Handler, Recipient, ResponseFuture, WrapFuture,
};

use crate::communication::message::{ControllerCommandMessage, MultiAgentUpdateMessage};
use crate::communication::protobuf::grpc;
use crate::communication::protobuf::grpc::command_item::Command;
use crate::communication::protobuf::grpc::{
    AgentUpdate, LaunchCommand, LoadSimCommand, StopCommand,
};
use crate::controller::client::downstream::DownstreamClient;
use crate::controller::message::{LoadSimulation, StartSimulation};
use crate::controller::model::simulation::SimulationState;

/// Time before an agent is considered disconnected if no updates are received
const AGENT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
struct AgentState {
    timestamp: SystemTime,
    state: grpc::AgentSimulationState,
}

/// Actor responsible for managing the state of the controller.
///
/// It handles:
/// - Aggregating updates from agents.
/// - Maintaining the state of connected agents.
/// - Coordinating simulation state changes (Ready, Launched, Stopped).
/// - Broadcasting commands to agents.
pub struct ControllerActor {
    downstream: DownstreamClient,
    metrics_storage: Recipient<MultiAgentUpdateMessage>,
    agents_state: HashMap<u32, AgentState>,
    simulation: SimulationState,
}

impl ControllerActor {
    pub fn new(
        downstream: DownstreamClient,
        metrics_storage: Recipient<MultiAgentUpdateMessage>,
    ) -> Self {
        Self {
            downstream,
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

    fn handle(
        &mut self,
        MultiAgentUpdateMessage(agent_updates): MultiAgentUpdateMessage,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let pre_handle_agents_count = self.count_agents();

        let agent_alignment_fut = self.align_agents_simulation_state(&agent_updates);
        let send_metrics_fut = self
            .metrics_storage
            .send(MultiAgentUpdateMessage(agent_updates));

        let post_handle_agents_count = self.count_agents();

        // If the number of agents changed, we need to broadcast the new count
        let cmd_fut = if pre_handle_agents_count != post_handle_agents_count {
            log::info!(
                "Update agents count {pre_handle_agents_count} -> {post_handle_agents_count}"
            );
            Some(
                self.downstream
                    .send_broadcast(vec![Command::UpdateAgentsCount(
                        post_handle_agents_count as u32,
                    )]),
            )
        } else {
            None
        };

        Box::pin(async move {
            let (_, send_metrics_out) = futures::join!(agent_alignment_fut, send_metrics_fut);

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

    fn broadcast_simulation_state(&mut self) -> RecipientRequest<ControllerCommandMessage> {
        self.downstream
            .send_broadcast(self.generate_simulation_state_commands())
    }

    fn misaligned_agents(&self) -> HashMap<u32, AgentState> {
        self.agents_state
            .iter()
            .filter(|(_, agent)| !self.simulation.is_aligned(&agent.state))
            .map(|(k, v)| (*k, v.clone()))
            .collect::<HashMap<_, _>>()
    }

    fn align_agents_simulation_state(
        &mut self,
        updates: &[AgentUpdate],
    ) -> impl Future<Output = ()> {
        for update in updates {
            // Validate timestamp
            if let Some(timestamp) = update
                .timestamp
                .clone()
                .map(SystemTime::try_from)
                .transpose()
                .ok()
                .flatten()
            {
                let entry = self
                    .agents_state
                    .entry(update.agent_id)
                    .or_insert(AgentState {
                        timestamp,
                        state: update.state(),
                    });

                // Update only if the new timestamp is more recent
                if entry.timestamp < timestamp {
                    entry.timestamp = timestamp;
                    entry.state = update.state();
                }
            }
        }

        // Remove timed-out agents
        self.agents_state
            .retain(|_id, state| state.timestamp.add(AGENT_TIMEOUT) > SystemTime::now());

        let misaligned = self.misaligned_agents();
        let commands = self.generate_simulation_state_commands();

        // Send commands to misaligned agents to bring them up to speed
        let maybe_send_fut = if misaligned.is_empty() {
            None
        } else {
            Some(
                self.downstream
                    .send_to_agents(misaligned.keys().cloned().collect(), commands),
            )
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

        let mut commands = vec![
             Command::Stop(StopCommand { reset: true }),
             Command::UpdateAgentsCount(agents_count as u32),
        ];

        match &self.simulation {
            SimulationState::Idle => {
                // Default commands are sufficient
            },
            SimulationState::Ready { simulation } => {
                commands.push(Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter().cloned().map(Into::into).collect(),
                    script: simulation.script.clone(),
                }));
            },
            SimulationState::Launched {
                start_ts,
                simulation,
            } => {
                 commands.push(Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter().cloned().map(Into::into).collect(),
                    script: simulation.script.clone(),
                }));
                commands.push(Command::Launch(LaunchCommand {
                    start_ts: Some((*start_ts).into()),
                }));
            },
        }
        commands
    }
}

impl Handler<LoadSimulation> for ControllerActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(
        &mut self,
        LoadSimulation(simulation): LoadSimulation,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.simulation = SimulationState::Ready { simulation };

        AtomicResponse::new(Box::pin(
            async {}
                .into_actor(self)
                .then(|_, act, _ctx| act.broadcast_simulation_state().into_actor(act))
                .map(|res, _, _| {
                    if let Err(err) = res {
                        log::error!("Error sending load-sim command - {err}");
                    }
                }),
        ))
    }
}

impl Handler<StartSimulation> for ControllerActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(
        &mut self,
        StartSimulation(start_ts): StartSimulation,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.simulation = match &self.simulation {
            SimulationState::Idle => {
                log::warn!("Ignoring StartSimulation command as state is idle");
                SimulationState::Idle
            }
            SimulationState::Ready { simulation } => SimulationState::Launched {
                start_ts,
                simulation: simulation.clone(),
            },
            SimulationState::Launched { simulation, .. } => SimulationState::Launched {
                start_ts,
                simulation: simulation.clone(),
            },
        };

        AtomicResponse::new(Box::pin(
            async move {}
                .into_actor(self)
                .then(|_, act, _ctx| act.broadcast_simulation_state().into_actor(act))
                .map(|res, _, _| {
                    if let Err(err) = res {
                        log::error!("Error sending load-sim command - {err}");
                    }
                }),
        ))
    }
}
