use std::sync::Arc;
use rune::runtime::RuntimeContext;
use rune::Unit;
use crate::simulation::rune::extension::user::UserBehaviour;
use crate::simulation::user::params::UserParams;
use crate::simulation::user::registry::User;

pub struct UserModelFactory {
    model: String,
    behaviour: UserBehaviour,
    runtime: Arc<RuntimeContext>,
    unit: Arc<Unit>,
}

impl UserModelFactory {
    pub fn new_user(&self, user_id: u32) -> User {
        let mut vm = rune::Vm::new(self.runtime.clone(), self.unit.clone());
        let params = UserParams { user_id };
        let instance = vm.call([&self.model, "new"], (params,)).expect("Error construction");
        User::new(
            self.behaviour.clone(),
            instance,
            vm,
        )
    }
}