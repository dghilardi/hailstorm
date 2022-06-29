use std::time::Instant;
use rune::{Any, Value};
use rune::runtime::{Function, Shared, VmError};

#[derive(Any)]
pub struct PerformanceRegistry;

impl PerformanceRegistry {
    pub fn new() -> Self {
        Self
    }

    pub async fn observe(&self, name: &str, action: Function) -> Result<i64, VmError> {
        let before = Instant::now();
        let res = action.async_send_call(()).await;
        let elapsed = before.elapsed().as_millis();
        res
    }
}