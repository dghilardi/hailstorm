use std::collections::HashMap;
use std::time::Duration;
use rand::{Rng, thread_rng};
use rune::{Any, Hash};
use rune::runtime::{Function, Shared};
use crate::simulation::actor::bot::BotState;

#[derive(Clone, Debug, Any)]
pub struct BotBehaviour {
    total_weight: f64,
    interval: Duration,
    actions: Vec<BotAction>,
    hooks: HashMap<BotState, Hash>,
}

#[derive(Clone, Debug, Any)]
pub enum ActionTrigger {
    Alive { weight: f32 },
    EnterState { state: BotState },
}

#[derive(Clone, Debug)]
struct BotAction {
    weight: f32,
    hash: Hash,
}

impl Default for BotBehaviour {
    fn default() -> Self {
        Self {
            total_weight: 0.0,
            interval: Duration::from_millis(5_000),
            actions: vec![],
            hooks: Default::default(),
        }
    }
}

impl BotBehaviour {
    pub fn register_action(&mut self, trigger: ActionTrigger, action: Shared<Function>) {
        let hash = action.take().expect("Error extracting action hash").type_hash();
        match trigger {
            ActionTrigger::Alive { weight } => {
                let weight = weight.max(0f32);
                self.total_weight += weight as f64;
                self.actions.push(BotAction { hash, weight });
            },
            ActionTrigger::EnterState { state } => {
                let overridden_action = self.hooks.insert(state, hash);
                if let Some(overridden_hash) = overridden_action {
                    log::warn!("[{:?}] overridden: {} -> {}", state, overridden_hash, hash)
                }
            },
        }
    }

    pub fn set_interval_millis(&mut self, interval: u64) {
        self.interval = Duration::from_millis(interval);
    }

    pub fn random_action(&self) -> Hash {
        let mut rand = thread_rng().gen_range(0f64..self.total_weight);
        for act in &self.actions {
            rand -= act.weight as f64;
            if rand <= 0f64 {
                return act.hash;
            }
        }
        return self.actions.last().expect("No actions found").hash;
    }

    pub fn hook_action(&self, state: BotState) -> Option<Hash> {
        self.hooks
            .get(&state)
            .cloned()
    }

    pub fn get_interval(&self) -> Duration {
        self.interval
    }
}