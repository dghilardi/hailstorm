use crate::communication::protobuf::grpc;
use crate::communication::server::HailstormGrpcServer;
use crate::communication::server_actor::GrpcServerActor;
use crate::controller::actor::ControllerActor;
use crate::controller::client::downstream::DownstreamClient;
use crate::MultiAgentUpdateMessage;
use actix::{Actor, Addr, AsyncContext, Context, Handler};
use std::net::SocketAddr;
use tonic::transport::server::Router;
use tonic::transport::Server;

/// Struct used to build a controller instance
pub struct ControllerBuilder<MetricsStorage> {
    metrics_storage: MetricsStorage,
}

impl Default for ControllerBuilder<()> {
    fn default() -> Self {
        Self {
            metrics_storage: (),
        }
    }
}

impl<MetricsStorage> ControllerBuilder<MetricsStorage> {
    /// Set metrics storage actor address
    pub fn metrics_storage<MetricsStorageAct>(
        self,
        metrics_storage_addr: Addr<MetricsStorageAct>,
    ) -> ControllerBuilder<Addr<MetricsStorageAct>>
    where
        MetricsStorageAct:
            Actor<Context = Context<MetricsStorageAct>> + Handler<MultiAgentUpdateMessage>,
    {
        ControllerBuilder {
            metrics_storage: metrics_storage_addr,
        }
    }
}

impl<MetricsStorageAct> ControllerBuilder<Addr<MetricsStorageAct>>
where
    MetricsStorageAct:
        Actor<Context = Context<MetricsStorageAct>> + Handler<MultiAgentUpdateMessage>,
{
    /// Build controller app
    pub async fn build(self) -> ControllerApp {
        let controller_ctx: Context<ControllerActor> = Context::new();
        let grpc_server_ctx: Context<GrpcServerActor> = Context::new();

        let controller_actor = ControllerActor::new(
            DownstreamClient::new(grpc_server_ctx.address().recipient()),
            self.metrics_storage.recipient(),
        );
        let grpc_server_actor = GrpcServerActor::new(controller_ctx.address().recipient());

        let server_addr = grpc_server_ctx.run(grpc_server_actor);
        let controller_addr = controller_ctx.run(controller_actor);

        let hailstorm_server = HailstormGrpcServer::new(server_addr.recipient());
        let router = Server::builder().add_service(
            grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server),
        );

        ControllerApp {
            controller_addr,
            grpc_router: router,
        }
    }
}

/// Controller application state
pub struct ControllerApp {
    controller_addr: Addr<ControllerActor>,
    grpc_router: Router,
}

impl ControllerApp {
    /// Get address to communicate with controller actor
    pub fn controller_addr(&self) -> Addr<ControllerActor> {
        self.controller_addr.clone()
    }

    /// Launch the controller and expose the grpc API
    pub async fn launch(self, address: SocketAddr) {
        self.grpc_router
            .serve(address)
            .await
            .expect("Error running grpc server")
    }
}
