use std::time::Duration;

pub type ActionOutcome = i64;

pub struct Timer {
    id: u32,
    execution_info: Option<ExecutionInfo>,
}

#[derive(Clone)]
pub struct ExecutionInfo {
    pub elapsed: Duration,
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
