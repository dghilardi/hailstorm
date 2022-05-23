use crate::grpc::{AgentStats, ClientGroupStates};
use crate::simulation::simulation_actor::ClientStats;

impl From<ClientStats> for AgentStats {
    fn from(cs: ClientStats) -> Self {
        Self {
            states: cs.count_by_state.into_iter()
                .map(|(state, count)| ClientGroupStates {
                    state_id: state.into(),
                    count: count as u32,
                })
                .collect(),
            model: cs.model,
        }
    }
}