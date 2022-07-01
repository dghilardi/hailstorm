use crate::communication::protobuf::grpc::{ModelStateSnapshot, ClientGroupStates};
use crate::simulation::simulation_actor::ClientStats;

impl From<ClientStats> for ModelStateSnapshot {
    fn from(cs: ClientStats) -> Self {
        Self {
            timestamp: Some(cs.timestamp.into()),
            states: cs.count_by_state.into_iter()
                .map(|(state, count)| ClientGroupStates {
                    state_id: state.into(),
                    count: count as u32,
                })
                .collect(),
        }
    }
}