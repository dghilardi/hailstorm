use crate::grpc::controller_command::Target;
tonic::include_proto!("hailstorm");

impl Target {
    pub fn includes_agent(&self, agent_id: u64) -> bool {
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