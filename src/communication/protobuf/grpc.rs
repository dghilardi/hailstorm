use std::time::SystemTime;
use crate::communication::protobuf::grpc::controller_command::Target;
tonic::include_proto!("hailstorm");

impl Target {
    pub fn includes_agent(&self, agent_id: u32) -> bool {
        match self {
            Target::Group(grp_id) => match AgentGroup::from_i32(*grp_id) {
                Some(AgentGroup::All) => true,
                None => false,
            },
            Target::AgentId(target_agent_id) => target_agent_id.eq(&agent_id),
            Target::Agents(MultiAgent{ agent_ids }) => agent_ids.iter().any(|target_agent_id| agent_id.eq(target_agent_id)),
        }
    }
}

impl AgentUpdate {
    pub fn last_ts(&self) -> Option<SystemTime> {
        self.stats.iter()
            .flat_map(|model_stats| model_stats.last_ts())
            .max()
    }

    pub fn update_ts(&self) -> Option<SystemTime> {
        self.timestamp.clone()
            .map(TryInto::try_into)
            .transpose()
            .ok()
            .flatten()
    }
}

impl ModelStats {
    pub fn last_ts(&self) -> Option<SystemTime> {
        let max_states_ts = self.states.iter()
            .flat_map(|state| state.timestamp.clone())
            .flat_map(|ts| SystemTime::try_from(ts).ok())
            .max();

        let max_perf_ts = self.performance.iter()
            .flat_map(|perf| perf.timestamp.clone())
            .flat_map(|ts| SystemTime::try_from(ts).ok())
            .max();

        [max_states_ts, max_perf_ts]
            .into_iter()
            .flatten()
            .max()
    }
}