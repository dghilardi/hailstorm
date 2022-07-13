use std::time::Duration;
use rune::{FromValue, Hash};
use rune::runtime::{UnsafeToValue, VmError};
use crate::simulation::rune::extension::user::UserBehaviour;
use crate::simulation::rune::types::value::OwnedValue;
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

    pub async fn run_random_action(&mut self) -> Result<(), VmError> {
        let action_hash = self.behaviour.random_action();
        self.vm.async_call(action_hash, (&self.instance, ))
            .await
            .map(|_| ()) // ignore result
    }

    pub async fn execute_handler(&mut self, identifier: Hash, param: impl UnsafeToValue) -> Result<OwnedValue, VmError> {
        self.vm.async_call(identifier, (&self.instance, param))
            .await
            .map(OwnedValue::from_value)
            .map_err(|e| VmError::panic(e.to_string()))?
            .map_err(|e| VmError::panic(e.to_string()))
    }

    pub async fn trigger_hook(&mut self, state: UserState) -> Result<(), VmError> {
        let maybe_hook = self.behaviour.hook_action(state);
        if let Some(hook) = maybe_hook {
            self.vm.async_call(hook, (&self.instance, ))
                .await
                .map(|_| ()) // ignore result
        } else {
            Ok(())
        }
    }
}