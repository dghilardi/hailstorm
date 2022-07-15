use std::vec;
use crate::simulation::rune::types::value::OwnedValue;

pub struct OwnedVec {
    inner: Vec<OwnedValue>,
}

impl std::iter::FromIterator<OwnedValue> for OwnedVec {
    fn from_iter<T: IntoIterator<Item = OwnedValue>>(src: T) -> Self {
        Self {
            inner: src.into_iter().collect(),
        }
    }
}

impl IntoIterator for OwnedVec {
    type Item = OwnedValue;
    type IntoIter = vec::IntoIter<OwnedValue>;

    /// Creates a consuming iterator, that is, one that moves each key-value
    /// pair out of the object in arbitrary order. The object cannot be used
    /// after calling this.
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}