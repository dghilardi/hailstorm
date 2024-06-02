use crate::agent::actor::AgentCoreActor;
use crate::agent::metrics::manager::actor::MetricsManagerActor;
use crate::communication::notifier_actor::UpdatesNotifierActor;
use crate::communication::protobuf::grpc;
use crate::communication::server::HailstormGrpcServer;
use crate::communication::server_actor::GrpcServerActor;
use crate::communication::upstream::contract::UpstreamAgentActor;
use crate::communication::upstream::grpc::GrpcUpstreamAgentActor;
use crate::simulation::actor::simulation::{SimulationActor, SimulationParams};
use crate::simulation::bot::registry::BotRegistry;
use actix::{Actor, Addr, AsyncContext, Context};
use rand::{thread_rng, RngCore};
use std::collections::HashMap;
use std::net::SocketAddr;
use tonic::transport::Server;

pub struct AgentBuilder<ContextBuilder, UpstreamCfg, DownstreamCfg> {
    agent_id: u32,
    simulation_params: SimulationParams,
    downstream: DownstreamCfg,
    upstream: HashMap<String, UpstreamCfg>,
    rune_context_builder: ContextBuilder,
}

impl<UpstreamCfg> Default for AgentBuilder<(), UpstreamCfg, ()> {
    fn default() -> Self {
        Self {
            agent_id: thread_rng().next_u32(),
            simulation_params: SimulationParams::default(),
            downstream: (),
            upstream: Default::default(),
            rune_context_builder: (),
        }
    }
}

pub struct AgentRuntime<Upstream: UpstreamAgentActor> {
    server: Addr<GrpcServerActor>,
    clients: Vec<Addr<Upstream>>,
}

impl<ContextBuilder, UpstreamCfg, DownstreamCfg>
    AgentBuilder<ContextBuilder, UpstreamCfg, DownstreamCfg>
{
    /// Set the unique id that represents this agent instance
    pub fn agent_id(self, agent_id: u32) -> Self {
        Self { agent_id, ..self }
    }

    /// Parameters used by this agent to run the simulations
    pub fn simulation_params(self, simulation_params: SimulationParams) -> Self {
        Self {
            simulation_params,
            ..self
        }
    }

    /// Configure the interface exposed for other agent to connect
    pub fn downstream<DownstreamConfigType>(
        self,
        downstream: DownstreamConfigType,
    ) -> AgentBuilder<ContextBuilder, UpstreamCfg, DownstreamConfigType> {
        AgentBuilder {
            agent_id: self.agent_id,
            simulation_params: self.simulation_params,
            downstream,
            upstream: self.upstream,
            rune_context_builder: self.rune_context_builder,
        }
    }

    /// Configure the parameters this agent will use to connect to other agents (or controller)
    pub fn upstream(self, upstream: HashMap<String, UpstreamCfg>) -> Self {
        Self { upstream, ..self }
    }

    /// Configure the rune context used to execute the simulations
    pub fn rune_context_builder<ContextBuilderType>(
        self,
        rune_context_builder: ContextBuilderType,
    ) -> AgentBuilder<ContextBuilderType, UpstreamCfg, DownstreamCfg> {
        AgentBuilder {
            agent_id: self.agent_id,
            simulation_params: self.simulation_params,
            downstream: self.downstream,
            upstream: self.upstream,
            rune_context_builder,
        }
    }
}

impl<ContextBuilder, UpstreamCfg, DownstreamCfg>
    AgentBuilder<ContextBuilder, UpstreamCfg, DownstreamCfg>
