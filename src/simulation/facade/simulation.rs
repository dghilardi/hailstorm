use crate::simulation::actor::bot::{BotState, ExecuteHandler};
use crate::simulation::actor::simulation::{BotStateChange, InvokeHandler};
use actix::dev::RecipientRequest;
use actix::{Actor, Addr, Context, Handler, Recipient};

/// Facade for interacting with the simulation actor from outside the actor system.
///
/// Provides a convenient interface to change bot states and invoke handlers
/// without needing direct access to the simulation actor's address.
#[derive(Clone)]
pub struct SimulationFacade {
    user_state_change_tx: Recipient<BotStateChange>,
    user_handler_tx: Recipient<InvokeHandler>,
}

impl SimulationFacade {
    pub fn new<S>(actor: Addr<S>) -> Self
    where
        S: Actor<Context = Context<S>> + Handler<BotStateChange> + Handler<InvokeHandler>,
    {
        Self {
            user_state_change_tx: actor.clone().recipient(),
            user_handler_tx: actor.recipient(),
        }
    }

    pub fn change_bot_state(
        &self,
        bot_id: u64,
        state: BotState,
    ) -> RecipientRequest<BotStateChange> {
        self.user_state_change_tx
            .send(BotStateChange { bot_id, state })
    }

    pub fn invoke_handler(
        &self,
        bot_id: u64,
        execution_args: ExecuteHandler,
    ) -> RecipientRequest<InvokeHandler> {
        self.user_handler_tx.send(InvokeHandler {
            bot_id,
            execution_args,
        })
    }
}
