use std::collections::HashMap;

pub mod empty;
pub mod csv;

pub trait StorageInitializerRegistry {
    fn initial_values_for(&self, user_id: u32) -> HashMap<String, String>;
    fn into_values(self) -> HashMap<u32, HashMap<String, String>>;
}