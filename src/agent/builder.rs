use std::collections::HashMap;
use std::net::SocketAddr;
use actix::Actor;
use futures::future::try_join_all;
use tonic::transport::Server;
use crate::agent::actor::AgentCoreActor;
use crate::communication::client_actor::AgentClientActor;
use crate::communication::grpc;
use crate::communication::notifier_actor::UpdatesNotifierActor;
use crate::communication::server::HailstormGrpcServer;
use crate::communication::server_actor::HailstormServerActor;

pub struct AgentBuilder {
    pub agent_id: u64,
    pub address: SocketAddr,
    pub upstream: HashMap<String, String>,
}

impl AgentBuilder {
    pub async fn launch(self) {
        let updater_addr = UpdatesNotifierActor::create(|_| UpdatesNotifierActor::new());
        let core_addr = AgentCoreActor::create(|_| AgentCoreActor::new(
            self.agent_id,
            updater_addr.clone(),
        ));

        if self.upstream.is_empty() {
            log::warn!("No parents defined");
        }

        let clients_fut = self.upstream
            .into_iter()
            .map(|(_k, url)| AgentClientActor::new(url, core_addr.clone()));

        let clients = try_join_all(clients_fut).await
            .expect("Error building clients")
            .into_iter()
            .map(Actor::start)
            .collect::<Vec<_>>();

        let server_actor = HailstormServerActor::create(|_| HailstormServerActor::new(updater_addr.clone()));
        let hailstorm_server = HailstormGrpcServer::new(server_actor);
        Server::builder()
            .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server))
            .serve(self.address)
            .await
            .unwrap();
    }
}