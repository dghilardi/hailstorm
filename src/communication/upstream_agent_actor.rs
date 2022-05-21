use actix::{Actor, Addr, Context};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use crate::agent::actor::{AgentCoreActor, RegisterAgentClientMsg};
use crate::communication::grpc::hailstorm_service_client::HailstormServiceClient;

pub struct UpstreamAgentActor {
    core_addr: Addr<AgentCoreActor>,
    client: HailstormServiceClient<Channel>,
}

impl Actor for UpstreamAgentActor {
    type Context = Context<Self>;
}


impl UpstreamAgentActor {
    pub async fn new(url: String, core_addr: Addr<AgentCoreActor>) -> Result<Self, tonic::transport::Error> {
        let mut client = HailstormServiceClient::connect(url).await?;

        let (tx, rx) = mpsc::channel(128);
        let cmd_stream = client.join(ReceiverStream::new(rx)).await.expect("error creating stream")
            .into_inner();

        core_addr.send(RegisterAgentClientMsg {
            cmd_stream,
            msg_sender: tx,
        }).await;

        Ok(Self {
            core_addr,
            client,
        })
    }
}