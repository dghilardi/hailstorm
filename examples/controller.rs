use std::collections::HashMap;
use std::fs;
use std::net::ToSocketAddrs;
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::time::{Duration, SystemTime};
use config::{Config, ConfigError, Environment, File};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tonic::transport::Server;
use hailstorm::grpc::{self, AgentMessage, AgentUpdate, ClientDistribution, ControllerCommand, LaunchCommand, LoadSimCommand};
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
        tokio::spawn(handle_messages(request.into_inner(), tx));
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
    let mut registered_agents = HashMap::<u64, AgentUpdate>::new();
    while let Some(msg_res) = msg_stream.next().await {
        match msg_res {
            Ok(msg) => {
                let prev_agent_count = registered_agents.len();
                for fragment in msg.updates {
                    registered_agents.insert(fragment.agent_id, fragment);
                }
                registered_agents
                    .retain(|_idx, update|
                        update.timestamp.as_ref()
                            .map(|ts| SystemTime::now().sub(Duration::from_secs(15)) < SystemTime::try_from(ts.clone()).unwrap())
                            .unwrap_or(false)
                    );

                let summary: HashMap<String, HashMap<u32, u32>> = registered_agents.iter()
                    .fold(HashMap::new(), |mut acc, (_, upd)| {
                        for model_stats in &upd.stats {
                            let model_acc = acc.entry(model_stats.model.clone())
                                .or_insert_with(HashMap::new);
                            for state_stats in &model_stats.states {
                                let acc_state_stats = model_acc.entry(state_stats.state_id)
                                    .or_insert(0);
                                *acc_state_stats += state_stats.count;
                            }
                        }
                        acc
                    });
                log::debug!("registered agents: {registered_agents:?}");
                log::debug!("summary: {summary:?}");
                print_summary(summary);

                if prev_agent_count != registered_agents.len() {
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

fn print_summary(summary: HashMap<String, HashMap<u32, u32>>) {
    for (model, model_stats) in summary {
        log::info!("== {model} ==");
        for (state, count) in model_stats {
            log::info!(" - [{state}] -> {count}");
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
    Server::builder()
        .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server))
        .serve(config.address.to_socket_addrs().unwrap().next().unwrap())
        .await
        .unwrap();
}