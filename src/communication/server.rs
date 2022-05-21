use std::pin::Pin;
use actix::Addr;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Code, Request, Response, Status, Streaming};
use crate::communication::grpc::{AgentMessage, ControllerCommand};
use crate::communication::grpc::hailstorm_service_server::HailstormService;
use crate::communication::server_actor::{HailstormServerActor, RegisterConnectedAgentMsg};

pub struct HailstormGrpcServer {
    server_actor_addr: Addr<HailstormServerActor>,
}

impl HailstormGrpcServer {
    pub fn new(
        server_actor_addr: Addr<HailstormServerActor>,
    ) -> Self {
        Self {
            server_actor_addr
        }
    }
}

type ResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[tonic::async_trait]
impl HailstormService for HailstormGrpcServer {
    type JoinStream = ResponseStream<ControllerCommand>;

    async fn join(&self, request: Request<Streaming<AgentMessage>>) -> Result<Response<Self::JoinStream>, Status> {
        let (tx, rx) = mpsc::channel(128);
        self.server_actor_addr.send(RegisterConnectedAgentMsg {
            cmd_sender: tx,
            states_stream: request.into_inner()
        })
            .await
            .map_err(|err| Status::new(Code::Internal, err.to_string()))?;
        Ok(Response::new(Box::pin(ReceiverStream::new(rx).map(Ok))))
    }
}