use crate::simulation::actor::bot::BotExecutionLimits;
use crate::simulation::actor::bot::{BotActor, BotState, ExecuteHandler, TriggerHook};
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
pub struct SimulationBot {
    state: BotState,
    addr: Addr<BotActor>,
    stop_requested: tokio::sync::watch::Sender<bool>,
}

impl SimulationBot {
    pub fn stop_bot(&mut self) {
        self.stop_requested.send_replace(true);
        self.state = BotState::Stopping;
    }

    pub fn execute_handler(&self, id: Hash, args: OwnedValue) -> Request<BotActor, ExecuteHandler> {
        self.addr.send(ExecuteHandler { id, args })
    }

    pub fn state(&self) -> BotState {
        self.state
    }

    pub fn change_state(&mut self, state: BotState) -> Request<BotActor, TriggerHook> {
        self.state = state;
        self.addr.send(TriggerHook { state })
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
    execution_limits: BotExecutionLimits,
}

impl BotModel {
    pub fn new(agent_id: u32, model_id: u32, factory: BotModelFactory) -> Self {
        Self {
            agent_id,
            model_id,
            bot_factory: factory,
            id_generator: Default::default(),
            bots: Default::default(),
            execution_limits: BotExecutionLimits::default(),
        }
    }

    pub(crate) fn with_execution_limits(mut self, limits: BotExecutionLimits) -> Self {
        self.execution_limits = limits;
        self
    }

    pub fn spawn_bot<A>(&mut self, addr: Addr<A>)
    where
        A: Actor<Context = Context<A>> + Handler<BotStateChange>,
    {
        let usr_id = self.id_generator.next();
        let compound_id = CompoundId::new(self.agent_id, self.model_id, usr_id);
        let internal_id = compound_id.internal_id();

        match self.bot_factory.new_bot(compound_id) {
            Some(bot_behaviour) => {
                let (stop_requested, stop_receiver) = tokio::sync::watch::channel(false);
                let limits = self.execution_limits;
                self.bots.insert(
                    internal_id,
                    SimulationBot {
                        state: BotState::Running,
                        addr: BotActor::create(|_| {
                            BotActor::new(internal_id, addr, bot_behaviour)
                                .with_control(limits, stop_receiver)
                        }),
                        stop_requested,
                    },
                );
            }
            None => {
                self.id_generator.release_id(usr_id);
                log::error!("Failed to create bot, releasing ID {usr_id}");
            }
        }
    }

    pub fn count_by_state(&self) -> HashMap<BotState, usize> {
        let mut group_by_state = HashMap::new();

        for usr in self.bots.values() {
            let entry = group_by_state.entry(usr.state).or_insert(0);
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

    pub(crate) fn has_bots(&self) -> bool {
        !self.bots.is_empty()
    }

    pub fn retain<F>(&mut self, mut condition: F)
    where
        F: FnMut(&u64, &SimulationBot) -> bool,
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

    pub fn bots_mut(&mut self) -> impl Iterator<Item = &mut SimulationBot> {
        self.bots.values_mut()
    }

    pub fn contains_id(&self, id: u64) -> bool {
        let sub_ids =
            Vec::<u32>::from_varint(&id.to_be_bytes()).expect("Error converting from varint");
        sub_ids[0] == self.model_id && self.bots.contains_key(&id)
    }

    pub fn remove_bot(&mut self, id: u64) {
        self.bots.remove(&id);
        let sub_ids =
            Vec::<u32>::from_varint(&id.to_be_bytes()).expect("Error converting from varint");
        self.id_generator.release_id(sub_ids[1]);
    }

    pub fn get_bot_mut(&mut self, id: u64) -> Option<&mut SimulationBot> {
        self.bots.get_mut(&id)
    }
}
