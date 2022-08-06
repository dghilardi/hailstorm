use std::collections::HashMap;
use std::ops::Add;
use std::time::{Duration, SystemTime};

use actix::{Actor, ActorFutureExt, ActorTryFutureExt, Addr, AsyncContext, Context, Handler, MailboxError, Recipient, ResponseFuture, WrapFuture};
use actix::dev::Request;
use futures::future::ok;
use futures::{join, StreamExt};
use rand::{Rng, thread_rng};
use tokio::sync::mpsc::Receiver;
use crate::agent::metrics::manager_actor::{ActionMetricsFamilySnapshot, FetchActionMetrics, MetricsManagerActor};

use crate::communication::protobuf::grpc::{AgentUpdate, ControllerCommand};
use crate::communication::message::{ControllerCommandMessage, SendAgentMessage};
use crate::communication::notifier_actor::{RegisterAgentUpdateSender, UpdatesNotifierActor};
use crate::communication::server_actor::GrpcServerActor;
use crate::MultiAgentUpdateMessage;
use crate::communication::protobuf::grpc;
use crate::communication::protobuf::grpc::command_item::Command;
use crate::communication::protobuf::grpc::{ModelStateSnapshot, ModelStats, StopCommand};
use crate::simulation::actor::simulation::{ClientStats, FetchSimulationStats, SimulationActor, SimulationCommand, SimulationCommandLst, SimulationState, SimulationStats};
use crate::simulation::actor::bot::BotState;
use crate::utils::actix::synchro_context::WeakContext;

struct AggregatedBotStateMetric {
    timestamp: SystemTime,
    model: String,
    state: BotState,
    count: usize,
}

pub struct AgentCoreActor {
    agent_id: u32,
    notifier_addr: Addr<UpdatesNotifierActor>,
    cmd_recipient: Recipient<ControllerCommandMessage>,
    simulation_addr: Addr<SimulationActor>,
    metrics_addr: Addr<MetricsManagerActor>,
    last_sent_metrics: Vec<AggregatedBotStateMetric>,
}

impl AgentCoreActor {
    pub fn new<ServerActor>(
        agent_id: u32,
        notifier_addr: Addr<UpdatesNotifierActor>,
        server_addr: Addr<ServerActor>,
        simulation_addr: Addr<SimulationActor>,
        metrics_addr: Addr<MetricsManagerActor>,
    ) -> Self
    where
        ServerActor: Actor<Context=Context<ServerActor>> + Handler<ControllerCommandMessage>,
    {
        Self {
            agent_id,
            notifier_addr,
            cmd_recipient: server_addr.recipient(),
            simulation_addr,
            metrics_addr,
            last_sent_metrics: vec![],
        }
    }

    fn fetch_perf_data(&mut self) -> Request<MetricsManagerActor, FetchActionMetrics> {
        self.metrics_addr.send(FetchActionMetrics)
    }

    fn fetch_state_data(&mut self) -> Request<SimulationActor, FetchSimulationStats> {
        self.simulation_addr.send(FetchSimulationStats)
    }

    fn send_data(&mut self, ctx: &mut actix::Context<Self>) {
        let agent_id = self.agent_id;
        let notifier_addr = self.notifier_addr.clone();
        let fetch_perf_req = self.fetch_perf_data();
        let fetch_state_req = self.fetch_state_data();

        let fut = async move {
            let (perf_res, state_res) = join!(fetch_perf_req, fetch_state_req);
            { Ok((perf_res?, state_res?)) }
                .map_err(|err: MailboxError| {
                    log::error!("Error fetching stats - {err}");
                    err
                })
        }
            .into_actor(self)
            .and_then(move |(in_perf, in_stats): (Vec<ActionMetricsFamilySnapshot>, SimulationStats), act, _ctx| {
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
                            states: vec![v],
                            performance: in_perf.iter()
                                .filter(|p| p.key.model.eq(&model))
                                .flat_map(|metr_fam| metr_fam.to_protobuf()).collect(),
                            model,
                        })
                        .collect(),
                    update_id: thread_rng().gen(),
                    timestamp: Some(SystemTime::now().into()),
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
                            self.last_sent_metrics.push(AggregatedBotStateMetric {
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
        ctx.run_interval_synchro(Duration::from_secs(3), |actor, ctx| actor.send_data(ctx));
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
        let server_addr = self.cmd_recipient.clone();
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