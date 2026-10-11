use crate::communication::protobuf::grpc::AgentSimulationState;
use crate::controller::model::simulation::SimulationDef;
use std::time::SystemTime;

/// Errors returned when a simulation control command cannot be applied.
#[derive(Debug, thiserror::Error)]
pub enum SimulationControlError {
    /// The simulation definition is invalid and was not sent to the agents.
    #[error("Invalid simulation - {0}")]
    InvalidSimulation(String),
    /// The command requires a loaded simulation.
    #[error("No simulation is loaded")]
    NotLoaded,
}

#[derive(actix::Message)]
#[rtype(result = "Result<String, SimulationControlError>")]
/// Load a new simulation, returning the identifier assigned to it
pub struct LoadSimulation(pub(super) SimulationDef);

impl LoadSimulation {
    /// Create a new LoadSimulation message with given simulation definition
    pub fn new(def: SimulationDef) -> Self {
        Self(def)
    }

    /// Extract simulation definition
    pub fn extract_definition(self) -> SimulationDef {
        self.0
    }
}

#[derive(actix::Message)]
#[rtype(result = "Result<(), SimulationControlError>")]
/// Start the loaded simulation at a specific time
pub struct StartSimulation(pub(super) SystemTime);

impl StartSimulation {
    /// Create a new StartSimulation message with specific starting time
    pub fn at(time: SystemTime) -> Self {
        Self(time)
    }

    /// Extract start time ts
    pub fn extract_ts(&self) -> SystemTime {
        self.0
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
/// Stop the running simulation and unload it from every agent
pub struct StopSimulation;

#[derive(actix::Message)]
#[rtype(result = "SimulationStatus")]
/// Fetch the controller simulation state and the last state reported by each agent
pub struct GetSimulationStatus;

/// Lifecycle phase of the controller simulation.
#[derive(Debug, Clone, PartialEq)]
pub enum SimulationPhase {
    /// No simulation is loaded.
    Idle,
    /// A simulation is loaded and waiting to be started.
    Ready,
    /// The simulation has been launched at the given time.
    Launched {
        /// Scheduled start time.
        start_ts: SystemTime,
    },
}

/// Last state reported by an agent.
#[derive(Debug, Clone)]
pub struct AgentStatus {
    /// Agent identifier.
    pub agent_id: u32,
    /// Simulation state reported by the agent.
    pub state: AgentSimulationState,
    /// Identifier of the simulation loaded by the agent, empty when none is loaded.
    pub simulation_id: String,
    /// Why the agent could not load `simulation_id`.
    pub simulation_error: Option<String>,
    /// Timestamp of the last update received from the agent.
    pub last_update: SystemTime,
}

/// Snapshot of the controller simulation and of the connected agents.
#[derive(Debug, Clone, actix::MessageResponse)]
pub struct SimulationStatus {
    /// Controller lifecycle phase.
    pub phase: SimulationPhase,
    /// Identifier of the loaded simulation, if any.
    pub simulation_id: Option<String>,
    /// Connected agents, sorted by identifier.
    pub agents: Vec<AgentStatus>,
}
