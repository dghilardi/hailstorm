use super::behaviour::BotBehaviour;
use crate::simulation::bot::params::BotParams;
use crate::simulation::rune::extension::bot::behaviour::ActionTrigger;
use crate::simulation::rune::extension::bot::state::BotState;
use rune::{ContextError, Module};

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", ["bot"])?;

    module.ty::<BotParams>()?;
    module.ty::<BotBehaviour>()?;
    module.associated_function("register_action", BotBehaviour::register_action)?;
    module.associated_function("set_interval_millis", BotBehaviour::set_interval_millis)?;

    module.ty::<ActionTrigger>()?;
    module.function("alive", |weight: f32| ActionTrigger::Alive { weight }).build()?;
    module.function("enter_state", |state: BotState| {
        ActionTrigger::EnterState {
            state: state.into(),
        }
    }).build()?;

    module.ty::<BotState>()?;

    Ok(module)
}
