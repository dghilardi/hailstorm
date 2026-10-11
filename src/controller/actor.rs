use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::{
    Actor, ActorFuture, ActorFutureExt, AtomicResponse, Context, Handler, Recipient,
    ResponseFuture, WrapFuture,
};

use crate::communication::message::MultiAgentUpdateMessage;
use crate::communication::protobuf::grpc;
use crate::communication::protobuf::grpc::command_item::Command;
use crate::communication::protobuf::grpc::{
    AgentDistribution, AgentUpdate, LaunchCommand, LoadSimCommand, StopCommand,
};
use crate::controller::client::downstream::DownstreamClient;
use crate::controller::message::{
    AgentStatus, GetSimulationStatus, LoadSimulation, SimulationControlError, SimulationPhase,
    SimulationStatus, StartSimulation, StopSimulation,
};
use crate::controller::model::simulation::SimulationState;
use rand::Rng;

#[derive(Clone, Debug)]
struct AgentState {
    timestamp: SystemTime,
    state: grpc::AgentSimulationState,
    simulation_id: String,
    simulation_error: Option<String>,
}

impl AgentState {
    fn from_update(timestamp: SystemTime, update: &AgentUpdate) -> Self {
        Self {
            timestamp,
            state: update.state(),
            simulation_id: update.simulation_id.clone(),
            simulation_error: Some(update.simulation_error.clone()).filter(|err| !err.is_empty()),
        }
    }
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

    fn alignment_commands(&self, agent: &AgentState) -> Vec<Command> {
        if self
            .simulation
            .is_aligned(&agent.state, &agent.simulation_id)
            || self.failed_current_simulation(agent)
        {
            return Vec::new();
        }
        if self.simulation.simulation_id() == Some(agent.simulation_id.as_str()) {
            return match &self.simulation {
                SimulationState::Launched { start_ts, .. } => {
                    vec![Command::Launch(LaunchCommand {
                        start_ts: Some((*start_ts).into()),
                    })]
                }
                SimulationState::Ready { .. } => vec![Command::Stop(StopCommand { reset: false })],
                SimulationState::Idle => unreachable!(),
            };
        }
        self.generate_simulation_state_commands()
    }

    /// Agents that already failed to load the current simulation are not sent it again.
    fn failed_current_simulation(&self, agent: &AgentState) -> bool {
        agent.simulation_error.is_some()
            && self.simulation.simulation_id() == Some(agent.simulation_id.as_str())
    }

    fn align_agents_simulation_state(
        &mut self,
        updates: &[AgentUpdate],
    ) -> impl Future<Output = ()> {
        let mut refreshed_agents = HashSet::new();
        for update in updates {
            if let Some(timestamp) = update
                .timestamp
                .clone()
                .map(SystemTime::try_from)
                .transpose()
                .ok()
                .flatten()
            {
                if self
                    .agents_state
                    .get(&update.agent_id)
                    .map_or(true, |entry| entry.timestamp < timestamp)
                {
                    self.agents_state
                        .insert(update.agent_id, AgentState::from_update(timestamp, update));
                    refreshed_agents.insert(update.agent_id);
                }
            }
        }
        self.agents_state
            .retain(|_id, state| state.timestamp.add(Duration::from_secs(60)) > SystemTime::now());

        let mut requests = Vec::new();
        for agent_id in refreshed_agents {
            if let Some(agent) = self.agents_state.get(&agent_id) {
                let commands = self.alignment_commands(agent);
                if !commands.is_empty() {
                    requests.push(self.downstream.send_to_agent(agent_id, commands));
                }
            }
        }

        async move {
            for result in futures::future::join_all(requests).await {
                if let Err(err) = result {
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
                Command::UpdateAgentsCount(agents_count as u32),
                Command::UpdateAgents(AgentDistribution {
                    agent_ids: self.sorted_agent_ids(),
                }),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter().cloned().map(Into::into).collect(),
                    script: simulation.script.clone(),
                    simulation_id: simulation.id.clone(),
                }),
            ],
            SimulationState::Launched {
                start_ts,
                simulation,
            } => vec![
                Command::UpdateAgentsCount(agents_count as u32),
                Command::UpdateAgents(AgentDistribution {
                    agent_ids: self.sorted_agent_ids(),
                }),
                Command::Load(LoadSimCommand {
                    clients_evolution: simulation.bots.iter().cloned().map(Into::into).collect(),
                    script: simulation.script.clone(),
                    simulation_id: simulation.id.clone(),
                }),
                Command::Launch(LaunchCommand {
                    start_ts: Some((*start_ts).into()),
                }),
            ],
        }
    }
}

