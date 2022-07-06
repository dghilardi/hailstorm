use std::collections::HashMap;
use std::time::Duration;
use actix::{Actor, AsyncContext, Context, Handler, Recipient};
use crate::communication::protobuf::grpc::{AgentMessage, AgentUpdate};
use crate::communication::message::{MultiAgentUpdateMessage, SendAgentMessage};

#[derive(Default)]
pub struct UpdatesNotifierActor {
    frames: HashMap<u64, AgentUpdate>,
    connected_clients: Vec<Recipient<SendAgentMessage>>,
}

impl Actor for UpdatesNotifierActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_secs(5), |actor, _ctx| actor.send_data());
    }
}

impl UpdatesNotifierActor {
    pub fn new() -> Self {
        Default::default()
    }

    fn send_data(&mut self) {
        let message = AgentMessage {
            updates: self.frames
                .drain()
                .map(|(_idx, frame)| frame)
                .collect()
        };

        for client in self.connected_clients.iter() {
            client.try_send(SendAgentMessage(message.clone()))
                .unwrap_or_else(|err| log::error!("Error sending frames {err:?}"));
        }
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct RegisterAgentUpdateSender(pub Recipient<SendAgentMessage>);

impl Handler<RegisterAgentUpdateSender> for UpdatesNotifierActor {
    type Result = ();

    fn handle(&mut self, RegisterAgentUpdateSender(msg): RegisterAgentUpdateSender, _ctx: &mut Self::Context) -> Self::Result {
        self.connected_clients.push(msg);
    }
}

impl Handler<MultiAgentUpdateMessage> for UpdatesNotifierActor {
    type Result = ();

    fn handle(&mut self, MultiAgentUpdateMessage(updates): MultiAgentUpdateMessage, _ctx: &mut Self::Context) -> Self::Result {
        for update in updates {
            self.frames.insert(update.update_id, update);
        }
    }
}