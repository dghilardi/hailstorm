use std::collections::HashMap;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::{Actor, ActorFutureExt, ActorTryFutureExt, Addr, AsyncContext, Context, Handler, Recipient, ResponseFuture, WrapFuture};
use futures::future::ok;
use futures::StreamExt;
use rand::{Rng, thread_rng};
use tokio::sync::mpsc::Receiver;

use crate::communication::protobuf::grpc::{AgentUpdate, ControllerCommand};
use crate::communication::message::{ControllerCommandMessage, SendAgentMessage};
use crate::communication::notifier_actor::{RegisterAgentUpdateSender, UpdatesNotifierActor};
use crate::communication::server_actor::GrpcServerActor;
use crate::MultiAgentUpdateMessage;
use crate::communication::protobuf::grpc;
use crate::communication::protobuf::grpc::command_item::Command;
use crate::communication::protobuf::grpc::{ModelStateSnapshot, ModelStats, StopCommand};
use crate::simulation::simulation_actor::{ClientStats, FetchSimulationStats, SimulationActor, SimulationCommand, SimulationCommandLst, SimulationState, SimulationStats};
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

                let model_states = act.update_simulation_stats(in_stats.stats, in_stats.timestamp)
                    .into_iter()
                    .map(|cs| (cs.model.clone(), ModelStateSnapshot::from(cs)))
                    .collect::<HashMap<_, _>>();

                notifier_addr.try_send(MultiAgentUpdateMessage(vec![AgentUpdate {
                    agent_id,
                    stats: model_states.into_iter()
                        .map(|(model, v)| ModelStats {
                            model,
                            states: vec![v],
                            performance: vec![]
                        })
                        .collect(),
                    update_id: thread_rng().gen(),
                    name: "".to_string(),
                    state: state as i32,
                    simulation_id: "".to_string(),
                }])).unwrap_or_else(|err| {
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
                                count: *count,
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
    pub cmd_receiver: Receiver<ControllerCommand>,
    pub msg_sender: Recipient<SendAgentMessage>,
}

impl Handler<RegisterAgentClientMsg> for AgentCoreActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterAgentClientMsg, ctx: &mut Self::Context) -> Self::Result {
        self.notifier_addr
            .try_send(RegisterAgentUpdateSender(msg.msg_sender))
            .unwrap_or_else(|err| log::error!("Error registering agent update sender - {err:?}"));

        let cmd_stream = tokio_stream::wrappers::ReceiverStream::new(msg.cmd_receiver);
        ctx.add_message_stream(
            cmd_stream
                .map(move |message| ConnectedClientMessage { message })
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

impl Handler<ConnectedClientMessage> for AgentCoreActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, ConnectedClientMessage { message, .. }: ConnectedClientMessage, _ctx: &mut Self::Context) -> Self::Result {
        log::debug!("message: {message:?}");
        let sim_commands: Vec<SimulationCommand> = message.commands.iter()
            .filter_map(|ci| ci.command.as_ref())
            .filter_map(From::from)
            .collect();

        let sim_addr = self.simulation_addr.clone();
        let server_addr = self.server_addr.clone();
        let agent_id = self.agent_id;

        Box::pin(async move {
            if message.target.as_ref().map(|t| t.includes_agent(agent_id)).unwrap_or(true) {
                let sim_cmd_out = sim_addr.send(SimulationCommandLst { commands: sim_commands }).await;
                if let Err(err) = sim_cmd_out {
                    log::error!("Error sending simulation command - {err}");
                }
            }

            let srv_out = server_addr.send(ControllerCommandMessage(message)).await;
            if let Err(err) = srv_out {
                log::error!("Error sending command to server actor - {err}");
            }
        })
    }
}