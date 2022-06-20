use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct SequentialIdGenerator {
    last_generated_id: u32,
    released_ids: BTreeSet<u32>,
}

impl SequentialIdGenerator {
    pub fn next(&mut self) -> u32 {
        if self.released_ids.is_empty() {
            self.last_generated_id += 1;
            self.last_generated_id
        } else {
            let released_id = *self.released_ids.iter().next().expect("No released id available");
            self.released_ids.take(&released_id).expect("Error taking released id")
        }
    }

    pub fn release_id(&mut self, id: u32) {
        if self.last_generated_id == id {
            self.last_generated_id = id - 1;
            for idx in self.released_ids.iter().rev() {
                if *idx == self.last_generated_id {
                    self.last_generated_id = idx - 1;
                } else {
                    break;
                }
            }
            self.released_ids.retain(|idx| *idx < self.last_generated_id);
        } else {
            self.released_ids.insert(id);
        }
    }
}