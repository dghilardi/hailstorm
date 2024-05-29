use std::collections::HashMap;

pub mod csv;
pub mod empty;

pub trait StorageInitializerRegistry {
    fn initial_values_for(&self, name: &str, bot_id: u32) -> HashMap<String, String>;
}
