use crate::simulation::bot::params::BotParams;
use crate::simulation::bot::scripted::ScriptedBot;
use crate::simulation::compound_id::CompoundId;
use crate::simulation::rune::extension::bot::BotBehaviour;
use rune::runtime::RuntimeContext;
use rune::Unit;
use std::sync::Arc;

pub struct BotModelFactory {
    pub model: String,
    pub behaviour: BotBehaviour,
    pub runtime: Arc<RuntimeContext>,
    pub unit: Arc<Unit>,
}

impl BotModelFactory {
    pub fn new_bot(&self, compound_id: CompoundId<u32>) -> ScriptedBot {
        let mut vm = rune::Vm::new(self.runtime.clone(), self.unit.clone());
        let params = BotParams {
            bot_id: compound_id.bot_id(),
            internal_id: compound_id.internal_id(),
            global_id: compound_id.global_id(),
        };
        let instance = vm
            .call([&self.model, "new"], (params,))
            .expect("Error construction");
        ScriptedBot::new(self.behaviour.clone(), instance, vm)
    }
}
