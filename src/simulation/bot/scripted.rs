use crate::simulation::actor::bot::BotState;
use crate::simulation::rune::extension::bot::BotBehaviour;
use crate::simulation::rune::types::value::OwnedValue;
use rune::runtime::VmError;
use rune::{FromValue, Hash, ToValue};
use std::time::Duration;

pub struct ScriptedBot {
    behaviour: BotBehaviour,
    instance: rune::Value,
    vm: rune::Vm,
}

impl ScriptedBot {
    pub(crate) fn new(behaviour: BotBehaviour, instance: rune::Value, vm: rune::Vm) -> Self {
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
        let _result: rune::Value = self
            .vm
            .async_call(action_hash, (&self.instance,))
            .await?;
        Ok(())
    }

    pub async fn execute_handler(
        &mut self,
        identifier: Hash,
        param: OwnedValue,
    ) -> Result<OwnedValue, VmError> {
        let param_val = param.to_value().map_err(VmError::from)?;
        let result: rune::Value = self
            .vm
            .async_call(identifier, (&self.instance, param_val))
            .await?;
        OwnedValue::from_value(result).map_err(VmError::from)
    }

    pub async fn trigger_hook(&mut self, state: BotState) -> Result<(), VmError> {
        let maybe_hook = self.behaviour.hook_action(state);
        if let Some(hook) = maybe_hook {
            let _result: rune::Value = self
                .vm
                .async_call(hook, (&self.instance,))
                .await?;
            Ok(())
        } else {
            Ok(())
        }
    }
}
