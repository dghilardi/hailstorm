//! Run a controller in the same process as an agent, without a gRPC connection.
//!
//! Launch an agent with [`EmbeddedUpstream`] as upstream, then send
//! [`StartEmbeddedController`] to obtain the address of a [`ControllerActor`] that
//! controls that agent with the same messages used by a remote controller.

use crate::agent::actor::{AgentCoreActor, RegisterAgentClientMsg};
use crate::communication::message::{
    ControllerCommandMessage, MultiAgentUpdateMessage, SendAgentMessage,
};
use crate::communication::protobuf::grpc::ControllerCommand;
use crate::communication::upstream::contract::UpstreamAgentActor;
use crate::controller::actor::ControllerActor;
use crate::controller::client::downstream::DownstreamClient;
use actix::{
    Actor, ActorFutureExt, Addr, AsyncContext, Context, Handler, MessageResult, WrapFuture,
};
use tokio::sync::mpsc;

/// Upstream connecting an agent to a controller running in the same process.
pub struct EmbeddedUpstream {
    core_addr: Addr<AgentCoreActor>,
    cmd_sender: Option<mpsc::Sender<ControllerCommand>>,
    controller: Option<Addr<ControllerActor>>,
}

impl Actor for EmbeddedUpstream {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let (cmd_sender, cmd_receiver) = mpsc::channel(128);
        self.cmd_sender = Some(cmd_sender);
        ctx.wait(
            self.core_addr
                .send(RegisterAgentClientMsg {
                    cmd_receiver,
                    msg_sender: ctx.address().recipient(),
                })
                .into_actor(self)
                .map(|res, _, _| {
                    if let Err(err) = res {
                        log::error!("Error registering embedded upstream - {err}");
                    }
                }),
        );
    }
}

impl UpstreamAgentActor for EmbeddedUpstream {
    type Config = ();
    type InitializationError = std::convert::Infallible;

    fn new(_cfg: (), core_addr: Addr<AgentCoreActor>) -> Result<Self, Self::InitializationError> {
        Ok(Self {
            core_addr,
            cmd_sender: None,
            controller: None,
        })
    }
}

/// Start the embedded controller, if not started yet, and return its address.
#[derive(actix::Message)]
#[rtype(result = "Addr<ControllerActor>")]
pub struct StartEmbeddedController;

impl Handler<StartEmbeddedController> for EmbeddedUpstream {
    type Result = MessageResult<StartEmbeddedController>;

    fn handle(&mut self, _msg: StartEmbeddedController, ctx: &mut Self::Context) -> Self::Result {
        let controller = self.controller.get_or_insert_with(|| {
            ControllerActor::new(
                DownstreamClient::new(ctx.address().recipient()),
                DiscardMetrics.start().recipient(),
            )
            .start()
        });
        MessageResult(controller.clone())
    }
}

impl Handler<ControllerCommandMessage> for EmbeddedUpstream {
    type Result = ();

    fn handle(
        &mut self,
        ControllerCommandMessage(cmd): ControllerCommandMessage,
        ctx: &mut Self::Context,
    ) {
        if let Some(sender) = self.cmd_sender.clone() {
            // Apply backpressure instead of dropping commands while the core catches up.
            ctx.wait(
                async move {
                    if let Err(err) = sender.send(cmd).await {
                        log::error!("Error sending command to the embedded agent - {err}");
                    }
                }
                .into_actor(self),
            );
        }
    }
}

impl Handler<SendAgentMessage> for EmbeddedUpstream {
    type Result = ();

    fn handle(&mut self, SendAgentMessage(msg): SendAgentMessage, _ctx: &mut Self::Context) {
        if let Some(controller) = &self.controller {
            controller.do_send(MultiAgentUpdateMessage(msg.updates));
        }
    }
}

/// The embedded controller has no metrics exporter.
struct DiscardMetrics;

impl Actor for DiscardMetrics {
    type Context = Context<Self>;
}

impl Handler<MultiAgentUpdateMessage> for DiscardMetrics {
    type Result = ();

    fn handle(&mut self, _msg: MultiAgentUpdateMessage, _ctx: &mut Self::Context) {}
}
