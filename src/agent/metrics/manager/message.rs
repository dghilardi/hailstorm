use crate::agent::metrics::storage::message::{MetricsFamilySnapshot, StartedTimer};
use crate::agent::metrics::timer::ExecutionInfo;
use actix::Message;
use std::time::SystemTime;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionTimerError {
    #[error("Internal Error - {0}")]
    InternalError(String),
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct StorageKey {
    pub(crate) model: String,
    pub(crate) action: String,
}

pub struct StartedActionTimer {
    pub(super) id: u32,
    pub(super) key: StorageKey,
    pub(super) timestamp: SystemTime,
}

impl From<StartedActionTimer> for StartedTimer {
    fn from(t: StartedActionTimer) -> Self {
        Self {
            id: t.id,
            timestamp: t.timestamp,
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<StartedActionTimer, ActionTimerError>")]
/// Message used to communicate to metrics manager to start a timer for a specific action
pub struct StartActionTimer {
    pub(super) model: String,
    pub(super) action: String,
}

impl StartActionTimer {
    pub fn new(model: &str, action: &str) -> Self {
        Self {
            model: model.to_string(),
            action: action.to_string(),
        }
    }
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn action(&self) -> &str {
        &self.action
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), ActionTimerError>")]
/// Message used to communicate to metrics manager to terminate an action-timer
pub struct StopActionTimer {
    pub(super) timer: StartedActionTimer,
    pub(super) execution: ExecutionInfo,
}

impl StopActionTimer {
    pub fn new(timer: StartedActionTimer, execution: ExecutionInfo) -> Self {
        Self { timer, execution }
    }

    pub fn timer(&self) -> &StartedActionTimer {
        &self.timer
    }

    pub fn execution(&self) -> &ExecutionInfo {
        &self.execution
    }
}

pub(crate) struct ActionMetricsFamilySnapshot {
    pub key: StorageKey,
    pub metrics: Vec<MetricsFamilySnapshot>,
}

#[derive(Message)]
#[rtype(result = "Vec<ActionMetricsFamilySnapshot>")]
pub(crate) struct FetchActionMetrics;
