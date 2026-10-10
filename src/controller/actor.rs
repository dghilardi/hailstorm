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
    AgentDistribution, AgentUpdate, LaunchCommand, LoadSimCommand, StopCommand,
};
use crate::controller::client::downstream::DownstreamClient;
use crate::controller::message::{LoadSimulation, StartSimulation};
use crate::controller::model::simulation::SimulationState;

#[derive(Clone, Debug)]
struct AgentState {
    timestamp: SystemTime,
    state: grpc::AgentSimulationState,
}

/// Core controller actor that orchestrates distributed simulations.
///
/// Tracks connected agent states, distributes simulation commands, and ensures
/// all agents converge to the desired simulation state (idle, ready, or launched).
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
        let pre_handle_agents = self.sorted_agent_ids();

        let agent_alignment_fut = self.align_agents_simulation_state(&agent_updates);
        let send_metrics_fut = self
            .metrics_storage
            .send(MultiAgentUpdateMessage(agent_updates));

        let post_handle_agents = self.sorted_agent_ids();

        let cmd_fut = if pre_handle_agents != post_handle_agents {
            let pre_handle_agents_count = pre_handle_agents.len();
            let post_handle_agents_count = post_handle_agents.len();
            log::info!(
                "Update agents count {pre_handle_agents_count} -> {post_handle_agents_count}"
            );
            Some(self.downstream.send_broadcast(self.distribution_commands()))
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
    fn sorted_agent_ids(&self) -> Vec<u32> {
        let mut agent_ids = self.agents_state.keys().copied().collect::<Vec<_>>();
        agent_ids.sort_unstable();
        agent_ids
    }

    fn distribution_commands(&self) -> Vec<Command> {
        vec![
            Command::UpdateAgentsCount(self.count_agents() as u32),
            Command::UpdateAgents(AgentDistribution {
                agent_ids: self.sorted_agent_ids(),
            }),
        ]
    }

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

                if entry.timestamp < timestamp {
                    entry.timestamp = timestamp;
                    entry.state = update.state();
                }
            }
        }
        self.agents_state
            .retain(|_id, state| state.timestamp.add(Duration::from_secs(60)) > SystemTime::now());

        let misaligned = self.misaligned_agents();
        let commands = self.generate_simulation_state_commands();

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
        match &self.simulation {
            SimulationState::Idle => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::UpdateAgentsCount(agents_count as u32),
                Command::UpdateAgents(AgentDistribution {
                    agent_ids: self.sorted_agent_ids(),
                }),
            ],
            SimulationState::Ready { simulation } => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::UpdateAgentsCount(agents_count as u32),
                Command::UpdateAgents(AgentDistribution {
                    agent_ids: self.sorted_agent_ids(),
                }),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter().cloned().map(Into::into).collect(),
                    script: simulation.script.clone(),
                }),
            ],
            SimulationState::Launched {
                start_ts,
                simulation,
            } => vec![
                Command::Stop(StopCommand { reset: true }),
                Command::UpdateAgentsCount(agents_count as u32),
                Command::UpdateAgents(AgentDistribution {
                    agent_ids: self.sorted_agent_ids(),
                }),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter().cloned().map(Into::into).collect(),
                    script: simulation.script.clone(),
                }),
                Command::Launch(LaunchCommand {
                    start_ts: Some((*start_ts).into()),
                }),
            ],
        }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::communication::message::MultiAgentUpdateMessage;
    use crate::communication::protobuf::grpc::{AgentUpdate, ControllerCommand};
    use crate::communication::server_actor::GrpcServerActor;
    use actix::{Actor, Context, Handler};
    use std::sync::{Arc, Mutex};

    struct MockMetricsStorage;
    impl Actor for MockMetricsStorage {
        type Context = Context<Self>;
    }
    impl Handler<MultiAgentUpdateMessage> for MockMetricsStorage {
        type Result = ();
        fn handle(&mut self, _msg: MultiAgentUpdateMessage, _ctx: &mut Self::Context) {}
    }

    #[actix::test]
    async fn controller_tracks_new_agents() {
        let metrics = MockMetricsStorage.start();
        let grpc_server_ctx: Context<GrpcServerActor> = Context::new();
        let grpc_server_actor = GrpcServerActor::new(metrics.clone().recipient());
        let server_addr = grpc_server_ctx.run(grpc_server_actor);

        let controller = ControllerActor::new(
            DownstreamClient::new(server_addr.recipient()),
            metrics.recipient(),
        );
        let controller_addr = controller.start();

        let update = MultiAgentUpdateMessage(vec![AgentUpdate {
            agent_id: 42,
            stats: vec![],
            update_id: 1,
            timestamp: Some(SystemTime::now().into()),
            name: String::new(),
            state: grpc::AgentSimulationState::Idle as i32,
            simulation_id: String::new(),
        }]);

        controller_addr.send(update).await.expect("send failed");
    }

    struct Capture {
        commands: Arc<Mutex<Vec<ControllerCommand>>>,
    }

    impl Actor for Capture {
        type Context = Context<Self>;
    }

    impl Handler<ControllerCommandMessage> for Capture {
        type Result = ();

        fn handle(&mut self, msg: ControllerCommandMessage, _: &mut Self::Context) {
            self.commands.lock().unwrap().push(msg.0);
        }
    }

    impl Handler<MultiAgentUpdateMessage> for Capture {
        type Result = ();

        fn handle(&mut self, _: MultiAgentUpdateMessage, _: &mut Self::Context) {}
    }

    #[actix::test]
    async fn replacing_an_agent_broadcasts_membership_even_when_count_is_unchanged() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let capture = Capture {
            commands: commands.clone(),
        }
        .start();
        let mut controller = ControllerActor::new(
            DownstreamClient::new(capture.clone().recipient()),
            capture.recipient(),
        );
        let now = SystemTime::now();
        for (id, timestamp) in [(0, now - Duration::from_secs(61)), (2, now)] {
            controller.agents_state.insert(
                id,
                AgentState {
                    timestamp,
                    state: grpc::AgentSimulationState::Idle,
                },
            );
        }
        controller
            .handle(
                MultiAgentUpdateMessage(vec![AgentUpdate {
                    agent_id: 4,
                    timestamp: Some(now.into()),
                    state: grpc::AgentSimulationState::Idle as i32,
                    ..Default::default()
                }]),
                &mut Context::new(),
            )
            .await;

        let commands = commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].commands.iter().any(|item| {
            matches!(&item.command, Some(Command::UpdateAgents(distribution)) if distribution.agent_ids == vec![2, 4])
        }));
        assert_eq!(controller.count_agents(), 2);
    }
}
