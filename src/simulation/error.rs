use meval::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("Bad Shape function - {0}")]
    BadShape(String),
}

impl From<meval::Error> for SimulationError {
    fn from(e: Error) -> Self {
        Self::BadShape(e.to_string())
    }
}
