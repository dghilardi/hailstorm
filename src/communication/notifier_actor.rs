use std::collections::HashMap;
use std::time::Duration;
use actix::{Actor, AsyncContext, Context, Handler};
use tokio::sync::mpsc::Sender;
use crate::communication::grpc::{AgentMessage, AgentUpdate};

#[derive(Default)]
pub struct UpdatesNotifierActor {
    frames: HashMap<u64, AgentUpdate>,
    connected_clients: Vec<Sender<AgentMessage>>,
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
            client.try_send(message.clone())
                .unwrap_or_else(|err| log::error!("Error sending frames {err:?}"));
        }
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct RegisterAgentUpdateSender(pub Sender<AgentMessage>);

impl Handler<RegisterAgentUpdateSender> for UpdatesNotifierActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterAgentUpdateSender, _ctx: &mut Self::Context) -> Self::Result {
        self.connected_clients.push(msg.0);
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct AgentUpdateMessage(pub AgentUpdate);

impl Handler<AgentUpdateMessage> for UpdatesNotifierActor {
    type Result = ();

    fn handle(&mut self, AgentUpdateMessage(msg): AgentUpdateMessage, _ctx: &mut Self::Context) -> Self::Result {
        self.frames.insert(msg.update_id, msg);
    }
}