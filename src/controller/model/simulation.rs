use crate::communication::protobuf::grpc;
use std::time::SystemTime;

use crate::communication::protobuf::grpc::{
    AgentSimulationState, ClientDistribution, LoadSimCommand,
};

/// Definition of a bot type within a simulation, pairing a model name with its load shape expression.
#[derive(Clone, Default)]
pub struct BotDef {
    /// Name of the bot model (must match a struct in the Rune script).
    model: String,
    /// Mathematical expression defining the desired bot count over time (e.g., `"1000 * sin(t/10)"`).
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

/// Complete definition of a simulation, including bot definitions and the Rune script source.
#[derive(Clone, Default)]
pub struct SimulationDef {
    /// Bot types and their load shape expressions.
    pub(crate) bots: Vec<BotDef>,
    /// Rune script source code defining bot behaviors.
    pub(crate) script: String,
    /// Identifier assigned by the controller when the simulation is loaded.
    pub(crate) id: String,
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

    /// Identifier assigned by the controller, empty until the simulation is loaded
    pub fn id_ref(&self) -> &str {
        &self.id
    }

    /// Check that every shape parses and that model names are unique and non-empty.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.bots.is_empty() {
            return Err(String::from("At least one bot model is required"));
        }
        let mut models = std::collections::HashSet::new();
        for bot in &self.bots {
            if bot.model.is_empty() {
                return Err(String::from("Bot model names cannot be empty"));
            }
            if !models.insert(bot.model.as_str()) {
                return Err(format!(
                    "Bot model '{}' is defined more than once",
                    bot.model
                ));
            }
            crate::simulation::shape::parse_shape_fun(bot.shape.clone())
                .map_err(|err| format!("Invalid shape for model '{}' - {err}", bot.model))?;
        }
        Ok(())
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
            simulation_id: def.id,
        }
    }
}

/// State machine representing the controller's current simulation lifecycle phase.
#[derive(Clone)]
pub enum SimulationState {
    /// No simulation is loaded.
    Idle,
    /// A simulation definition is loaded and ready to be launched.
    Ready {
        /// The loaded simulation definition.
        simulation: SimulationDef,
    },
    /// The simulation has been launched and is running (or waiting to start).
    Launched {
        /// The scheduled start timestamp.
        start_ts: SystemTime,
        /// The active simulation definition.
        simulation: SimulationDef,
    },
}

impl SimulationState {
    /// Identifier of the loaded simulation, if any.
    pub fn simulation_id(&self) -> Option<&str> {
        match self {
            SimulationState::Idle => None,
            SimulationState::Ready { simulation }
            | SimulationState::Launched { simulation, .. } => Some(simulation.id_ref()),
        }
    }

    /// Check whether an agent's reported simulation state is aligned with the controller's desired state.
    ///
    /// Once a simulation is loaded, agents must also report its identifier: an agent running a
    /// different simulation is never aligned, even if its state matches.
    pub fn is_aligned(
        &self,
        agent_sim_state: &grpc::AgentSimulationState,
        agent_simulation_id: &str,
    ) -> bool {
        if self
            .simulation_id()
            .is_some_and(|id| id != agent_simulation_id)
        {
            return false;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Add;
    use std::time::Duration;

    fn make_launched_future() -> SimulationState {
        SimulationState::Launched {
            start_ts: SystemTime::now().add(Duration::from_secs(3600)),
            simulation: SimulationDef::default(),
        }
    }

    fn make_launched_past() -> SimulationState {
        SimulationState::Launched {
            start_ts: SystemTime::now() - Duration::from_secs(3600),
            simulation: SimulationDef::default(),
        }
    }

    #[test]
    fn idle_aligned_with_idle() {
        assert!(SimulationState::Idle.is_aligned(&AgentSimulationState::Idle, ""));
    }

    #[test]
    fn idle_aligned_with_stopping() {
        assert!(SimulationState::Idle.is_aligned(&AgentSimulationState::Stopping, ""));
    }

    #[test]
    fn idle_not_aligned_with_running() {
        assert!(!SimulationState::Idle.is_aligned(&AgentSimulationState::Running, ""));
    }

    #[test]
    fn idle_not_aligned_with_ready() {
        assert!(!SimulationState::Idle.is_aligned(&AgentSimulationState::Ready, ""));
    }

    #[test]
    fn ready_aligned_with_ready() {
        let state = SimulationState::Ready {
            simulation: SimulationDef::default(),
        };
        assert!(state.is_aligned(&AgentSimulationState::Ready, ""));
    }

    #[test]
    fn ready_not_aligned_with_idle() {
        let state = SimulationState::Ready {
            simulation: SimulationDef::default(),
        };
        assert!(!state.is_aligned(&AgentSimulationState::Idle, ""));
    }

    #[test]
    fn launched_aligned_with_running() {
        assert!(make_launched_past().is_aligned(&AgentSimulationState::Running, ""));
    }

    #[test]
    fn launched_future_aligned_with_waiting() {
        assert!(make_launched_future().is_aligned(&AgentSimulationState::Waiting, ""));
    }

    #[test]
    fn launched_past_not_aligned_with_waiting() {
        assert!(!make_launched_past().is_aligned(&AgentSimulationState::Waiting, ""));
    }

    #[test]
    fn launched_not_aligned_with_idle() {
        assert!(!make_launched_past().is_aligned(&AgentSimulationState::Idle, ""));
    }

    #[test]
    fn bot_def_builder() {
        let def = BotDef::default().model("test_bot").shape("1000 * sin(t)");
        assert_eq!(def.model, "test_bot");
        assert_eq!(def.shape, "1000 * sin(t)");
    }

    #[test]
    fn simulation_def_builder() {
        let def = SimulationDef::default()
            .bots(vec![BotDef::default().model("bot1")])
            .script("test script".into());
        assert_eq!(def.bots_ref().len(), 1);
        assert_eq!(def.script_ref(), "test script");
    }

    #[test]
    fn a_different_simulation_is_never_aligned() {
        let simulation = SimulationDef {
            id: String::from("run-2"),
            ..Default::default()
        };
        let ready = SimulationState::Ready {
            simulation: simulation.clone(),
        };
        assert!(ready.is_aligned(&AgentSimulationState::Ready, "run-2"));
        assert!(!ready.is_aligned(&AgentSimulationState::Ready, "run-1"));
        let launched = SimulationState::Launched {
            start_ts: SystemTime::now() - Duration::from_secs(1),
            simulation,
        };
        assert!(launched.is_aligned(&AgentSimulationState::Running, "run-2"));
        assert!(!launched.is_aligned(&AgentSimulationState::Running, ""));
    }

    #[test]
    fn invalid_definitions_are_rejected() {
        let bot = |model: &str, shape: &str| BotDef::default().model(model).shape(shape);
        for (bots, expected) in [
            (vec![], "At least one bot model"),
            (vec![bot("", "1")], "cannot be empty"),
            (vec![bot("A", "1"), bot("A", "2")], "more than once"),
            (vec![bot("A", "10 *")], "Invalid shape for model 'A'"),
            (vec![bot("A", "x + 1")], "Invalid shape for model 'A'"),
        ] {
            let error = SimulationDef::default().bots(bots).validate().unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
        assert!(SimulationDef::default()
            .bots(vec![bot("A", "100 * sin(t)")])
            .validate()
            .is_ok());
    }
}
