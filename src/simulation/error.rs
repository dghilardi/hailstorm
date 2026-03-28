use meval::Error;
use thiserror::Error;

/// Errors that can occur during simulation setup.
#[derive(Debug, Error)]
pub enum SimulationError {
    /// The mathematical expression for a load shape function could not be parsed.
    #[error("Bad Shape function - {0}")]
    BadShape(String),
}

impl From<meval::Error> for SimulationError {
    fn from(e: Error) -> Self {
        Self::BadShape(e.to_string())
    }
}