impl ControllerActor {
    fn broadcast_state_change(&mut self) -> impl ActorFuture<Self, Output = ()> + 'static {
        self.broadcast_commands(self.generate_simulation_state_commands())
    }

    fn broadcast_commands(
        &mut self,
        commands: Vec<Command>,
    ) -> impl ActorFuture<Self, Output = ()> + 'static {
        self.downstream
            .send_broadcast(commands)
            .into_actor(self)
            .map(|res, _, _| {
                if let Err(err) = res {
                    log::error!("Error broadcasting simulation state - {err}");
                }
            })
    }
}

impl Handler<LoadSimulation> for ControllerActor {
    type Result = AtomicResponse<Self, Result<String, SimulationControlError>>;

    fn handle(
        &mut self,
        LoadSimulation(mut simulation): LoadSimulation,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(err) = simulation.validate() {
            return AtomicResponse::new(Box::pin(
                futures::future::err(SimulationControlError::InvalidSimulation(err))
                    .into_actor(self),
            ));
        }
        simulation.id = format!("{:016x}", rand::rng().random::<u64>());
        let simulation_id = simulation.id.clone();
        self.simulation = SimulationState::Ready { simulation };

        AtomicResponse::new(Box::pin(
            self.broadcast_state_change()
                .map(move |_, _, _| Ok(simulation_id)),
        ))
    }
}

impl Handler<StartSimulation> for ControllerActor {
    type Result = AtomicResponse<Self, Result<(), SimulationControlError>>;

    fn handle(
        &mut self,
        StartSimulation(start_ts): StartSimulation,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.simulation = match &self.simulation {
            SimulationState::Idle => {
                return AtomicResponse::new(Box::pin(
                    futures::future::err(SimulationControlError::NotLoaded).into_actor(self),
                ));
            }
            SimulationState::Ready { simulation }
            | SimulationState::Launched { simulation, .. } => SimulationState::Launched {
                start_ts,
                simulation: simulation.clone(),
            },
        };

        AtomicResponse::new(Box::pin(
            self.broadcast_commands(vec![Command::Launch(LaunchCommand {
                start_ts: Some(start_ts.into()),
            })])
            .map(|_, _, _| Ok(())),
        ))
    }
}

impl Handler<StopSimulation> for ControllerActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, _msg: StopSimulation, _ctx: &mut Self::Context) -> Self::Result {
        self.simulation = SimulationState::Idle;
        AtomicResponse::new(Box::pin(self.broadcast_state_change()))
    }
}

impl Handler<GetSimulationStatus> for ControllerActor {
    type Result = SimulationStatus;

