use crate::simulation::actor::bot::{BotActor, BotState, ExecuteHandler, StopBot, TriggerHook};
use crate::simulation::actor::simulation::BotStateChange;
use crate::simulation::bot::model_factory::BotModelFactory;
use crate::simulation::compound_id::CompoundId;
use crate::simulation::rune::types::value::OwnedValue;
use crate::simulation::sequential_id_generator::SequentialIdGenerator;
use crate::utils::varint::VarintDecode;
use actix::dev::Request;
use actix::{Actor, Addr, Context, Handler};
use rune::Hash;
use std::collections::HashMap;

/// Represents a single bot instance in the simulation.
pub struct SimulationBot {
    state: BotState,
    addr: Addr<BotActor>,
}

impl SimulationBot {
    /// Stops the bot.
    pub fn stop_bot(&mut self) {
        let send_outcome = self.addr.try_send(StopBot);
        if let Err(err) = send_outcome {
            log::error!("Error stopping bot - {}", err);
        } else {
            self.state = BotState::Stopping;
        }
    }

    /// Executes a handler on the bot.
    pub fn execute_handler(&self, id: Hash, args: OwnedValue) -> Request<BotActor, ExecuteHandler> {
        self.addr.send(ExecuteHandler { id, args })
    }

    /// Returns the current state of the bot.
    pub fn state(&self) -> BotState {
        self.state
    }

    /// Changes the state of the bot.
    pub fn change_state(&mut self, state: BotState) -> Request<BotActor, TriggerHook> {
        self.state = state;
        self.addr.send(TriggerHook { state })
    }

    /// Checks if the bot is connected (i.e., the actor is running).
    pub fn is_connected(&self) -> bool {
        self.addr.connected()
    }
}

/// Manages a collection of bots for a specific model.
pub struct BotModel {
    agent_id: u32,
    model_id: u32,
    id_generator: SequentialIdGenerator,
    bot_factory: BotModelFactory,
    bots: HashMap<u64, SimulationBot>,
}

impl BotModel {
    pub fn new(agent_id: u32, model_id: u32, factory: BotModelFactory) -> Self {
        Self {
            agent_id,
            model_id,
            bot_factory: factory,
            id_generator: Default::default(),
            bots: Default::default(),
        }
    }

    /// Spawns a new bot.
    pub fn spawn_bot<A>(&mut self, addr: Addr<A>)
    where
        A: Actor<Context = Context<A>> + Handler<BotStateChange>,
    {
        let usr_id = self.id_generator.next();
        let compound_id = CompoundId::new(self.agent_id, self.model_id, usr_id);
        let internal_id = compound_id.internal_id();
        let bot_behaviour = self.bot_factory.new_bot(compound_id);

        self.bots.insert(
            internal_id,
            SimulationBot {
                state: BotState::Running,
                addr: BotActor::create(|_| BotActor::new(internal_id, addr, bot_behaviour)),
            },
        );
    }

    /// Counts the bots by their current state.
    pub fn count_by_state(&self) -> HashMap<BotState, usize> {
        let mut group_by_state = HashMap::new();

        for usr in self.bots.values() {
            let entry = group_by_state.entry(usr.state).or_insert(0);
            *entry += 1;
        }

        group_by_state
    }

    /// Counts the number of active bots (not stopping).
    pub fn count_active(&self) -> usize {
        self.bots
            .iter()
            .filter(|(_id, bot)| bot.state != BotState::Stopping)
            .count()
    }

    /// Retains only the bots that satisfy the condition.
    pub fn retain<F>(&mut self, mut condition: F)
    where
        F: FnMut(&u64, &SimulationBot) -> bool,
    {
        self.bots.retain(|id, bot| {
            let outcome = condition(id, bot);
            if !outcome {
                match CompoundId::from_internal_id((), *id) {
                     Ok(compound_id) => self.id_generator.release_id(compound_id.bot_id()),
                     Err(err) => {
                         log::error!("internal id {id:08x} is in unexpected format: {err}");
                         // We can't release the ID if we can't parse it, but we still remove the bot.
                     }
                }
            }
            outcome
        })
    }

    pub fn bots_mut(&mut self) -> impl Iterator<Item = &mut SimulationBot> {
        self.bots.values_mut()
    }

    /// Checks if the bot with the given ID exists and belongs to this model.
    pub fn contains_id(&self, id: u64) -> bool {
        if let Ok(sub_ids) = Vec::<u32>::from_varint(&id.to_be_bytes()) {
             if sub_ids.len() > 0 {
                return sub_ids[0] == self.model_id && self.bots.contains_key(&id);
             }
        }
        false
    }

    /// Removes a bot by ID.
    pub fn remove_bot(&mut self, id: u64) {
        if self.bots.remove(&id).is_some() {
            match Vec::<u32>::from_varint(&id.to_be_bytes()) {
                Ok(sub_ids) if sub_ids.len() > 1 => {
                    self.id_generator.release_id(sub_ids[1]);
                },
                _ => {
                    log::error!("Error converting from varint for id {id}");
                }
            }
        }
    }

    pub fn get_bot_mut(&mut self, id: u64) -> Option<&mut SimulationBot> {
        self.bots.get_mut(&id)
    }
}
