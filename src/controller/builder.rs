use std::net::SocketAddr;
use actix::{Addr, AsyncContext, Context, Recipient};
use tonic::transport::Server;
use tonic::transport::server::Router;
use crate::communication::grpc;
use crate::communication::notifier_actor::AgentUpdateMessage;
use crate::communication::server::HailstormGrpcServer;
use crate::communication::server_actor::GrpcServerActor;
use crate::controller::controller_actor::ControllerActor;

pub struct ControllerBuilder {
    pub metrics_storage: Recipient<AgentUpdateMessage>,
}

impl ControllerBuilder {
    pub async fn build(self) -> ControllerApp {
        let controller_ctx: Context<ControllerActor> = Context::new();
        let grpc_server_ctx: Context<GrpcServerActor> = Context::new();

        let controller_actor = ControllerActor::new(grpc_server_ctx.address().recipient(), self.metrics_storage);
        let grpc_server_actor = GrpcServerActor::new(controller_ctx.address().recipient());

        let server_addr = grpc_server_ctx.run(grpc_server_actor);
        let controller_addr = controller_ctx.run(controller_actor);

        let hailstorm_server = HailstormGrpcServer::new(server_addr.recipient());
        let router = Server::builder()
            .add_service(grpc::hailstorm_service_server::HailstormServiceServer::new(hailstorm_server));

        ControllerApp {
            controller_addr,
            grpc_router: router,
        }
    }
}

pub struct ControllerApp {
    controller_addr: Addr<ControllerActor>,
    grpc_router: Router,
}

impl ControllerApp {
    pub fn controller_addr(&self) -> Addr<ControllerActor> {
        self.controller_addr.clone()
    }

    pub async fn launch(self, address: SocketAddr) {
        self.grpc_router
            .serve(address)
            .await
            .expect("Error running grpc server")
    }
}