where
    ContextBuilder: FnOnce(Addr<SimulationActor>) -> rune::Context,
{
    /// Build and start the agent
    pub fn launch<Upstream: UpstreamAgentActor<Config = UpstreamCfg>>(
        self,
    ) -> AgentRuntime<Upstream> {
        let metrics_addr = MetricsManagerActor::start_default();

        let simulation_ctx: Context<SimulationActor> = Context::new();

        let rune_context = (self.rune_context_builder)(simulation_ctx.address());
        let bot_registry = BotRegistry::new(rune_context, metrics_addr.clone())
            .expect("Error during registry construction");

        let updater_addr = UpdatesNotifierActor::create(|_| UpdatesNotifierActor::new());
        let server_actor =
            GrpcServerActor::create(|_| GrpcServerActor::new(updater_addr.clone().recipient()));
        let simulation_actor = simulation_ctx.run(SimulationActor::new(
            self.agent_id,
            self.simulation_params,
            bot_registry,
        ));
        let core_addr = AgentCoreActor::create(|_| {
            AgentCoreActor::new(
                self.agent_id,
                updater_addr.clone(),
                server_actor.clone(),
                simulation_actor,
                metrics_addr,
            )
        });

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

    fn initialize_clients<Upstream: UpstreamAgentActor>(
        cfg: HashMap<String, Upstream::Config>,
        core_addr: Addr<AgentCoreActor>,
    ) -> Result<Vec<Addr<Upstream>>, Upstream::InitializationError> {
        let clients = cfg
            .into_values()
            .map(|conf| Upstream::new(conf, core_addr.clone()))
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
    /// Build and start the agent using grpc as communication channel agent to agent and agent to controller
    pub async fn launch_grpc(self) {
        let address = self.downstream;
        let runtime = self.launch::<GrpcUpstreamAgentActor>();

        let hailstorm_server = HailstormGrpcServer::new(runtime.server.recipient());
        Server::builder()
            .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(
                hailstorm_server,
            ))
            .serve(address)
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod test {
    use crate::agent::actor::AgentCoreActor;
    use crate::agent::builder::AgentBuilder;
    use crate::communication::message::ControllerCommandMessage;
    use crate::communication::upstream::contract::UpstreamAgentActor;
    use crate::grpc::ControllerCommand;
    use crate::simulation::actor::simulation::SimulationParams;
    use actix::{Actor, Addr, Context};
    use std::net::{IpAddr, SocketAddr};
    use std::time::Duration;
    use tokio::time::error::Elapsed;

    #[actix::test]
    async fn launch_agent() {
        struct MockUpstream;
        impl Actor for MockUpstream {
            type Context = Context<Self>;
        }

        impl UpstreamAgentActor for MockUpstream {
            type Config = u32;
            type InitializationError = std::io::Error;

            fn new(
                cfg: Self::Config,
                core_addr: Addr<AgentCoreActor>,
            ) -> Result<Self, Self::InitializationError> {
                Ok(Self)
            }
        }

        let rt = AgentBuilder::default()
            .agent_id(5702_u32)
            .simulation_params(SimulationParams::default())
            .upstream([(String::from("core"), 0)].into_iter().collect())
            .rune_context_builder(|_sim| {
                rune::Context::with_default_modules().expect("error loading default rune modules")
            })
            .launch::<MockUpstream>();

        let out = rt
            .server
            .send(ControllerCommandMessage(ControllerCommand {
                commands: vec![],
                target: None,
            }))
            .await
            .expect("Error sending command to server");
    }

    #[actix::test]
    async fn launch_grpc_agent() {
        let serve_fut = AgentBuilder::default()
            .agent_id(5702_u32)
            .simulation_params(SimulationParams::default())
            .upstream(
                [(String::from("core"), String::from("127.0.0.1"))]
                    .into_iter()
                    .collect(),
            )
            .downstream(SocketAddr::new(IpAddr::from([127, 0, 0, 1]), 9999))
            .rune_context_builder(|_sim| {
                rune::Context::with_default_modules().expect("error loading default rune modules")
            })
            .launch_grpc();

        // TODO: find a batter way to test grpc server
        match tokio::time::timeout(Duration::from_secs(10), serve_fut).await {
            Ok(()) => log::warn!("grpc server completed in less tha 10s"),
            Err(_elapsed) => log::debug!("grpc server started"),
        }
    }
}
