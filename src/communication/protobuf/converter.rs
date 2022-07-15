use crate::agent::metrics::manager_actor::ActionMetricsFamilySnapshot;
use crate::communication::protobuf::grpc::{ModelStateSnapshot, ClientGroupStates};
use crate::grpc::{PerformanceHistogram, PerformanceSnapshot};
use crate::simulation::actor::simulation::ClientStats;

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

impl ActionMetricsFamilySnapshot {
    pub fn to_protobuf(&self) -> Vec<PerformanceSnapshot> {
        self.metrics.iter()
            .map(|metr| PerformanceSnapshot {
                timestamp: Some(metr.timestamp.into()),
                action: self.key.action.clone(),
                histograms: metr.metrics.iter()
                    .map(|(out, hist)| PerformanceHistogram {
                        status: *out,
                        buckets: hist.histogram.to_vec(),
                        sum: hist.sum
                    })
                    .collect()
            })
            .collect()
    }
}