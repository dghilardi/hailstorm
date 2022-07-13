use std::collections::{btree_map, BTreeMap};
use crate::simulation::rune::types::value::OwnedValue;

pub struct OwnedObject {
    inner: BTreeMap<String, OwnedValue>,
}

impl std::iter::FromIterator<(String, OwnedValue)> for OwnedObject {
    fn from_iter<T: IntoIterator<Item = (String, OwnedValue)>>(src: T) -> Self {
        Self {
            inner: src.into_iter().collect(),
        }
    }
}

impl IntoIterator for OwnedObject {
    type Item = (String, OwnedValue);
    type IntoIter = btree_map::IntoIter<String, OwnedValue>;

    /// Creates a consuming iterator, that is, one that moves each key-value
    /// pair out of the object in arbitrary order. The object cannot be used
    /// after calling this.
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}