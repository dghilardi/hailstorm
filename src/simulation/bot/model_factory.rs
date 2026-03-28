use crate::simulation::bot::params::BotParams;
use crate::simulation::bot::scripted::ScriptedBot;
use crate::simulation::compound_id::CompoundId;
use crate::simulation::rune::extension::bot::BotBehaviour;
use rune::runtime::RuntimeContext;
use rune::Unit;
use std::sync::Arc;

/// Factory for creating bot instances of a specific model.
///
/// Holds the shared Rune runtime and compiled unit so that each new bot
/// gets its own VM instance but shares the compiled bytecode.
pub struct BotModelFactory {
    pub model: String,
    pub behaviour: BotBehaviour,
    pub runtime: Arc<RuntimeContext>,
    pub unit: Arc<Unit>,
}

impl BotModelFactory {
    /// Create a new bot instance for the given compound ID.
    ///
    /// Returns `None` if the Rune `new()` constructor fails (e.g., script error).
    pub fn new_bot(&self, compound_id: CompoundId<u32>) -> Option<ScriptedBot> {
        let mut vm = rune::Vm::new(self.runtime.clone(), self.unit.clone());
        let params = BotParams {
            bot_id: compound_id.bot_id(),
            internal_id: compound_id.internal_id(),
            global_id: compound_id.global_id(),
        };
        match vm.call([&self.model, "new"], (params,)) {
            Ok(instance) => Some(ScriptedBot::new(self.behaviour.clone(), instance, vm)),
            Err(err) => {
                log::error!("Error constructing bot '{}': {err}", self.model);
                None
            }
        }
    }
}
