use super::behaviour::BotBehaviour;
use crate::simulation::bot::params::BotParams;
use crate::simulation::rune::extension::bot::behaviour::ActionTrigger;
use crate::simulation::rune::extension::bot::state::BotState;
use rune::{ContextError, Module};

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["bot"]);

    module.ty::<BotParams>()?;
    module.ty::<BotBehaviour>()?;
    module.inst_fn("register_action", BotBehaviour::register_action)?;
    module.inst_fn("set_interval_millis", BotBehaviour::set_interval_millis)?;

    module.ty::<ActionTrigger>()?;
    module.function(&["ActionTrigger", "alive"], |weight| ActionTrigger::Alive {
        weight,
    })?;
    module.function(&["ActionTrigger", "enter_state"], |state: BotState| {
        ActionTrigger::EnterState {
            state: state.into(),
        }
    })?;

    module.ty::<BotState>()?;

    Ok(module)
}
