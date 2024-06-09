use std::collections::HashMap;

pub mod csv;
pub mod empty;

/// A trait for initializing storage with initial values.
///
/// Implementors of this trait provide a mechanism to populate storage with initial data based on
/// a given storage name and bot ID. This is useful in scenarios where bots or agents require
/// predefined data at the start of a simulation.
///
/// # Methods
///
/// - `initial_values_for`: Given a storage name and bot ID, returns a `HashMap` containing the
/// initial key-value pairs to populate the storage.
pub trait StorageInitializerRegistry {
    /// Provides initial values for a specified storage and bot ID.
    ///
    /// # Parameters
    ///
    /// - `name`: A string slice representing the name of the storage. This could correspond to different
    /// types or categories of data needed by bots.
    /// - `bot_id`: A 32-bit unsigned integer representing the unique identifier of the bot.
    /// This allows for customization of initial data on a per-bot basis.
    ///
    /// # Returns
    ///
    /// Returns a `HashMap<String, String>` containing the initial key-value pairs for the specified
    /// storage. If no initial data is available or relevant, an empty `HashMap` may be returned.
    ///
    /// # Examples
    ///
    /// Implementors will override this method to provide specific logic for data initialization:
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use hailstorm::simulation::rune::extension::storage::initializer::StorageInitializerRegistry;
    ///
    /// struct MyInitializer;
    ///
    /// impl StorageInitializerRegistry for MyInitializer {
    ///     fn initial_values_for(&self, name: &str, bot_id: u32) -> HashMap<String, String> {
    ///         [(String::from("foo"), String::from("bar"))]
    ///             .into_iter()
    ///             .collect()
    ///     }
    /// }
    /// ```
    fn initial_values_for(&self, name: &str, bot_id: u32) -> HashMap<String, String>;
}
