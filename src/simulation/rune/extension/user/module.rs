use rune::{ContextError, Module};
use super::behaviour::UserBehaviour;

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["user"]);

    module.ty::<UserBehaviour>()?;
    module.inst_fn("register_action", UserBehaviour::register_action)?;
    module.inst_fn("set_interval_millis", UserBehaviour::set_interval_millis)?;

    Ok(module)
}