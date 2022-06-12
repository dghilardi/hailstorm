use std::future::ready;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::{Actor, ActorFutureExt, ActorTryFutureExt, Addr, AsyncContext, Context, Handler, StreamHandler, WrapFuture};
use futures::future::ok;
use futures::StreamExt;
use rand::{Rng, thread_rng};
use tokio::sync::mpsc::Sender;
use tonic::Streaming;

use crate::communication::grpc::{AgentMessage, AgentUpdate, ControllerCommand};
use crate::communication::message::ControllerCommandMessage;
use crate::communication::notifier_actor::{AgentUpdateMessage, RegisterAgentUpdateSender, UpdatesNotifierActor};
use crate::communication::server_actor::GrpcServerActor;
use crate::grpc;
use crate::grpc::controller_command::Command;
use crate::grpc::StopCommand;
use crate::simulation::simulation_actor::{ClientStats, FetchSimulationStats, SimulationActor, SimulationCommand, SimulationState, SimulationStats};
use crate::simulation::user_actor::UserState;

struct AggregatedUserStateMetric {
    timestamp: SystemTime,
    model: String,
    state: UserState,
    count: usize,
}

pub struct AgentCoreActor {
    agent_id: u64,
    notifier_addr: Addr<UpdatesNotifierActor>,
    server_addr: Addr<GrpcServerActor>,
    simulation_addr: Addr<SimulationActor>,
    last_sent_metrics: Vec<AggregatedUserStateMetric>,
}

impl AgentCoreActor {
    pub fn new(
        agent_id: u64,
        notifier_addr: Addr<UpdatesNotifierActor>,
        server_addr: Addr<GrpcServerActor>,
        simulation_addr: Addr<SimulationActor>,
    ) -> Self {
        Self {
            agent_id,
            notifier_addr,
            server_addr,
            simulation_addr,
            last_sent_metrics: vec![],
        }
    }


    fn send_data(&mut self, ctx: &mut actix::Context<Self>) {
        let agent_id = self.agent_id;
        let notifier_addr = self.notifier_addr.clone();
        let simulation_addr = self.simulation_addr.clone();

        let fut = async move {
            simulation_addr.send(FetchSimulationStats).await
                .map_err(|err| {
                    log::error!("Error fetching simulation stats - {err}");
                    err
                })
        }
            .into_actor(self)
            .and_then(move |in_stats: SimulationStats, act, _ctx| {
                let state = match in_stats.state {
                    SimulationState::Idle => grpc::AgentSimulationState::Idle,
                    SimulationState::Ready => grpc::AgentSimulationState::Ready,
                    SimulationState::Waiting => grpc::AgentSimulationState::Waiting,
                    SimulationState::Running => grpc::AgentSimulationState::Running,
                    SimulationState::Stopping => grpc::AgentSimulationState::Stopping,
                };

                let stats = act.update_simulation_stats(in_stats.stats, in_stats.timestamp)
                    .into_iter()
                    .map(Into::into)
                    .collect();

                notifier_addr.try_send(AgentUpdateMessage(AgentUpdate {
                    agent_id,
                    stats,
                    update_id: thread_rng().gen(),
                    timestamp: Some(in_stats.timestamp.into()),
                    name: "".to_string(),
                    state: state as i32,
                    simulation_id: "".to_string(),
                })).unwrap_or_else(|err| {
                    log::error!("Error sending agent stats to notifier actor {err}");
                });

                ok(())
            }).map(|res, _act, _ctx| {
                if let Err(err) = res {
                    log::error!("Error sending agent stats {err}");
                }
            });

        ctx.spawn(fut);
    }

    fn update_simulation_stats(&mut self, stats: Vec<ClientStats>, timestamp: SystemTime) -> Vec<ClientStats> {
        stats.into_iter()
            .map(|mut client| {

                self.last_sent_metrics.iter()
                    .filter(|m| m.model.eq(&client.model) && !client.count_by_state.contains_key(&m.state))
                    .collect::<Vec<_>>().into_iter()
                    .for_each(|m| { client.count_by_state.insert(m.state, 0); });

                client.count_by_state = client.count_by_state
                    .into_iter()
                    .filter(|(state, count)| {
                        if let Some(sent_metrics) = self.last_sent_metrics.iter_mut().find(|m| m.model.eq(&client.model) && m.state.eq(state)) {
                            if *count != sent_metrics.count || (*count > 0 && sent_metrics.timestamp.add(Duration::from_secs(25)) < timestamp) {
                                sent_metrics.count = *count;
                                sent_metrics.timestamp = timestamp;
                                true
                            } else {
                                false
                            }
                        } else {
                            self.last_sent_metrics.push(AggregatedUserStateMetric {
                                timestamp,
                                model: client.model.clone(),
                                state: *state,
                                count: *count
                            });
                            true
                        }
                    })
                    .collect();
                client
            })
            .collect()
    }
}

impl Actor for AgentCoreActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_secs(3), |actor, ctx| actor.send_data(ctx));
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

impl From<&Command> for Option<SimulationCommand> {
    fn from(cmd: &Command) -> Self {
        match cmd {
            Command::Load(load) => Some(SimulationCommand::LoadSimulation {
                model_shapes: load.clients_evolution.iter()
                    .map(|cd| (cd.model.clone(), cd.shape.clone()))
                    .collect(),
                script: load.script.clone(),
            }),
            Command::Launch(launch) => launch.start_ts.clone()
                .and_then(|ts| ts.try_into()
                    .map_err(|err| log::error!("Error converting timestamp to systemtime - {err}"))
                    .ok()
                ).map(|start_ts| SimulationCommand::LaunchSimulation { start_ts }),
            Command::UpdateAgentsCount(count) => Some(SimulationCommand::UpdateAgentsCount { count: *count }),
            Command::Stop(StopCommand { reset }) => Some(SimulationCommand::StopSimulation { reset: *reset }),
        }
    }
}

impl StreamHandler<ConnectedClientMessage> for AgentCoreActor {
    fn handle(&mut self, ConnectedClientMessage { message, .. }: ConnectedClientMessage, _ctx: &mut Self::Context) {
        log::debug!("message: {message:?}");
        let maybe_sim_command: Option<SimulationCommand> = message.command.as_ref()
            .and_then(From::from);

        if let Some(sim_command) = maybe_sim_command {
            self.simulation_addr.try_send(sim_command)
                .unwrap_or_else(|err| log::error!("Error sending simulation command - {err}"));
        }

        self.server_addr.try_send(ControllerCommandMessage(message))
            .unwrap_or_else(|err| log::error!("Error sending command to server actor - {err}"));
    }

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler started")
    }

    fn finished(&mut self, _ctx: &mut Self::Context) {
        log::debug!("ConnectedAgentMessage stream handler finished")
    }
}