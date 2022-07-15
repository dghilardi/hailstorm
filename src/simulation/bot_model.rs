use std::collections::HashMap;
use actix::{Actor, Addr, Context, Handler};
use actix::dev::Request;
use rune::Hash;
use crate::simulation::compound_id::CompoundId;
use crate::simulation::rune::types::value::OwnedValue;
use crate::simulation::sequential_id_generator::SequentialIdGenerator;
use crate::simulation::actor::simulation::BotStateChange;
use crate::simulation::bot::model_factory::BotModelFactory;
use crate::simulation::actor::bot::{ExecuteHandler, StopBot, TriggerHook, BotActor, BotState};
use crate::utils::varint::VarintDecode;
pub struct SimulationBot {
    pub state: BotState,
    addr: Addr<BotActor>,
}

impl SimulationBot {
    pub fn stop_bot(&mut self) {
        let send_outcome = self.addr.try_send(StopBot);
        if let Err(err) = send_outcome {
            log::error!("Error stopping bot - {}", err);
        } else {
            self.state = BotState::Stopping;
        }
    }

    pub fn trigger_hook(&mut self, state: BotState) -> Request<BotActor, TriggerHook> {
        self.addr.send(TriggerHook { state })
    }

    pub fn execute_handler(&self, id: Hash, args: OwnedValue) -> Request<BotActor, ExecuteHandler> {
        self.addr.send(ExecuteHandler { id, args })
    }

    pub fn state(&self) -> BotState {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.addr.connected()
    }
}

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
            bots: Default::default()
        }
    }

    pub fn spawn_bot<A>(&mut self, addr: Addr<A>)
        where A: Actor<Context=Context<A>>
        + Handler<BotStateChange>
    {
        let usr_id = self.id_generator.next();
        let compound_id = CompoundId::new(self.agent_id, self.model_id, usr_id);
        let internal_id = compound_id.internal_id();
        let bot_behaviour = self.bot_factory.new_bot(compound_id);

        self.bots.insert(internal_id, SimulationBot {
            state: BotState::Running,
            addr: BotActor::create(|_| BotActor::new(internal_id, addr, bot_behaviour)),
        });
    }

    pub fn count_by_state(&self) -> HashMap<BotState, usize> {
        let mut group_by_state = HashMap::new();

        for usr in self.bots.values() {
            let entry = group_by_state.entry(usr.state)
                .or_insert(0);
            *entry += 1;
        }

        group_by_state
    }

    pub fn count_active(&self) -> usize {
        self.bots
            .iter()
            .filter(|(_id, bot)| bot.state != BotState::Stopping)
            .count()
    }

    pub fn retain<F>(&mut self, mut condition: F)
    where F: FnMut(&u64, &SimulationBot) -> bool
    {
        self.bots.retain(|id, bot| {
            let outcome = condition(id, bot);
            if !outcome {
                let compound_id = CompoundId::from_internal_id((), *id)
                    .unwrap_or_else(|_| panic!("internal id {id:08x} is in unexpected format"));
                self.id_generator.release_id(compound_id.bot_id());
            }
            outcome
        })
    }

    pub fn bots_mut(&mut self) -> impl Iterator<Item=&mut SimulationBot> {
        self.bots.values_mut()
    }

    pub fn contains_id(&self, id: u64) -> bool {
        let sub_ids = Vec::<u32>::from_varint(&id.to_be_bytes()).expect("Error converting from varint");
        sub_ids[0] == self.model_id && self.bots.contains_key(&id)
    }

    pub fn remove_bot(&mut self, id: u64) {
        self.bots.remove(&id);
        let sub_ids = Vec::<u32>::from_varint(&id.to_be_bytes()).expect("Error converting from varint");
        self.id_generator.release_id(sub_ids[1]);
    }

    pub fn get_bot_mut(&mut self, id: u64) -> Option<&mut SimulationBot> {
        self.bots.get_mut(&id)
    }
}