use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::Deserialize;
use super::StorageInitializerRegistry;

#[derive(Debug)]
pub struct CsvStorageInitializer {
    values: HashMap<u32, HashMap<String, String>>,
}

#[derive(Deserialize)]
struct CsvEntry {
    id: u32,
    #[serde(flatten)]
    values: HashMap<String, String>,
}

impl CsvStorageInitializer {
    pub fn new(dir: PathBuf, agent_id: u64) -> Result<Self, csv::Error> {
        let filename = format!("init-{agent_id}.csv");
        let values = csv::Reader::from_path(dir.join(Path::new(&filename)))?
            .deserialize()
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
            });
        Ok(Self {
            values,
        })
    }
}

impl StorageInitializerRegistry for CsvStorageInitializer {
    fn initial_values_for(&self, user_id: u32) -> HashMap<String, String> {
        self.values.get(&user_id)
            .cloned()
            .unwrap_or_default()
    }

    fn into_values(self) -> HashMap<u32, HashMap<String, String>> {
        self.values
    }
}