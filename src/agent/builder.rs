use std::net::SocketAddr;
use actix::Actor;
use tonic::transport::Server;
use crate::communication::grpc;
use crate::communication::notifier_actor::UpdatesNotifierActor;
use crate::communication::server::HailstormGrpcServer;
use crate::communication::server_actor::HailstormServerActor;

pub struct AgentBuilder {
    pub address: SocketAddr,
}

impl AgentBuilder {
    pub async fn launch(self) {
        let updater_addr = UpdatesNotifierActor::create(|_| UpdatesNotifierActor::new());

        let server_actor = HailstormServerActor::create(|_| HailstormServerActor::new(updater_addr.clone()));
        let hailstorm_server = HailstormGrpcServer::new(server_actor);
        let tonic_server = Server::builder()
            .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server))
            .serve(self.address)
            .await
            .unwrap();
    }
}