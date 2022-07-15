use std::sync::Arc;
use rune::runtime::RuntimeContext;
use rune::Unit;
use crate::simulation::rune::extension::bot::BotBehaviour;
use crate::simulation::bot::params::BotParams;
use crate::simulation::bot::scripted::ScriptedBot;

pub struct BotModelFactory {
    pub model: String,
    pub behaviour: BotBehaviour,
    pub runtime: Arc<RuntimeContext>,
    pub unit: Arc<Unit>,
}

impl BotModelFactory {
    pub fn new_bot(&self, bot_id: u64) -> ScriptedBot {
        let mut vm = rune::Vm::new(self.runtime.clone(), self.unit.clone());
        let params = BotParams { bot_id };
        let instance = vm.call([&self.model, "new"], (params,)).expect("Error construction");
        ScriptedBot::new(
            self.behaviour.clone(),
            instance,
            vm,
        )
    }
}