use std::collections::HashMap;

use super::StorageInitializerRegistry;

/// A no-op storage initializer.
///
/// `EmptyInitializer` is an implementation of the `StorageInitializerRegistry` that returns empty
/// data for any request. It is useful as a default or placeholder initializer in systems that either
/// do not require initial data loading or where initial data loading is conditionally bypassed.
///
/// This initializer can also be used in testing environments to mock or simulate storage initialization
/// without actual data.
pub struct EmptyInitializer;

impl StorageInitializerRegistry for EmptyInitializer {
    fn initial_values_for(&self, _name: &str, _bot_id: u32) -> HashMap<String, String> {
        Default::default()
    }
}
