use crate::controller::model::simulation::SimulationDef;
use std::time::SystemTime;

#[derive(actix::Message)]
#[rtype(result = "()")]
/// Load a new simulation
pub struct LoadSimulation(pub(super) SimulationDef);

impl LoadSimulation {
    /// Create a new LoadSimulation message with given simulation definition
    pub fn new(def: SimulationDef) -> Self {
        Self(def)
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
/// Start the loaded simulation at a specific time
pub struct StartSimulation(pub(super) SystemTime);

impl StartSimulation {
    /// Create a new StartSimulation message with specific starting time
    pub fn at(time: SystemTime) -> Self {
        Self(time)
    }
}
