use std::collections::BTreeSet;

#[derive(Debug)]
pub struct SequentialIdGenerator {
    family_id: u32,
    family_bits: usize,
    last_generated_id: u32,
    released_ids: BTreeSet<u32>,
}

impl SequentialIdGenerator {
    pub fn new(
        family_id: u32,
        family_bits: usize,
    ) -> Self {
        Self {
            family_id,
            family_bits,
            last_generated_id: 0,
            released_ids: Default::default(),
        }
    }

    fn apply_mask(&self, num: u32) -> u32 {
        let mask = !0 >> self.family_bits;
        let family = (self.family_id << (32 - self.family_bits)) & !mask;
        let id = num & mask;
        family | id
    }

    fn remove_mask(&self, num: u32) -> u32 {
        (!0 >> self.family_bits) & num
    }

    pub fn next(&mut self) -> u32 {
        let res = if self.released_ids.is_empty() {
            self.last_generated_id += 1;
            self.last_generated_id
        } else {
            let released_id = *self.released_ids.iter().next().expect("No released id available");
            self.released_ids.take(&released_id).expect("Error taking released id")
        };
        self.apply_mask(res)
    }

    pub fn release_id(&mut self, masked_id: u32) {
        let id = self.remove_mask(masked_id);
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