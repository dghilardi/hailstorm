use std::collections::HashMap;
use std::net::SocketAddr;
use actix::{Actor, Addr, AsyncContext, Context};
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

pub struct AgentBuilder<ContextBuilder, UpstreamCfg, DownstreamCfg> {
    pub agent_id: u32,
    pub max_running_bots: usize,
    pub downstream: DownstreamCfg,
    pub upstream: HashMap<String, UpstreamCfg>,
    pub rune_context_builder: ContextBuilder,
}

pub struct AgentRuntime<Upstream: UpstreamAgentActor> {
    pub server: Addr<GrpcServerActor>,
    pub clients: Vec<Addr<Upstream>>,
}

impl<ContextBuilder, UpstreamCfg, DownstreamCfg> AgentBuilder<ContextBuilder, UpstreamCfg, DownstreamCfg>
    where
        ContextBuilder: FnOnce(Addr<SimulationActor>) -> rune::Context,
{
    pub fn launch<Upstream: UpstreamAgentActor<Config=UpstreamCfg>>(self) -> AgentRuntime<Upstream> {
        let metrics_addr = MetricsManagerActor::start_default();

        let simulation_ctx: Context<SimulationActor> = Context::new();

        let rune_context = (self.rune_context_builder)(simulation_ctx.address());
        let bot_registry = BotRegistry::new(rune_context, metrics_addr.clone()).expect("Error during registry construction");

        let updater_addr = UpdatesNotifierActor::create(|_| UpdatesNotifierActor::new());
        let server_actor = GrpcServerActor::create(|_| GrpcServerActor::new(updater_addr.clone().recipient()));
        let simulation_actor = simulation_ctx.run(SimulationActor::new(self.agent_id, self.max_running_bots, bot_registry));
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

        let clients = Self::initialize_clients::<Upstream>(self.upstream, core_addr)
            .expect("Error initializing clients");

        AgentRuntime {
            server: server_actor,
            clients,
        }
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

impl<ContextBuilder> AgentBuilder<ContextBuilder, String, SocketAddr>
    where
        ContextBuilder: FnOnce(Addr<SimulationActor>) -> rune::Context,
{
    pub async fn launch_grpc(self) {
        let address = self.downstream.clone();
        let runtime = self.launch::<GrpcUpstreamAgentActor>();

        let hailstorm_server = HailstormGrpcServer::new(runtime.server.recipient());
        Server::builder()
            .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server))
            .serve(address)
            .await
            .unwrap();
    }
}