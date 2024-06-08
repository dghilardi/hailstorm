use crate::utils::varint::{VarintDecode, VarintEncode};
use thiserror::Error;

/// Represents a compound identifier consisting of an agent ID, a model ID, and a bot ID.
///
/// This struct is designed to encapsulate a multiple level identifier into a unique composed identifier.
/// It provides functionality to construct a new `CompoundId`, parse from an internal representation,
/// and convert back to various forms.
///
/// # Generics
///
/// - `AgentId`: The type of the agent ID. This allows for flexibility in specifying what
/// type the agent ID should be, accommodating different use cases or identifier schemes.
///
/// # Fields
///
/// - `agent_id`: The identifier for the agent. Its type is generic.
/// - `model_id`: A 32-bit unsigned integer representing the model ID.
/// - `bot_id`: A 32-bit unsigned integer representing the bot ID.
///
/// # Errors
///
/// - `CompoundIdParseError::BadFormat`: Indicates that the internal ID could not be parsed
/// into a valid `CompoundId` due to incorrect format or content.
///
/// # Examples
///
/// Creating a new `CompoundId`:
///
/// ```
/// use hailstorm::simulation::compound_id::CompoundId;
///
/// let compound_id = CompoundId::new(123, 456, 789);
/// ```
///
/// Parsing a `CompoundId` from an internal ID:
///
/// ```
/// use hailstorm::simulation::compound_id::CompoundId;
///
/// let compound_id = CompoundId::from_internal_id(12, 0x1719u64).unwrap();
/// ```
///
/// Getting the internal ID representation:
///
/// ```
/// use hailstorm::simulation::compound_id::CompoundId;
/// let compound_id = CompoundId::new(123, 456, 789);
///
/// let internal_id = compound_id.internal_id();
/// ```
#[derive(Clone)]
pub struct CompoundId<AgentId> {
    agent_id: AgentId,
    model_id: u32,
    bot_id: u32,
}

/// An error type for failures encountered while parsing a `CompoundId` from an internal representation.
///
/// This enum is used to represent errors that may occur when attempting to decode a `CompoundId` from a
/// compact or serialized form, such as incorrect format, missing components, or invalid data.
///
/// # Variants
///
/// - `BadFormat(String)`: Indicates that the internal ID could not be parsed into a valid `CompoundId`
/// due to incorrect format or content. The contained `String` provides a descriptive error message.
///
/// # Usage
///
/// `CompoundIdParseError` is typically used in the result of functions or methods that parse `CompoundId`
/// instances from serialized or compact representations, providing detailed error information in case of failure.
#[derive(Error, Debug)]
pub enum CompoundIdParseError {
    #[error("Bad Format - {0}")]
    BadFormat(String),
}

impl<AgentId> CompoundId<AgentId> {
    /// Constructs a new `CompoundId`.
    ///
    /// # Parameters
    ///
    /// - `agent_id`: The identifier for the agent. Its type is generic, allowing flexibility in specifying the agent ID's type.
    /// - `model_id`: A 32-bit unsigned integer representing the model ID.
    /// - `bot_id`: A 32-bit unsigned integer representing the bot ID.
    ///
    /// # Returns
    ///
    /// Returns an instance of `CompoundId` with the specified agent, model, and bot IDs.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::compound_id::CompoundId;
    ///
    /// let agent_id = 42;
    /// let model_id = 100;
    /// let bot_id = 200;
    /// let compound_id = CompoundId::new(agent_id, model_id, bot_id);
    /// ```
    pub fn new(agent_id: AgentId, model_id: u32, bot_id: u32) -> Self {
        Self {
            agent_id,
            model_id,
            bot_id,
        }
    }

    /// Parses a `CompoundId` from an internal ID representation.
    ///
    /// This method attempts to decode a given internal ID into its constituent parts (model ID and bot ID),
    /// using the provided agent ID to construct a complete `CompoundId`.
    ///
    /// # Parameters
    ///
    /// - `agent_id`: The identifier for the agent. This ID is directly used in the resulting `CompoundId`.
    /// - `internal_id`: A 64-bit unsigned integer representing the encoded model and bot IDs.
    ///
    /// # Returns
    ///
    /// If successful, returns `Ok(CompoundId)` containing the decoded IDs. If the internal ID cannot be
    /// correctly parsed, returns `Err(CompoundIdParseError::BadFormat)` with an error message.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::compound_id::CompoundId;
    ///
    /// let agent_id = 42;
    /// let internal_id = 0x040507u64;
    /// let compound_id = CompoundId::from_internal_id(agent_id, internal_id).unwrap();
    /// ```
    pub fn from_internal_id(
        agent_id: AgentId,
        internal_id: u64,
    ) -> Result<Self, CompoundIdParseError> {
        let sub_ids = Vec::<u32>::from_varint(&internal_id.to_be_bytes())
            .map_err(|e| CompoundIdParseError::BadFormat(e.to_string()))?;
        if sub_ids.len() != 2 {
            return Err(CompoundIdParseError::BadFormat(format!(
                "Expected 2 subid in internal_id, found {}",
                sub_ids.len()
            )));
        }
        Ok(Self {
            agent_id,
            model_id: sub_ids[0],
            bot_id: sub_ids[1],
        })
    }

