use thiserror::Error;
use crate::utils::varint::{VarintDecode, VarintEncode};

#[derive(Clone)]
pub struct CompoundId<AgentId> {
    agent_id: AgentId,
    model_id: u32,
    user_id: u32,
}

#[derive(Error, Debug)]
pub enum CompoundIdParseError {
    #[error("Bad Format - {0}")]
    BadFormat(String)
}

impl <AgentId> CompoundId<AgentId> {
    pub fn new(
        agent_id: AgentId,
        model_id: u32,
        user_id: u32,
    ) -> Self {
        Self {
            agent_id,
            model_id,
            user_id,
        }
    }
    pub fn from_internal_id(agent_id: AgentId, internal_id: u64) -> Result<Self, CompoundIdParseError> {
        let sub_ids = Vec::<u32>::from_varint(&internal_id.to_be_bytes())
            .map_err(|e| CompoundIdParseError::BadFormat(e.to_string()))?;
        if sub_ids.len() != 2 {
            return Err(CompoundIdParseError::BadFormat(format!("Expected 2 subid in internal_id, found {}", sub_ids.len())));
        }
        Ok(Self {
            agent_id,
            model_id: sub_ids[0],
            user_id: sub_ids[1],
        })
    }

    pub fn internal_id(&self) -> u64 {
        let mut varint = vec![self.model_id, self.user_id].to_varint();
        varint.splice(0..0, vec![0; 8 - varint.len()]);
        u64::from_be_bytes(varint.try_into().expect("Error collecting bytes"))
    }

    pub fn user_id(&self) -> u32 {
        self.user_id
    }

    pub fn with_agent_id<NewAgentId>(self, agent_id: NewAgentId) -> CompoundId<NewAgentId> {
        CompoundId {
            agent_id,
            model_id: self.model_id,
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
        vec![
            self.agent_id,
            self.model_id,
            self.user_id,
        ].to_varint()
    }
}