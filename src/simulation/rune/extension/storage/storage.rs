use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use rune::Any;

#[derive(Any)]
pub struct UserStorage {
    user_id: u32,
    storage: Arc<DashMap<(u32, String), String>>,
}

impl UserStorage {
    pub fn new(
        user_id: u32,
        storage: Arc<DashMap<(u32, String), String>>,
    ) -> Self {
        Self {
            user_id,
            storage,
        }
    }

    pub fn read(&self, name: &str) -> Option<String> {
        self.storage.get(&(self.user_id, name.to_string())).map(|v| v.clone())
    }

    pub fn write(&mut self, name: String, value: String) {
        self.storage.insert((self.user_id, name), value);
    }
}