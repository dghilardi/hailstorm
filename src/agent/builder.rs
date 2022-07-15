use std::collections::HashMap;
use std::net::SocketAddr;
use actix::{Actor, Addr, AsyncContext, Context};
use futures::future::try_join_all;
use tonic::transport::Server;
use crate::agent::actor::AgentCoreActor;
use crate::agent::metrics::manager_actor::MetricsManagerActor;
use crate::communication::upstream_agent_actor::UpstreamAgentActor;
use crate::communication::protobuf::grpc;
use crate::communication::notifier_actor::UpdatesNotifierActor;
use crate::communication::server::HailstormGrpcServer;
use crate::communication::server_actor::GrpcServerActor;
use crate::simulation::actor::simulation::SimulationActor;
use crate::simulation::bot::registry::BotRegistry;

pub struct AgentBuilder <ContextBuilder> {
    pub agent_id: u32,
    pub address: SocketAddr,
    pub upstream: HashMap<String, String>,
    pub rune_context_builder: ContextBuilder,
}

impl <ContextBuilder> AgentBuilder<ContextBuilder>
where
    ContextBuilder: FnOnce(Addr<SimulationActor>) -> rune::Context,
{
    pub async fn launch(self) {
        let metrics_addr = MetricsManagerActor::start_default();

        let simulation_ctx: Context<SimulationActor> = Context::new();

        let rune_context = (self.rune_context_builder)(simulation_ctx.address());
        let bot_registry = BotRegistry::new(rune_context, metrics_addr.clone()).expect("Error during registry construction");

        let updater_addr = UpdatesNotifierActor::create(|_| UpdatesNotifierActor::new());
        let server_actor = GrpcServerActor::create(|_| GrpcServerActor::new(updater_addr.clone().recipient()));
        let simulation_actor = simulation_ctx.run(SimulationActor::new(self.agent_id, bot_registry));
        let core_addr = AgentCoreActor::create(|_| AgentCoreActor::new(
            self.agent_id,
            updater_addr.clone(),
            server_actor.clone(),
            simulation_actor,
            metrics_addr,
        ));

        if self.upstream.is_empty() {
            log::warn!("No parents defined");
        }

        let clients_fut = self.upstream
            .into_iter()
            .map(|(_k, url)| UpstreamAgentActor::new(url, core_addr.clone()));

        let clients = try_join_all(clients_fut).await
            .expect("Error building clients")
            .into_iter()
            .map(Actor::start)
            .collect::<Vec<_>>();

        let hailstorm_server = HailstormGrpcServer::new(server_actor.recipient());
        Server::builder()
            .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server))
            .serve(self.address)
            .await
            .unwrap();
    }
}