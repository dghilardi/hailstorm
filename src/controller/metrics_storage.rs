use crate::grpc::AgentUpdate;

pub trait MetricsStorage {
    fn store(&mut self, agent_update: &AgentUpdate);
}