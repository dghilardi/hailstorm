use crate::agent::actor::{AgentCoreActor, RegisterAgentClientMsg};
use crate::communication::message::SendAgentMessage;
use crate::communication::protobuf::grpc::hailstorm_service_client::HailstormServiceClient;
use crate::communication::protobuf::grpc::{AgentMessage, ControllerCommand};
use crate::communication::upstream::contract::UpstreamAgentActor;
use actix::{
    Actor, ActorContext, ActorFutureExt, ActorTryFutureExt, Addr, AsyncContext, Context, Handler,
    Message, ResponseActFuture, ResponseFuture, StreamHandler, WrapFuture,
};
use futures::future::{ok, ready};
use futures::{StreamExt, TryFutureExt};
use rand::Rng;
use std::cmp::min;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::Streaming;

struct UpstreamConnection {
    /// Kept alive to maintain the gRPC channel; used implicitly by the stream.
    #[allow(dead_code)]
    client: HailstormServiceClient<Channel>,
    upd_sender: Sender<AgentMessage>,
}

pub struct GrpcUpstreamAgentActor {
    url: String,
    core_addr: Addr<AgentCoreActor>,
    connection: Option<UpstreamConnection>,
    cmd_sender: Option<Sender<ControllerCommand>>,
}

impl Actor for GrpcUpstreamAgentActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("UpstreamAgentActor started");
        let (cmd_sender, cmd_receiver) = mpsc::channel(128);
        self.cmd_sender = Some(cmd_sender);
        let registration_req = self.core_addr.send(RegisterAgentClientMsg {
            cmd_receiver,
            msg_sender: ctx.address().recipient(),
        });
        ctx.spawn(
            registration_req
                .into_actor(self)
                .map(|res, _act, _ctx| match res {
                    Ok(()) => Ok(()),
                    Err(err) => Err(GrpcConnectionError::Internal(err.to_string())),
                })
                .and_then(|_, act, ctx| {
                    ctx.address()
                        .send(EstablishConnection { attempt: 0 })
                        .into_actor(act)
                        .map(|res, _, _| {
                            res.unwrap_or_else(|err| {
                                Err(GrpcConnectionError::Internal(err.to_string()))
                            })
                        })
                })
                .map(|res, _act, ctx| match res {
                    Ok(()) => log::debug!("Connection established"),
                    Err(err) => {
                        log::warn!("Connection failed - {err}. Stopping actor");
                        ctx.stop();
                    }
                }),
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::debug!("UpstreamAgentActor stopped");
    }
}

impl UpstreamAgentActor for GrpcUpstreamAgentActor {
    type Config = String;
    type InitializationError = tonic::transport::Error;

    fn new(url: String, core_addr: Addr<AgentCoreActor>) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            url,
            core_addr,
            connection: None,
            cmd_sender: None,
        })
    }
}

#[derive(Debug, Error)]
enum GrpcConnectionError {
    #[error("Connection Error - {0}")]
    Connection(String),
    #[error("Channel Creation Error - {0}")]
    ChannelCreation(String),
    #[error("Internal Error - {0}")]
    Internal(String),
}

#[derive(Message)]
#[rtype(result = "Result<(), GrpcConnectionError>")]
struct EstablishConnection {
    attempt: u32,
}

impl Handler<EstablishConnection> for GrpcUpstreamAgentActor {
    type Result = ResponseActFuture<Self, Result<(), GrpcConnectionError>>;

