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

/// Overall progress of the loaded simulation across the connected agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationOutcome {
    /// No simulation is loaded.
    Idle,
    /// Some agents have not confirmed the current simulation yet.
    Pending,
    /// Every agent loaded the simulation, which has not been started.
    Ready,
    /// Every agent is running (or waiting to start) the simulation.
    Running,
    /// At least one agent could not load the simulation.
    Failed,
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

impl SimulationStatus {
    /// Summarize the agents' confirmations of the current simulation.
    pub fn outcome(&self) -> SimulationOutcome {
        let Some(simulation_id) = self.simulation_id.as_deref() else {
            return SimulationOutcome::Idle;
        };
        let current = |agent: &&AgentStatus| agent.simulation_id == simulation_id;
        if self
            .agents
            .iter()
            .filter(current)
            .any(|agent| agent.simulation_error.is_some())
        {
            return SimulationOutcome::Failed;
        }
        let (expected, outcome): (&[AgentSimulationState], _) = match self.phase {
            SimulationPhase::Idle => return SimulationOutcome::Idle,
            SimulationPhase::Ready => (&[AgentSimulationState::Ready], SimulationOutcome::Ready),
            SimulationPhase::Launched { .. } => (
                &[AgentSimulationState::Waiting, AgentSimulationState::Running],
                SimulationOutcome::Running,
            ),
        };
        let confirmed = !self.agents.is_empty()
            && self.agents.iter().all(|agent| {
                agent.simulation_id == simulation_id && expected.contains(&agent.state)
            });
        if confirmed {
            outcome
        } else {
            SimulationOutcome::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(
        agent_id: u32,
        state: AgentSimulationState,
        simulation_id: &str,
        error: Option<&str>,
    ) -> AgentStatus {
        AgentStatus {
            agent_id,
            state,
            simulation_id: simulation_id.to_string(),
            simulation_error: error.map(String::from),
            last_update: SystemTime::now(),
        }
    }

    fn status(phase: SimulationPhase, agents: Vec<AgentStatus>) -> SimulationStatus {
        SimulationStatus {
            simulation_id: (phase != SimulationPhase::Idle).then(|| String::from("run")),
            phase,
            agents,
        }
    }

    #[test]
    fn outcome_reflects_agent_confirmations() {
        use AgentSimulationState::*;
        let launched = || SimulationPhase::Launched {
            start_ts: SystemTime::now(),
        };
        let cases = [
            (
                status(SimulationPhase::Idle, vec![agent(1, Idle, "", None)]),
                SimulationOutcome::Idle,
            ),
            (
                status(SimulationPhase::Ready, vec![]),
                SimulationOutcome::Pending,
            ),
            (
                status(
                    SimulationPhase::Ready,
                    vec![agent(1, Ready, "run", None), agent(2, Ready, "old", None)],
                ),
                SimulationOutcome::Pending,
            ),
            (
                status(
                    SimulationPhase::Ready,
                    vec![agent(1, Ready, "run", None), agent(2, Ready, "run", None)],
                ),
                SimulationOutcome::Ready,
            ),
            (
                status(
                    SimulationPhase::Ready,
                    vec![
                        agent(1, Ready, "run", None),
                        agent(2, Idle, "run", Some("bad")),
                    ],
                ),
                SimulationOutcome::Failed,
            ),
            (
                status(
                    SimulationPhase::Ready,
                    vec![
                        agent(1, Idle, "old", Some("bad")),
                        agent(2, Ready, "run", None),
                    ],
                ),
                SimulationOutcome::Pending,
            ),
            (
                status(
                    launched(),
                    vec![
                        agent(1, Waiting, "run", None),
                        agent(2, Running, "run", None),
                    ],
                ),
                SimulationOutcome::Running,
            ),
            (
                status(launched(), vec![agent(1, Ready, "run", None)]),
                SimulationOutcome::Pending,
            ),
        ];
        for (index, (status, expected)) in cases.into_iter().enumerate() {
            assert_eq!(status.outcome(), expected, "case {index}");
        }
    }
}
