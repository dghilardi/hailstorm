use std::time::Duration;
use crate::simulation::rune::extension::user::UserBehaviour;
use crate::simulation::user_actor::UserState;

pub struct ScriptedUser {
    behaviour: UserBehaviour,
    instance: rune::Value,
    vm: rune::Vm,
}

impl ScriptedUser {
    pub(crate) fn new(
        behaviour: UserBehaviour,
        instance: rune::Value,
        vm: rune::Vm,
    ) -> Self {
        Self {
            behaviour,
            instance,
            vm,
        }
    }

    pub fn get_interval(&self) -> Duration {
        self.behaviour.get_interval()
    }

    pub async fn run_random_action(&mut self) {
        let action_hash = self.behaviour.random_action();
        let action_out = self.vm.async_call(action_hash, (&self.instance, )).await;
        if let Err(e) = action_out {
            log::error!("Error executing action - {e}");
        }
    }

    pub async fn trigger_hook(&mut self, state: UserState) {
        let maybe_hook = self.behaviour.hook_action(state);
        if let Some(hook) = maybe_hook {
            let action_out = self.vm.async_call(hook, (&self.instance, )).await;
            if let Err(e) = action_out {
                log::error!("Error executing action - {e}");
            }
        }
    }
}