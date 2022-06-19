use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use crate::simulation::rune::extension::storage::storage::UserStorage;

pub struct StorageRegistry {
    storage: Arc<DashMap<(u32, String), String>>
}

impl StorageRegistry {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Default::default())
        }
    }

    pub fn get_user_storage(&self, user_id: u32) -> UserStorage {
        UserStorage::new(user_id, self.storage.clone())
    }
}