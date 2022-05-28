use std::time::Duration;
use rand::{Rng, thread_rng};
use rune::{Any, ContextError, Hash, Module};
use rune::runtime::{Function, Shared};

#[derive(Clone, Debug, Any)]
pub struct UserBehaviour {
    total_weight: f64,
    interval: Duration,
    actions: Vec<UserAction>
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
            actions: vec![]
        }
    }
}

impl UserBehaviour {
    fn register_action(&mut self, weight: f32, action: Shared<Function>) {
        let weight = weight.max(0f32);
        let hash = action.take().expect("Error extracting action hash").type_hash();
        self.total_weight += weight as f64;
        self.actions.push(UserAction { hash, weight });
    }

    fn set_interval_millis(&mut self, interval: u64) {
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

    pub fn get_interval(&self) -> Duration {
        self.interval
    }
}

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["user"]);

    module.ty::<UserBehaviour>()?;
    module.inst_fn("register_action", UserBehaviour::register_action)?;
    module.inst_fn("set_interval_millis", UserBehaviour::set_interval_millis)?;

    Ok(module)
}