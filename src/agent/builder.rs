use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::Add;
use actix::{Actor, Addr, AsyncContext, Context};
use futures::future::try_join_all;
use tonic::transport::Server;
use crate::agent::actor::AgentCoreActor;
use crate::agent::metrics::manager_actor::MetricsManagerActor;
use crate::communication::upstream::grpc::GrpcUpstreamAgentActor;
use crate::communication::protobuf::grpc;
use crate::communication::notifier_actor::UpdatesNotifierActor;
use crate::communication::server::HailstormGrpcServer;
use crate::communication::server_actor::GrpcServerActor;
use crate::communication::upstream::contract::UpstreamAgentActor;
use crate::simulation::actor::simulation::SimulationActor;
use crate::simulation::bot::registry::BotRegistry;

pub struct AgentBuilder <ContextBuilder, UpstreamCfg> {
    pub agent_id: u32,
    pub address: SocketAddr,
    pub upstream: HashMap<String, UpstreamCfg>,
    pub rune_context_builder: ContextBuilder,
}

impl <ContextBuilder, UpstreamCfg> AgentBuilder<ContextBuilder, UpstreamCfg>
where
    ContextBuilder: FnOnce(Addr<SimulationActor>) -> rune::Context,
{
    pub async fn launch<Upstream: UpstreamAgentActor<Config=UpstreamCfg>>(self) {
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

        let clients = Self::initialize_clients::<Upstream>(self.upstream, core_addr);

        let hailstorm_server = HailstormGrpcServer::new(server_actor.recipient());
        Server::builder()
            .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server))
            .serve(self.address)
            .await
            .unwrap();
    }

    fn initialize_clients<Upstream: UpstreamAgentActor>(cfg: HashMap<String, Upstream::Config>, core_addr: Addr<AgentCoreActor>) -> Result<Vec<Addr<Upstream>>, Upstream::InitializationError> {
        let clients = cfg.into_iter()
            .map(|(_k, conf)| Upstream::new(conf, core_addr.clone()))
            .collect::<Result<Vec<Upstream>, _>>()?
            .into_iter()
            .map(Actor::start)
            .collect();
        Ok(clients)
    }
}