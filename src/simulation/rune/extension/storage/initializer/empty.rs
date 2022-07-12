use std::collections::HashMap;

use super::StorageInitializerRegistry;

pub struct EmptyInitializer;

impl StorageInitializerRegistry for EmptyInitializer {
    fn initial_values_for(&self, _name: &str, _user_id: u32) -> HashMap<String, String> {
        Default::default()
    }
}