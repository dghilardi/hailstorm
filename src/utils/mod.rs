//! Utility modules for actix actor extensions and data encoding.
//!
//! - [`actix`] — Extensions for the actix actor framework including weak-reference intervals
//!   and synchronized periodic tasks.
//! - [`varint`] — Variable-length integer encoding/decoding for compact ID storage.

pub mod actix;
pub(crate) mod varint;
