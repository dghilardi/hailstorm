use std::sync::Arc;
use dashmap::DashMap;
use crate::simulation::rune::extension::storage::initializer::StorageInitializerRegistry;
use crate::simulation::rune::extension::storage::bot_storage::BotStorage;

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
    bot_id: u32,
    name: String,
    storage: Arc<DashMap<u32, MultiStorage>>,
}

impl StorageSlice {
    pub fn read(&self, key: &str) -> Option<String> {
        self.storage
            .get(&self.bot_id)
            .and_then(|bot_data| bot_data.storages.get(&self.name).and_then(|storage| storage.values.get(key).map(|v| v.clone())))
    }

    pub fn write(&mut self, key: String, value: String) {
        self.storage
            .entry(self.bot_id).or_insert_with(Default::default).storages
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

    pub fn get_bot_storage(&self, name: &str, bot_id: u32) -> BotStorage {
        BotStorage::new(
            self.initializer.initial_values_for(name, bot_id),
            StorageSlice {
                bot_id,
                name: name.to_string(),
                storage: self.storage.clone(),
            }
        )
    }
}