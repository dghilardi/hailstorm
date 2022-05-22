use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::ToSocketAddrs;
use std::ops::Add;
use std::pin::Pin;
use std::time::{Duration, SystemTime};
use config::{Config, ConfigError, Environment, File};
use futures::{Stream, StreamExt, FutureExt, TryStreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tonic::transport::Server;
use hailstorm::grpc::{self, AgentMessage, ClientDistribution, ControllerCommand, LaunchCommand, LoadSimCommand};
use hailstorm::grpc::controller_command::Command;
use hailstorm::grpc::hailstorm_service_server::HailstormService;

pub struct EchoHailstormServer {
    start_ts: SystemTime,
    load_command: Command
}

type ResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[tonic::async_trait]
impl HailstormService for EchoHailstormServer {
    type JoinStream = ResponseStream<ControllerCommand>;

    async fn join(&self, request: Request<Streaming<AgentMessage>>) -> Result<Response<Self::JoinStream>, Status> {
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(initialize_agents(self.start_ts, self.load_command.clone(), tx.clone()));
        tokio::spawn(handle_messages(request.into_inner(), tx.clone()));
        Ok(Response::new(Box::pin(ReceiverStream::new(rx).map(Ok))))
    }
}

async fn initialize_agents(
    start_ts: SystemTime,
    load_command: Command,
    sender: Sender<ControllerCommand>
) {
    actix::clock::sleep(Duration::from_secs(5)).await;
    sender.send(ControllerCommand { command: Some(load_command.clone()) }).await.expect("Error sending load command");
    sender.send(ControllerCommand {
        command: Some(Command::Launch(LaunchCommand { start_ts: Some(start_ts.into()) }))
    }).await.expect("Error sending launch command");
}

async fn handle_messages(mut msg_stream: Streaming<AgentMessage>, sender: Sender<ControllerCommand>) {
    let mut registered_agents = HashSet::<u64>::new();
    while let Some(msg_res) = msg_stream.next().await {
        match msg_res {
            Ok(msg) => {
                log::info!("Received msg: {msg:?}");
                let new_agents = msg.updates.iter()
                    .map(|upd| upd.agent_id)
                    .filter(|agent_id| !registered_agents.contains(&agent_id))
                    .collect::<HashSet<_>>();

                if new_agents.len() > 0 {
                    for agent_id in new_agents {
                        registered_agents.insert(agent_id);
                    }
                    sender.send(ControllerCommand {
                        command: Some(Command::UpdateAgentsCount(registered_agents.len() as u32))
                    }).await.expect("Error sending UpdateAgentsCount")
                }
            }
            Err(err) => {
                log::error!("Error receiving message {err}");}
        }
    }
}

#[derive(Deserialize)]
pub struct HailstormAgentConfig {
    pub address: String,
    pub clients_distribution: HashMap<String, String>,
    pub script_path: String,
}

pub fn compose_config<'de, CFG: Deserialize<'de>>(external_path: &str) -> Result<CFG, ConfigError> {
    Config::builder()

        // Start off by local configuration file
        .add_source(File::with_name(external_path).required(false))

        // Add in settings from the environment (with a prefix of hs)
        .add_source(Environment::with_prefix("hs"))

        .build()?
        .try_deserialize()
}

#[actix::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();
    let config: HailstormAgentConfig = compose_config("config/hailstorm-controller")
        .expect("Error loading config");

    log::info!("Starting controller ...");
    let hailstorm_server = EchoHailstormServer {
        start_ts: SystemTime::now().add(Duration::from_secs(10)),
        load_command: Command::Load(LoadSimCommand {
            clients_evolution: config.clients_distribution.into_iter()
                .map(|(model, shape)| ClientDistribution { model, shape })
                .collect(),
            script: fs::read_to_string(config.script_path).expect("Error loading script file")
        })
    };
    let tonic_server = Server::builder()
        .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server))
        .serve(config.address.to_socket_addrs().unwrap().next().unwrap())
        .await
        .unwrap();
}