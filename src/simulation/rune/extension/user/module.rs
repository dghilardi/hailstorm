use rune::{ContextError, Module};
use crate::simulation::rune::extension::user::behaviour::ActionTrigger;
use crate::simulation::rune::extension::user::user_state::UserState;
use crate::simulation::user::params::UserParams;
use super::behaviour::UserBehaviour;

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["user"]);

    module.ty::<UserParams>()?;
    module.ty::<UserBehaviour>()?;
    module.inst_fn("register_action", UserBehaviour::register_action)?;
    module.inst_fn("set_interval_millis", UserBehaviour::set_interval_millis)?;

    module.ty::<ActionTrigger>()?;
    module.function(&["ActionTrigger", "alive"], |weight| ActionTrigger::Alive { weight })?;
    module.function(&["ActionTrigger", "enter_state"], |state: UserState| ActionTrigger::EnterState { state: state.into() })?;

    module.ty::<UserState>()?;

    Ok(module)
}