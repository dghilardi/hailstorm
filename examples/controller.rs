use std::collections::HashMap;
use std::fs;
use std::net::ToSocketAddrs;
use std::ops::Add;
use std::pin::Pin;
use std::time::{Duration, SystemTime};
use config::{Config, ConfigError, Environment, File};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
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
        tokio::spawn(handle_messages(request.into_inner()));
        let cmd_stream = futures::stream::iter(vec![
            ControllerCommand {
                command: Some(self.load_command.clone())
            },
            ControllerCommand {
                command: Some(Command::Launch(LaunchCommand {
                    start_ts: Some(self.start_ts.into())
                }))
            },
        ]).map(Ok);
        Ok(Response::new(Box::pin(cmd_stream)))
    }
}

async fn handle_messages(mut msg_stream: Streaming<AgentMessage>) {
    while let Some(msg) = msg_stream.next().await {
        log::info!("Received msg: {msg:?}");
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
        start_ts: SystemTime::now().add(Duration::from_secs(5)),
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