    /// Generates an internal ID representation of the `CompoundId`.
    ///
    /// This method combines the model ID and bot ID into a single 64-bit unsigned integer,
    /// suitable for compact storage or transmission.
    ///
    /// # Returns
    ///
    /// Returns a `u64` representing the encoded model and bot IDs.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::compound_id::CompoundId;
    ///
    /// let compound_id = CompoundId::new(42, 100, 200);
    /// let internal_id = compound_id.internal_id();
    /// ```
    pub fn internal_id(&self) -> u64 {
        let mut varint = vec![self.model_id, self.bot_id].to_varint();
        varint.splice(0..0, vec![0; 8 - varint.len()]);
        u64::from_be_bytes(varint.try_into().expect("Error collecting bytes"))
    }

    /// Retrieves the bot ID from the `CompoundId`.
    ///
    /// # Returns
    ///
    /// Returns a `u32` representing the bot ID stored within the `CompoundId`.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::compound_id::CompoundId;
    ///
    /// let compound_id = CompoundId::new(42, 100, 200);
    /// let bot_id = compound_id.bot_id();
    /// ```
    pub fn bot_id(&self) -> u32 {
        self.bot_id
    }

    /// Creates a new `CompoundId` instance with a different agent ID, preserving the model and bot IDs.
    ///
    /// This method allows changing the type of the agent ID, making it flexible for scenarios where
    /// the agent ID's type might differ between contexts.
    ///
    /// # Parameters
    ///
    /// - `agent_id`: The new agent ID to be used in the returned `CompoundId`. The type of this parameter
    /// can differ from the original `CompoundId`'s agent ID type.
    ///
    /// # Returns
    ///
    /// Returns a new `CompoundId` instance with the specified agent ID and the original model and bot IDs.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::compound_id::CompoundId;
    ///
    /// let compound_id = CompoundId::new(42u32, 100, 200);
    /// let new_agent_id = "Agent42";
    /// let new_compound_id = compound_id.with_agent_id(new_agent_id);
    /// ```
    pub fn with_agent_id<NewAgentId>(self, agent_id: NewAgentId) -> CompoundId<NewAgentId> {
        CompoundId {
            agent_id,
            model_id: self.model_id,
            bot_id: self.bot_id,
        }
    }
}

impl CompoundId<u32> {
    /// Generates a global ID for `CompoundId<u32>` instances.
    ///
    /// This method combines the agent, model, and bot IDs into a single 64-bit unsigned integer,
    /// providing a unique identifier that encompasses all three components.
    ///
    /// # Returns
    ///
    /// Returns a `u64` representing the combined agent, model, and bot IDs.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::compound_id::CompoundId;
    ///
    /// let compound_id = CompoundId::new(1u32, 2, 3);
    /// let global_id = compound_id.global_id();
    /// ```
    pub fn global_id(&self) -> u64 {
        let mut varint = vec![self.agent_id, self.model_id, self.bot_id].to_varint();
        varint.splice(0..0, vec![0; 8 - varint.len()]);
        u64::from_be_bytes(varint.try_into().expect("Error collecting bytes"))
    }

    /// Converts the `CompoundId` into a byte vector representation.
    ///
    /// This method is useful for serializing the `CompoundId` for storage or network transmission.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<u8>` containing the varint-encoded agent, model, and bot IDs.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::compound_id::CompoundId;
    ///
    /// let compound_id = CompoundId::new(1u32, 2, 3);
    /// let bytes = compound_id.into_bytes();
    /// ```
    pub fn into_bytes(self) -> Vec<u8> {
        vec![self.agent_id, self.model_id, self.bot_id].to_varint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_new_compound_id() {
        let agent_id = 1u32;
        let model_id = 2u32;
        let bot_id = 3u32;
        let compound_id = CompoundId::new(agent_id, model_id, bot_id);

        assert_eq!(compound_id.agent_id, agent_id);
        assert_eq!(compound_id.model_id, model_id);
        assert_eq!(compound_id.bot_id, bot_id);
    }

    #[test]
    fn test_from_internal_id_success() {
        let agent_id = 1u32;
        let internal_id = 0x0507u64;
        let compound_id = CompoundId::from_internal_id(agent_id, internal_id).unwrap();

        assert_eq!(compound_id.agent_id, agent_id);
        assert_eq!(compound_id.model_id, 2);
        assert_eq!(compound_id.bot_id, 3);
    }

    #[test]
    fn test_from_internal_id_bad_format() {
        let agent_id = 1u32;
        let internal_id = 0xFFFFFFFFFFFFFFFFu64; // An invalid internal ID
        let result = CompoundId::from_internal_id(agent_id, internal_id);

        assert!(matches!(result, Err(CompoundIdParseError::BadFormat(_))));
    }

    #[test]
    fn test_internal_id_conversion() {
        let agent_id = 1u32;
        let model_id = 2u32;
        let bot_id = 3u32;
        let compound_id = CompoundId::new(agent_id, model_id, bot_id);

        let internal_id = compound_id.internal_id();
        assert_eq!(internal_id, 0x0507u64);
    }

    #[test]
    fn test_global_id_for_u32_agent_id() {
        let agent_id = 1u32;
        let model_id = 2u32;
        let bot_id = 3u32;
        let compound_id = CompoundId::new(agent_id, model_id, bot_id);

        let global_id = compound_id.global_id();
        assert_eq!(global_id, 0x00030507u64);
    }

    #[test]
    fn test_into_bytes() {
        let agent_id = 1u32;
        let model_id = 2u32;
        let bot_id = 3u32;
        let compound_id = CompoundId::new(agent_id, model_id, bot_id);

        let bytes = compound_id.into_bytes();
        assert_eq!(vec![3u8, 5u8, 7u8], bytes);
    }
}
