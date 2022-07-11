use std::sync::Arc;
use dashmap::DashMap;
use crate::simulation::rune::extension::storage::initializer::StorageInitializerRegistry;
use crate::simulation::rune::extension::storage::user_storage::UserStorage;

pub struct StorageRegistry {
    storage: Arc<DashMap<(u32, String), String>>
}

impl StorageRegistry {
    pub fn new(initializer: impl StorageInitializerRegistry + Send + Sync + 'static) -> Self {
        let storage = initializer.into_values()
            .into_iter()
            .flat_map(|(user_id, values)| values.into_iter().map(move |(k, v)| ((user_id, k), v)))
            .collect();
        Self {
            storage: Arc::new(storage)
        }
    }

    pub fn get_user_storage(&self, user_id: u32) -> UserStorage {
        UserStorage::new(user_id, self.storage.clone())
    }
}