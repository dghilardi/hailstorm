use std::time::SystemTime;

use crate::grpc::{ClientDistribution, LoadSimCommand};

#[derive(Clone)]
pub struct UserDef {
    pub model: String,
    pub shape: String,
}

#[derive(Clone)]
pub struct SimulationDef {
    pub users: Vec<UserDef>,
    pub script: String,
}

impl From<SimulationDef> for LoadSimCommand {
    fn from(def: SimulationDef) -> Self {
        Self {
            clients_evolution: def.users.into_iter()
                .map(|UserDef { model, shape }| ClientDistribution { model, shape })
                .collect(),
            script: def.script,
        }
    }
}

#[derive(Clone)]
pub enum SimulationState {
    Idle,
    Ready { simulation: SimulationDef },
    Launched { start_ts: SystemTime, simulation: SimulationDef },
}