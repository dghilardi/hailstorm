use crate::simulation::rune::extension::storage::registry::StorageSlice;
use rune::Any;
use std::collections::HashMap;

#[derive(Any)]
pub struct BotStorage {
    init: HashMap<String, String>,
    storage: StorageSlice,
}

impl BotStorage {
    pub fn new(init: HashMap<String, String>, storage: StorageSlice) -> Self {
        Self { init, storage }
    }

    pub fn read(&self, name: &str) -> Option<String> {
        self.storage
            .read(name)
            .or_else(|| self.init.get(name).cloned())
    }

    pub fn write(&mut self, name: String, value: String) {
        self.storage.write(name, value);
    }
}
