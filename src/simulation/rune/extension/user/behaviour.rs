use std::collections::HashMap;
use std::time::Duration;
use rand::{Rng, thread_rng};
use rune::{Any, Hash};
use rune::runtime::{Function, Shared};
use crate::simulation::user_actor::UserState;

#[derive(Clone, Debug, Any)]
pub struct UserBehaviour {
    total_weight: f64,
    interval: Duration,
    actions: Vec<UserAction>,
    hooks: HashMap<UserState, Hash>,
}

#[derive(Clone, Debug, Any)]
pub enum ActionTrigger {
    Alive { weight: f32 },
    EnterState { state: UserState },
}

#[derive(Clone, Debug)]
struct UserAction {
    weight: f32,
    hash: Hash,
}

impl Default for UserBehaviour {
    fn default() -> Self {
        Self {
            total_weight: 0.0,
            interval: Duration::from_millis(5_000),
            actions: vec![],
            hooks: Default::default(),
        }
    }
}

impl UserBehaviour {
    pub fn register_action(&mut self, trigger: ActionTrigger, action: Shared<Function>) {
        let hash = action.take().expect("Error extracting action hash").type_hash();
        match trigger {
            ActionTrigger::Alive { weight } => {
                let weight = weight.max(0f32);
                self.total_weight += weight as f64;
                self.actions.push(UserAction { hash, weight });
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

    pub fn hook_action(&self, state: UserState) -> Option<Hash> {
        self.hooks
            .get(&state)
            .cloned()
    }

    pub fn get_interval(&self) -> Duration {
        self.interval
    }
}