use std::future::ready;
use std::time::Duration;

use actix::{Actor, Addr, AsyncContext, Context, Handler, MailboxError, StreamHandler};
use futures::StreamExt;
use rand::{Rng, thread_rng};
use tokio::sync::mpsc::Sender;
use tonic::Streaming;

use crate::communication::grpc::{AgentMessage, AgentUpdate, ControllerCommand};
use crate::communication::message::ControllerCommandMessage;
use crate::communication::notifier_actor::{AgentUpdateMessage, RegisterAgentUpdateSender, UpdatesNotifierActor};
use crate::communication::server_actor::HailstormServerActor;
use crate::grpc;
use crate::grpc::controller_command::Command;
use crate::simulation::simulation_actor::{FetchSimulationStats, SimulationActor, SimulationCommand, SimulationState};

pub struct AgentCoreActor {
    agent_id: u64,
    notifier_addr: Addr<UpdatesNotifierActor>,
    server_addr: Addr<HailstormServerActor>,
    simulation_addr: Addr<SimulationActor>,
}

impl AgentCoreActor {
    pub fn new(
        agent_id: u64,
        notifier_addr: Addr<UpdatesNotifierActor>,
        server_addr: Addr<HailstormServerActor>,
        simulation_addr: Addr<SimulationActor>,
    ) -> Self {
        Self {
            agent_id,
            notifier_addr,
            server_addr,
            simulation_addr,
        }
    }


    fn send_data(&mut self) {
        let agent_id = self.agent_id;
        let notifier_addr = self.notifier_addr.clone();
        let simulation_addr = self.simulation_addr.clone();
        actix::spawn(async move {
            let stats = simulation_addr.send(FetchSimulationStats).await
                .map_err(|err| {
                    log::error!("Error fetching simulation stats - {err}");
                    err
                })?;

            let state = match stats.state {
                SimulationState::Idle => grpc::AgentState::Idle,
                SimulationState::Ready => grpc::AgentState::Ready,
                SimulationState::Waiting => grpc::AgentState::Waiting,
                SimulationState::Running => grpc::AgentState::Running,
                SimulationState::Stopping => grpc::AgentState::Stopping,
            };

            notifier_addr.try_send(AgentUpdateMessage(AgentUpdate {
                agent_id,
                stats: stats.stats.into_iter()
                    .map(Into::into)
                    .collect(),
                update_id: thread_rng().gen(),
                timestamp: Some(stats.timestamp.into()),
                name: "".to_string(),
                state: state as i32,
                simulation_id: "".to_string(),
            })).unwrap_or_else(|err| {
                log::error!("Error sending agent stats to notifier actor {err}");
            });

            Result::<_, MailboxError>::Ok(())
        });
    }
}

impl Actor for AgentCoreActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_secs(3), |actor, _ctx| actor.send_data());
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
            Command::UpdateAgentsCount(count) => Some(SimulationCommand::UpdateAgentsCount { count: *count })
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