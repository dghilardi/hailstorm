use super::StorageInitializerRegistry;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Represents a collection of values initialized from a CSV slice.
#[derive(Debug)]
struct SliceInit {
    values: HashMap<u32, HashMap<String, String>>,
}

/// Initializes storage with values loaded from CSV files for specific agents.
///
/// This struct is responsible for reading CSV files named according to a convention that includes
/// the agent ID and loading those values into a structured format for easy access and initialization
/// of storage components.
#[derive(Debug)]
pub struct CsvStorageInitializer {
    agent_id: u64,
    base_path: PathBuf,
    slices: Arc<Mutex<RefCell<HashMap<String, SliceInit>>>>,
}

/// Represents a single entry within a CSV file.
///
/// This struct is used during the deserialization of CSV records, mapping columns to fields directly
/// and capturing all other columns into a `HashMap` for flexible data representation.
///
/// # Fields
///
/// - `id`: A `u32` representing the unique identifier of the entry, typically corresponding to a bot ID.
/// - `values`: A `HashMap<String, String>` storing all other CSV columns as key-value pairs, allowing for
/// dynamic and flexible data structures.
#[derive(Deserialize)]
struct CsvEntry {
    id: u32,
    #[serde(flatten)]
    values: HashMap<String, String>,
}

impl CsvStorageInitializer {
    /// Creates a new `CsvStorageInitializer`.
    ///
    /// # Parameters
    ///
    /// - `dir`: The base directory where CSV files are located.
    /// - `agent_id`: The unique identifier for the agent.
    ///
    /// # Returns
    ///
    /// Returns an instance of `CsvStorageInitializer` configured to load data from the specified directory
    /// and for the specified agent.
    pub fn new(dir: PathBuf, agent_id: u64) -> Self {
        Self {
            agent_id,
            base_path: dir,
            slices: Arc::new(Mutex::new(RefCell::new(Default::default()))),
        }
    }

    fn load_slice(&self, name: &str) -> SliceInit {
        let filename = format!("{name}-{}.csv", self.agent_id);
        let slice = if let Ok(mut values) =
            csv::Reader::from_path(self.base_path.join(Path::new(&filename)))
        {
            values
                .deserialize()
                .filter_map(|record: Result<CsvEntry, _>| match record {
                    Ok(entry) => Some(entry),
                    Err(err) => {
                        log::warn!("Error parsing csv entry - {err}");
                        None
                    }
                })
                .fold(HashMap::new(), |mut acc, entry| {
                    acc.insert(entry.id, entry.values);
                    acc
                })
        } else {
            Default::default()
        };
        SliceInit { values: slice }
    }
}

impl StorageInitializerRegistry for CsvStorageInitializer {
    fn initial_values_for(&self, name: &str, bot_id: u32) -> HashMap<String, String> {
        self.slices
            .lock()
            .expect("Error locking storage")
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| self.load_slice(name))
            .values
            .get(&bot_id)
            .cloned()
            .unwrap_or_default()
    }
}
