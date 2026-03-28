use crate::communication::protobuf::grpc::hailstorm_service_server::HailstormService;
use crate::communication::protobuf::grpc::{AgentMessage, ControllerCommand};
use actix::Recipient;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Code, Request, Response, Status, Streaming};

/// Message sent when a new agent connects via gRPC, carrying its update stream
/// and a channel for sending commands back to it.
#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct RegisterConnectedAgentMsg {
    /// Incoming stream of agent update messages.
    pub states_stream: Streaming<AgentMessage>,
    /// Channel for sending controller commands to this agent.
    pub cmd_sender: Sender<ControllerCommand>,
}

/// gRPC server implementing the [`HailstormService`] trait.
///
/// Delegates incoming agent connections to the [`GrpcServerActor`](super::server_actor::GrpcServerActor)
/// via the [`RegisterConnectedAgentMsg`] message.
pub struct HailstormGrpcServer {
    server_actor_addr: Recipient<RegisterConnectedAgentMsg>,
}

impl HailstormGrpcServer {
    pub fn new(server_actor_addr: Recipient<RegisterConnectedAgentMsg>) -> Self {
        Self { server_actor_addr }
    }
}

type ResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[tonic::async_trait]
impl HailstormService for HailstormGrpcServer {
    type JoinStream = ResponseStream<ControllerCommand>;

    async fn join(
        &self,
        request: Request<Streaming<AgentMessage>>,
    ) -> Result<Response<Self::JoinStream>, Status> {
        let (tx, rx) = mpsc::channel(128);
        self.server_actor_addr
            .send(RegisterConnectedAgentMsg {
                cmd_sender: tx,
                states_stream: request.into_inner(),
            })
            .await
            .map_err(|err| Status::new(Code::Internal, err.to_string()))?;
        Ok(Response::new(Box::pin(ReceiverStream::new(rx).map(Ok))))
    }
}
