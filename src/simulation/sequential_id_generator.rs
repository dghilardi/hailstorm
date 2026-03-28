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
            let released_id = *self
                .released_ids
                .iter()
                .next()
                .expect("No released id available");
            self.released_ids
                .take(&released_id)
                .expect("Error taking released id")
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
            self.released_ids
                .retain(|idx| *idx < self.last_generated_id);
        } else {
            self.released_ids.insert(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_sequential_ids() {
        let mut gen = SequentialIdGenerator::default();
        assert_eq!(gen.next(), 1);
        assert_eq!(gen.next(), 2);
        assert_eq!(gen.next(), 3);
    }

    #[test]
    fn reuses_released_ids() {
        let mut gen = SequentialIdGenerator::default();
        let _a = gen.next(); // 1
        let b = gen.next(); // 2
        let _c = gen.next(); // 3

        gen.release_id(b); // release 2
        assert_eq!(gen.next(), 2); // should reuse 2
        assert_eq!(gen.next(), 4); // next fresh
    }

    #[test]
    fn release_last_id_shrinks_counter() {
        let mut gen = SequentialIdGenerator::default();
        gen.next(); // 1
        gen.next(); // 2
        let c = gen.next(); // 3

        gen.release_id(c); // release 3, should shrink counter
        assert_eq!(gen.next(), 3); // should get 3 again, not 4
    }

    #[test]
    fn release_consecutive_tail_ids() {
        let mut gen = SequentialIdGenerator::default();
        gen.next(); // 1
        gen.next(); // 2
        gen.next(); // 3

        gen.release_id(2); // release middle first
        gen.release_id(3); // release tail — should compact: last_generated = 1

        assert_eq!(gen.next(), 2); // should start from 2 again
    }

    #[test]
    fn interleaved_release_and_generate() {
        let mut gen = SequentialIdGenerator::default();
        let ids: Vec<u32> = (0..5).map(|_| gen.next()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);

        gen.release_id(2);
        gen.release_id(4);

        // Should reuse released IDs (BTreeSet order: smallest first)
        assert_eq!(gen.next(), 2);
        assert_eq!(gen.next(), 4);
        assert_eq!(gen.next(), 6);
    }
}