    fn handle(&mut self, _msg: GetSimulationStatus, _ctx: &mut Self::Context) -> Self::Result {
        let phase = match &self.simulation {
            SimulationState::Idle => SimulationPhase::Idle,
            SimulationState::Ready { .. } => SimulationPhase::Ready,
            SimulationState::Launched { start_ts, .. } => SimulationPhase::Launched {
                start_ts: *start_ts,
            },
        };
        let agents = self
            .sorted_agent_ids()
            .into_iter()
            .filter_map(|agent_id| {
                self.agents_state.get(&agent_id).map(|agent| AgentStatus {
                    agent_id,
                    state: agent.state,
                    simulation_id: agent.simulation_id.clone(),
                    simulation_error: agent.simulation_error.clone(),
                    last_update: agent.timestamp,
                })
            })
            .collect();
        SimulationStatus {
            phase,
            simulation_id: self.simulation.simulation_id().map(String::from),
            agents,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::communication::message::{ControllerCommandMessage, MultiAgentUpdateMessage};
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
            ..Default::default()
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
                    simulation_id: String::new(),
                    simulation_error: None,
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

    fn started_controller() -> (
        actix::Addr<ControllerActor>,
        Arc<Mutex<Vec<ControllerCommand>>>,
    ) {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let capture = Capture {
            commands: commands.clone(),
        }
        .start();
        let controller = ControllerActor::new(
            DownstreamClient::new(capture.clone().recipient()),
            capture.recipient(),
        )
        .start();
        (controller, commands)
    }

    fn definition(shape: &str) -> crate::controller::model::simulation::SimulationDef {
        use crate::controller::model::simulation::{BotDef, SimulationDef};
        SimulationDef::default()
            .bots(vec![BotDef::default().model("Bot").shape(shape)])
            .script(String::from("struct Bot;"))
    }

    fn loaded_ids(commands: &[ControllerCommand]) -> Vec<String> {
        commands
            .iter()
            .flat_map(|cmd| &cmd.commands)
            .filter_map(|item| match &item.command {
                Some(Command::Load(load)) => Some(load.simulation_id.clone()),
                _ => None,
            })
            .collect()
    }

    fn update(agent_id: u32, simulation_id: &str, error: &str) -> MultiAgentUpdateMessage {
        MultiAgentUpdateMessage(vec![AgentUpdate {
            agent_id,
            timestamp: Some(SystemTime::now().into()),
            state: grpc::AgentSimulationState::Idle as i32,
            simulation_id: simulation_id.to_string(),
            simulation_error: error.to_string(),
            ..Default::default()
        }])
    }

    fn state_update(
        agent_id: u32,
        simulation_id: &str,
        state: grpc::AgentSimulationState,
        timestamp: SystemTime,
    ) -> MultiAgentUpdateMessage {
        MultiAgentUpdateMessage(vec![AgentUpdate {
            agent_id,
            timestamp: Some(timestamp.into()),
            state: state as i32,
            simulation_id: simulation_id.to_string(),
            ..Default::default()
        }])
    }

    #[actix::test]
    async fn starting_a_loaded_simulation_only_broadcasts_launch() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        commands.lock().unwrap().clear();
        let start_ts = SystemTime::now();
        controller
            .send(StartSimulation::at(start_ts))
            .await
            .unwrap()
            .unwrap();
        let commands = commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].commands.len(), 1);
        assert!(matches!(&commands[0].commands[0].command,
            Some(Command::Launch(launch)) if launch.start_ts == Some(start_ts.into())));
        assert!(loaded_ids(&commands).is_empty(), "{id} must remain loaded");
    }

    #[actix::test]
    async fn ready_and_waiting_heartbeats_after_launch_only_receive_launch() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        let start_ts = SystemTime::now() - Duration::from_secs(10);
        controller
            .send(StartSimulation::at(start_ts))
            .await
            .unwrap()
            .unwrap();
        for state in [
            grpc::AgentSimulationState::Ready,
            grpc::AgentSimulationState::Waiting,
        ] {
            controller
                .send(state_update(1, &id, state, SystemTime::now()))
                .await
                .unwrap();
            let mut commands = commands.lock().unwrap();
            let targeted = commands
                .iter()
                .filter(|command| {
                    command.target == Some(grpc::controller_command::Target::AgentId(1))
                })
                .collect::<Vec<_>>();
            assert_eq!(targeted.len(), 1);
            assert_eq!(targeted[0].commands.len(), 1);
            assert!(matches!(&targeted[0].commands[0].command,
                Some(Command::Launch(launch)) if launch.start_ts == Some(start_ts.into())));
            commands.clear();
        }
    }

    #[actix::test]
    async fn older_and_duplicate_heartbeats_do_not_trigger_reconciliation() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        let timestamp = SystemTime::now();
        controller
            .send(state_update(
                1,
                &id,
                grpc::AgentSimulationState::Ready,
                timestamp,
            ))
            .await
            .unwrap();
        controller
            .send(StartSimulation::at(timestamp))
            .await
            .unwrap()
            .unwrap();
        commands.lock().unwrap().clear();
        for ts in [timestamp, timestamp - Duration::from_secs(1)] {
            controller
                .send(state_update(1, "old", grpc::AgentSimulationState::Idle, ts))
                .await
                .unwrap();
        }
        assert!(commands.lock().unwrap().is_empty());
        let status = controller.send(GetSimulationStatus).await.unwrap();
        assert_eq!(status.agents[0].simulation_id, id);
        assert_eq!(status.agents[0].state, grpc::AgentSimulationState::Ready);
    }

    #[actix::test]
    async fn different_agents_receive_only_the_recovery_they_need() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        controller
            .send(StartSimulation::at(SystemTime::now()))
            .await
            .unwrap()
            .unwrap();
        commands.lock().unwrap().clear();
        let mut updates =
            state_update(1, &id, grpc::AgentSimulationState::Ready, SystemTime::now());
        updates.0.extend(
            state_update(
                2,
                "old",
                grpc::AgentSimulationState::Running,
                SystemTime::now(),
            )
            .0,
        );
        controller.send(updates).await.unwrap();
        let commands = commands.lock().unwrap();
        let targeted = |id| {
            commands
                .iter()
                .find(|command| {
                    command.target == Some(grpc::controller_command::Target::AgentId(id))
                })
                .unwrap()
        };
        assert_eq!(targeted(1).commands.len(), 1);
        assert!(matches!(
            targeted(1).commands[0].command,
            Some(Command::Launch(_))
        ));
        assert_eq!(loaded_ids(&[targeted(2).clone()]), vec![id]);
        assert!(targeted(2)
            .commands
            .iter()
            .any(|item| matches!(item.command, Some(Command::Launch(_)))));
        assert!(!targeted(2)
            .commands
            .iter()
            .any(|item| matches!(item.command, Some(Command::Stop(_)))));
    }

    #[actix::test]
    async fn ready_reconciliation_stops_without_unloading_the_definition() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        commands.lock().unwrap().clear();
        controller
            .send(state_update(
                1,
                &id,
                grpc::AgentSimulationState::Running,
                SystemTime::now(),
            ))
            .await
            .unwrap();
        let commands = commands.lock().unwrap();
        let targeted = commands
            .iter()
            .find(|command| command.target == Some(grpc::controller_command::Target::AgentId(1)))
            .unwrap();
        assert_eq!(targeted.commands.len(), 1);
        assert!(matches!(
            targeted.commands[0].command,
            Some(Command::Stop(StopCommand { reset: false }))
        ));
    }

    #[actix::test]
    async fn other_agent_updates_do_not_retry_cached_misalignment() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        controller
            .send(StartSimulation::at(SystemTime::now()))
            .await
            .unwrap()
            .unwrap();
        controller.send(update(1, "old", "")).await.unwrap();
        commands.lock().unwrap().clear();
        controller
            .send(state_update(
                2,
                &id,
                grpc::AgentSimulationState::Running,
                SystemTime::now(),
            ))
            .await
            .unwrap();
        assert!(loaded_ids(&commands.lock().unwrap()).is_empty());
    }

    #[actix::test]
    async fn invalid_definitions_are_rejected_before_reaching_agents() {
        let (controller, commands) = started_controller();
        let out = controller
            .send(LoadSimulation::new(definition("10 *")))
            .await
            .unwrap();
        assert!(matches!(
            out,
            Err(SimulationControlError::InvalidSimulation(_))
        ));
        assert!(commands.lock().unwrap().is_empty());
        let status = controller.send(GetSimulationStatus).await.unwrap();
        assert_eq!(status.phase, SimulationPhase::Idle);
    }

    #[actix::test]
    async fn loading_assigns_an_identifier_sent_to_the_agents() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded_ids(&commands.lock().unwrap()), vec![id.clone()]);
        let status = controller.send(GetSimulationStatus).await.unwrap();
        assert_eq!(status.phase, SimulationPhase::Ready);
        assert_eq!(status.simulation_id, Some(id));
    }

    #[actix::test]
    async fn starting_without_a_loaded_simulation_fails() {
        let (controller, commands) = started_controller();
        let out = controller
            .send(StartSimulation::at(SystemTime::now()))
            .await
            .unwrap();
        assert!(matches!(out, Err(SimulationControlError::NotLoaded)));
        assert!(commands.lock().unwrap().is_empty());
    }

    #[actix::test]
    async fn agents_that_failed_the_current_simulation_are_not_reloaded() {
        let (controller, commands) = started_controller();
        let id = controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();

        controller
            .send(update(1, &id, "Invalid script"))
            .await
            .unwrap();
        controller
            .send(update(1, &id, "Invalid script"))
            .await
            .unwrap();
        // Only the initial broadcast and the membership update after the first report.
        assert_eq!(loaded_ids(&commands.lock().unwrap()), vec![id.clone()]);

        // An agent reporting an older simulation is realigned.
        controller.send(update(2, "previous", "")).await.unwrap();
        assert_eq!(
            loaded_ids(&commands.lock().unwrap()),
            vec![id.clone(), id.clone()]
        );

        let status = controller.send(GetSimulationStatus).await.unwrap();
        let failed = status
            .agents
            .iter()
            .find(|agent| agent.agent_id == 1)
            .unwrap();
        assert_eq!(failed.simulation_error.as_deref(), Some("Invalid script"));
    }

    #[actix::test]
    async fn stopping_unloads_the_simulation_from_every_agent() {
        let (controller, commands) = started_controller();
        controller
            .send(LoadSimulation::new(definition("10")))
            .await
            .unwrap()
            .unwrap();
        controller
            .send(StartSimulation::at(SystemTime::now()))
            .await
            .unwrap()
            .unwrap();
        controller.send(StopSimulation).await.unwrap();

        let status = controller.send(GetSimulationStatus).await.unwrap();
        assert_eq!(status.phase, SimulationPhase::Idle);
        assert_eq!(status.simulation_id, None);
        let commands = commands.lock().unwrap();
        let last = commands.last().unwrap();
        assert!(last.commands.iter().any(|item| matches!(
            item.command,
            Some(Command::Stop(StopCommand { reset: true }))
        )));
        assert!(!last
            .commands
            .iter()
            .any(|item| matches!(item.command, Some(Command::Load(_)))));
    }
}
