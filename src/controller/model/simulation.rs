use crate::communication::protobuf::grpc;
use std::time::SystemTime;

use crate::communication::protobuf::grpc::{
    AgentSimulationState, ClientDistribution, LoadSimCommand,
};

#[derive(Clone, Default)]
pub struct BotDef {
    model: String,
    shape: String,
}

impl BotDef {
    /// Set model name for this bot definition
    pub fn model(self, model: &str) -> Self {
        Self {
            model: String::from(model),
            ..self
        }
    }

    /// Set shape for this model definition
    pub fn shape(self, shape: &str) -> Self {
        Self {
            shape: String::from(shape),
            ..self
        }
    }
}

impl From<BotDef> for ClientDistribution {
    fn from(ud: BotDef) -> Self {
        Self {
            model: ud.model,
            shape: ud.shape,
        }
    }
}

#[derive(Clone, Default)]
pub struct SimulationDef {
    pub(crate) bots: Vec<BotDef>,
    pub(crate) script: String,
}

impl SimulationDef {
    /// set bots for this simulation
    pub fn bots(self, bots: Vec<BotDef>) -> Self {
        Self { bots, ..self }
    }

    /// immutable bots reference
    pub fn bots_ref(&self) -> &[BotDef] {
        &self.bots
    }

    /// set script for this simulation
    pub fn script(self, script: String) -> Self {
        Self { script, ..self }
    }

    /// immutable script reference
    pub fn script_ref(&self) -> &str {
        &self.script
    }
}

impl From<SimulationDef> for LoadSimCommand {
    fn from(def: SimulationDef) -> Self {
        Self {
            clients_evolution: def
                .bots
                .into_iter()
                .map(|BotDef { model, shape }| ClientDistribution { model, shape })
                .collect(),
            script: def.script,
        }
    }
}

#[derive(Clone)]
pub enum SimulationState {
    Idle,
    Ready {
        simulation: SimulationDef,
    },
    Launched {
        start_ts: SystemTime,
        simulation: SimulationDef,
    },
}

impl SimulationState {
    pub fn is_aligned(&self, agent_sim_state: &grpc::AgentSimulationState) -> bool {
        match (self, agent_sim_state) {
            (
                SimulationState::Idle,
                AgentSimulationState::Idle | AgentSimulationState::Stopping,
            ) => true,
            (
                SimulationState::Idle,
                AgentSimulationState::Ready
                | AgentSimulationState::Waiting
                | AgentSimulationState::Running,
            ) => false,
            (SimulationState::Ready { .. }, AgentSimulationState::Ready) => true,
            (
                SimulationState::Ready { .. },
                AgentSimulationState::Idle
                | AgentSimulationState::Stopping
                | AgentSimulationState::Waiting
                | AgentSimulationState::Running,
            ) => false,
            (SimulationState::Launched { .. }, AgentSimulationState::Running) => true,
            (SimulationState::Launched { start_ts, .. }, AgentSimulationState::Waiting)
                if *start_ts > SystemTime::now() =>
            {
                true
            }
            (
                SimulationState::Launched { .. },
                AgentSimulationState::Idle
                | AgentSimulationState::Ready
                | AgentSimulationState::Waiting
                | AgentSimulationState::Stopping,
            ) => false,
        }
    }
}
