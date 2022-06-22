use std::sync::Arc;
use rune::runtime::RuntimeContext;
use rune::Unit;
use crate::simulation::rune::extension::user::UserBehaviour;
use crate::simulation::user::params::UserParams;
use crate::simulation::user::registry::User;

pub struct UserModelFactory {
    pub model: String,
    pub behaviour: UserBehaviour,
    pub runtime: Arc<RuntimeContext>,
    pub unit: Arc<Unit>,
}

impl UserModelFactory {
    pub fn new_user(&self, user_id: u64) -> User {
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