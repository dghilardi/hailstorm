use std::cmp::min;
use rand::{Rng, thread_rng};
use rune::{Any, ContextError, Hash, Module, Value};
use rune::runtime::{AccessError, Function, Shared};

#[derive(Debug, Any)]
pub struct HailstormUser {
    actions: Vec<UserAction>
}

#[derive(Debug)]
struct UserAction {
    weight: f64,
    hash: Hash,
}

impl HailstormUser {
    pub fn new() -> Self {
        Self { actions: vec![] }
    }

    fn register_action(&mut self, weight: f64, action: Shared<Function>) {
        let weight = weight.max(0f64);
        let hash = action.take().expect("Error extracting action hash").type_hash();
        self.actions.push(UserAction { hash, weight });
    }

    pub fn random_action(&self) -> Hash {
        self.actions[thread_rng().gen::<usize>() % self.actions.len()].hash
    }
}

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["user"]);

    module.ty::<HailstormUser>()?;
    module.inst_fn("register_action", HailstormUser::register_action)?;

    Ok(module)
}