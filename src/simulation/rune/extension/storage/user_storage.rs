use rune::Any;
use crate::simulation::rune::extension::storage::registry::StorageSlice;

#[derive(Any)]
pub struct UserStorage {
    storage: StorageSlice,
}

impl UserStorage {
    pub fn new(
        storage: StorageSlice,
    ) -> Self {
        Self {
            storage,
        }
    }

    pub fn read(&self, name: &str) -> Option<String> {
        self.storage.read(name)
    }

    pub fn write(&mut self, name: String, value: String) {
        self.storage.write(name, value);
    }
}