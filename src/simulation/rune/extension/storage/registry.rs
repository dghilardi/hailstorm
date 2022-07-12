use std::sync::Arc;
use dashmap::DashMap;
use crate::simulation::rune::extension::storage::initializer::StorageInitializerRegistry;
use crate::simulation::rune::extension::storage::user_storage::UserStorage;

#[derive(Default)]
pub struct KeyValueStorage {
    values: DashMap<String, String>,
}

#[derive(Default)]
pub struct MultiStorage {
    storages: DashMap<String, KeyValueStorage>,
}

pub struct StorageRegistry {
    initializer: Box<dyn StorageInitializerRegistry + Send + Sync>,
    storage: Arc<DashMap<u32, MultiStorage>>,
}

pub struct StorageSlice {
    user_id: u32,
    name: String,
    storage: Arc<DashMap<u32, MultiStorage>>,
}

impl StorageSlice {
    pub fn read(&self, key: &str) -> Option<String> {
        self.storage
            .get(&self.user_id)
            .and_then(|user_data| user_data.storages.get(&self.name).and_then(|storage| storage.values.get(key).map(|v| v.clone())))
    }

    pub fn write(&mut self, key: String, value: String) {
        self.storage
            .entry(self.user_id).or_insert_with(Default::default).storages
            .entry(self.name.clone()).or_insert_with(Default::default).values
            .insert(key, value);
    }
}

impl StorageRegistry {
    pub fn new(initializer: impl StorageInitializerRegistry + Send + Sync + 'static) -> Self {
        Self {
            initializer: Box::new(initializer),
            storage: Arc::new(Default::default()),
        }
    }

    pub fn get_user_storage(&self, name: &str, user_id: u32) -> UserStorage {
        UserStorage::new(
            self.initializer.initial_values_for(name, user_id),
            StorageSlice {
                user_id,
                name: name.to_string(),
                storage: self.storage.clone(),
            }
        )
    }
}