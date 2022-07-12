use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use serde::Deserialize;
use super::StorageInitializerRegistry;

#[derive(Debug)]
struct SliceInit {
    values: HashMap<u32, HashMap<String, String>>,
}

#[derive(Debug)]
pub struct CsvStorageInitializer {
    agent_id: u64,
    base_path: PathBuf,
    slices: Arc<Mutex<RefCell<HashMap<String, SliceInit>>>>,
}

#[derive(Deserialize)]
struct CsvEntry {
    id: u32,
    #[serde(flatten)]
    values: HashMap<String, String>,
}

impl CsvStorageInitializer {
    pub fn new(dir: PathBuf, agent_id: u64) -> Result<Self, csv::Error> {
        Ok(Self {
            agent_id,
            base_path: dir,
            slices: Arc::new(Mutex::new(RefCell::new(Default::default()))),
        })
    }

    fn load_slice(&self, name: &str) -> SliceInit {
        let filename = format!("{name}-{}.csv", self.agent_id);
        let slice = if let Ok(mut values) = csv::Reader::from_path(self.base_path.join(Path::new(&filename))) {
            values.deserialize()
                .filter_map(|record: Result<CsvEntry, _>| {
                    match record {
                        Ok(entry) => Some(entry),
                        Err(err) => {
                            log::warn!("Error parsing csv entry - {err}");
                            None
                        }
                    }
                })
                .fold(HashMap::new(), |mut acc, entry| {
                    acc.insert(entry.id, entry.values);
                    acc
                })
        } else {
            Default::default()
        };
        SliceInit {
            values: slice,
        }
    }
}

impl StorageInitializerRegistry for CsvStorageInitializer {
    fn initial_values_for(&self, name: &str, user_id: u32) -> HashMap<String, String> {
        self.slices
            .lock()
            .expect("Error locking storage")
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| self.load_slice(name))
            .values.get(&user_id).cloned()
            .unwrap_or_default()
    }
}