    fn handle(&mut self, msg: EstablishConnection, _ctx: &mut Self::Context) -> Self::Result {
        let url = self.url.clone();
        let attempt = msg.attempt;

        let actor_future = HailstormServiceClient::connect(self.url.clone())
            .map_err(|err| GrpcConnectionError::Connection(err.to_string()))
            .into_actor(self)
            .and_then(|mut client: HailstormServiceClient<_>, act, _ctx| {
                async move {
                    let (tx, rx) = mpsc::channel(128);
                    let cmd_stream = client
                        .join(ReceiverStream::new(rx))
                        .await
                        .map_err(|err| GrpcConnectionError::ChannelCreation(err.to_string()))?;
                    Ok((client, tx, cmd_stream.into_inner()))
                }
                .into_actor(act)
            })
            .and_then(
                |(client, upd_sender, cmd_stream): (_, _, Streaming<ControllerCommand>),
                 act,
                 ctx| {
                    act.connection = Some(UpstreamConnection { client, upd_sender });
                    ctx.add_stream(cmd_stream.filter_map(|result| {
                        ready(match result {
                            Ok(cmd) => Some(cmd),
                            Err(err) => {
                                log::error!("Error processing command stream - {err}");
                                None
                            }
                        })
                    }));
                    ok(())
                },
            )
            .then(move |result, act, ctx| {
                let address = ctx.address();
                async move {
                    if let Err(err) = result {
                        log::error!(
                            "Error connecting to parent '{url}' (attempt {attempt} - {err}"
                        );
                        actix::clock::sleep(truncated_exponential_backoff(
                            msg.attempt,
                            Duration::from_secs(300),
                        ))
                        .await;
                        address
                            .send(EstablishConnection {
                                attempt: msg.attempt.saturating_add(1),
                            })
                            .await
                            .map_err(|err| GrpcConnectionError::Internal(err.to_string()))?
                    } else {
                        Ok(())
                    }
                }
                .into_actor(act)
            });

        Box::pin(actor_future)
    }
}

fn truncated_exponential_backoff(attempt_n: u32, max_backoff: Duration) -> Duration {
    let base = 2u64.saturating_pow(attempt_n.min(30));
    let jitter = Duration::from_millis(rand::rng().random_range(0..1000));
    min(
        Duration::from_secs(base).saturating_add(jitter),
        max_backoff,
    )
}

impl Handler<SendAgentMessage> for GrpcUpstreamAgentActor {
    type Result = ResponseFuture<()>;

    fn handle(
        &mut self,
        SendAgentMessage(msg): SendAgentMessage,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let maybe_sender = self.connection.as_ref().map(|conn| conn.upd_sender.clone());
        Box::pin(async move {
            if let Some(sender) = maybe_sender {
                let out = sender.send(msg).await;
                if let Err(err) = out {
                    log::error!("Error sending message {}", err);
                }
            } else {
                log::warn!("Upstream channel not yet initialized");
            }
        })
    }
}

impl StreamHandler<ControllerCommand> for GrpcUpstreamAgentActor {
    fn handle(&mut self, item: ControllerCommand, ctx: &mut Self::Context) {
        if let Some(sender) = self.cmd_sender.clone() {
            // Pause stream polling while the core catches up instead of dropping commands.
            ctx.wait(
                async move {
                    if let Err(err) = sender.send(item).await {
                        log::error!("Error sending command to actor - {err}");
                    }
                }
                .into_actor(self),
            );
        } else {
            log::warn!("Connection is not initialized");
        }
    }

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("Command stream for '{}' started", self.url);
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        log::debug!("Command stream for '{}' finished", self.url);
        self.connection = None;
        if ctx.state().alive() {
            let reconnection_req = ctx.address().send(EstablishConnection { attempt: 0 });
            ctx.spawn(
                reconnection_req
                    .into_actor(self)
                    .map(|result, act, ctx| match result {
                        Ok(Ok(())) => {
                            log::debug!("Reconnection for {} completed", act.url);
                        }
                        Ok(Err(err)) => {
                            log::error!("Reconnection failed - {err}");
                            ctx.stop();
                        }
                        Err(err) => {
                            log::error!("Error sending reconnection request - {err}");
                            ctx.stop();
                        }
                    }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_capped_without_overflow() {
        let cap = Duration::from_secs(300);
        for attempt in [9, 31, 32, 63, 64, u32::MAX] {
            assert_eq!(truncated_exponential_backoff(attempt, cap), cap);
        }
    }
}
