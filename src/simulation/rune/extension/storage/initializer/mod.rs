use std::collections::HashMap;

pub mod empty;
pub mod csv;

pub trait StorageInitializerRegistry {
    fn initial_values_for(&self, name: &str, bot_id: u32) -> HashMap<String, String>;
}