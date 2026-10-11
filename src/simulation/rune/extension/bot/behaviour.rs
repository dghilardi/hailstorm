use crate::simulation::actor::bot::BotState;
use rand::Rng;
use rune::runtime::Function;
use rune::{Any, Hash};
use std::collections::HashMap;
use std::time::Duration;

/// Describes a bot's behavior: its periodic actions, state hooks, and tick interval.
///
/// Populated by the `register_bot` function in a Rune script. Actions are selected
/// randomly based on their weights; hooks fire when the bot enters a specific state.
#[derive(Clone, Debug, Any)]
#[rune(item = ::hailstorm::bot)]
pub struct BotBehaviour {
    total_weight: f64,
    interval: Duration,
    actions: Vec<BotAction>,
    hooks: HashMap<BotState, Hash>,
}

/// Determines when a registered action should be triggered.
#[derive(Clone, Debug, Any)]
#[rune(item = ::hailstorm::bot)]
pub enum ActionTrigger {
    /// Triggered periodically while the bot is alive, with the given weight for random selection.
    Alive { weight: f32 },
    /// Triggered once when the bot enters the specified state.
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
    pub fn register_action(&mut self, trigger: ActionTrigger, action: Function) {
        let hash = action.type_hash();
        match trigger {
            ActionTrigger::Alive { weight } => {
                let weight = weight.max(0f32);
                self.total_weight += weight as f64;
                self.actions.push(BotAction { hash, weight });
            }
            ActionTrigger::EnterState { state } => {
                let overridden_action = self.hooks.insert(state, hash);
                if let Some(overridden_hash) = overridden_action {
                    log::warn!("[{:?}] overridden: {} -> {}", state, overridden_hash, hash)
                }
            }
        }
    }

    pub fn set_interval_millis(&mut self, interval: u64) {
        self.interval = Duration::from_millis(interval);
    }

    /// Check the registered configuration before any bot is spawned.
    pub fn validate(&self) -> Result<(), String> {
        if self.interval.is_zero() {
            return Err(String::from(
                "the action interval must be greater than zero",
            ));
        }
        if !self.total_weight.is_finite() {
            return Err(String::from("action weights must be finite"));
        }
        Ok(())
    }

    /// Whether at least one periodic action can be selected.
    pub fn has_actions(&self) -> bool {
        self.total_weight > 0.0
    }

    /// Pick a periodic action according to its weight, if any can be selected.
    pub fn random_action(&self) -> Option<Hash> {
        if !self.has_actions() {
            return None;
        }
        let mut rand = rand::rng().random_range(0f64..self.total_weight);
        for act in self.actions.iter().filter(|act| act.weight > 0.0) {
            rand -= act.weight as f64;
            if rand <= 0f64 {
                return Some(act.hash);
            }
        }
        self.actions
            .iter()
            .rev()
            .find(|act| act.weight > 0.0)
            .map(|act| act.hash)
    }

    pub fn hook_action(&self, state: BotState) -> Option<Hash> {
        self.hooks.get(&state).cloned()
    }

    pub fn get_interval(&self) -> Duration {
        self.interval
    }
}
