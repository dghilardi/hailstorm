use std::time::Duration;

/// Numeric identifier for an action's outcome (e.g., HTTP status code).
pub type ActionOutcome = i64;

/// A pending timer tracking a single action execution.
pub struct Timer {
    id: u32,
    execution_info: Option<ExecutionInfo>,
}

/// Information collected upon completion of an action execution.
#[derive(Clone)]
pub struct ExecutionInfo {
    /// How long the action took to complete.
    pub elapsed: Duration,
    /// The outcome of the action (e.g., HTTP status code or custom status).
    pub outcome: ActionOutcome,
}

impl Timer {
    pub fn empty(id: u32) -> Self {
        Self {
            id,
            execution_info: None,
        }
    }

    pub fn set_execution(&mut self, elapsed: Duration, outcome: i64) {
        self.execution_info = Some(ExecutionInfo { elapsed, outcome })
    }

    pub fn get_execution(&self) -> Option<ExecutionInfo> {
        self.execution_info.clone()
    }
    pub fn get_id(&self) -> u32 {
        self.id
    }
}
