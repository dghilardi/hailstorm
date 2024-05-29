use crate::communication::protobuf::grpc;
use std::time::SystemTime;

use crate::communication::protobuf::grpc::{
    AgentSimulationState, ClientDistribution, LoadSimCommand,
};

#[derive(Clone)]
pub struct BotDef {
    pub model: String,
    pub shape: String,
}

impl From<BotDef> for ClientDistribution {
    fn from(ud: BotDef) -> Self {
        Self {
            model: ud.model,
            shape: ud.shape,
        }
    }
}

#[derive(Clone)]
pub struct SimulationDef {
    pub bots: Vec<BotDef>,
    pub script: String,
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
