#[derive(Clone)]
pub struct U32Mask {
    pub code: u32,
    pub bits: usize,
}

impl U32Mask {
    pub fn apply_mask(&self, num: u32) -> u32 {
        let mask = !0 >> self.bits;
        let family = (self.code << (32 - self.bits)) & !mask;
        let id = num & mask;
        family | id
    }

    pub fn remove_mask(&self, num: u32) -> u32 {
        (!0 >> self.bits) & num
    }

    pub fn matches(&self, num: u32) -> bool {
        (num >> (32 - self.bits)) == self.code
    }
}

#[derive(Clone)]
pub struct CompoundId<AgentId> {
    agent_id: AgentId,
    mask: U32Mask,
    user_id: u32,
}

impl <AgentId> CompoundId<AgentId> {
    pub fn from_user_id(agent_id: AgentId, user_id: u32) -> Self {
        Self {
            agent_id,
            mask: U32Mask {
                code: 0,
                bits: 0
            },
            user_id,
        }
    }

    pub fn internal_id(&self) -> u32 {
        self.mask.apply_mask(self.user_id)
    }

    pub fn user_id(&self) -> u32 {
        self.user_id
    }

    pub fn with_agent_id<NewAgentId>(self, agent_id: NewAgentId) -> CompoundId<NewAgentId> {
        CompoundId {
            agent_id,
            mask: self.mask,
            user_id: self.user_id,
        }
    }
}

impl CompoundId<u32> {
    pub fn global_id(&self) -> u64 {
        let agent_part = (self.agent_id as u64) << 32;
        agent_part | (self.internal_id() as u64)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let agent_id_complete_bytes = self.agent_id.to_be_bytes();
        let agent_id_bytes = if self.agent_id <= (u16::MAX as u32) {
            &agent_id_complete_bytes[2..]
        } else {
            &agent_id_complete_bytes
        };
        [
            agent_id_bytes,
            &self.internal_id().to_be_bytes()[..]
        ]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }
}