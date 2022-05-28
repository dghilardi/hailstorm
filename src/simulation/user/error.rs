use rune::{BuildError, ContextError};

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("No debug info in unit")]
    NoDebugInfo,
    #[error("Rune initialization error")]
    RuneInitError(String),
    #[error("Build Error")]
    BuildError,
}

impl From<BuildError> for UserError {
    fn from(_: BuildError) -> Self {
        Self::BuildError
    }
}

impl From<ContextError> for UserError {
    fn from(ce: ContextError) -> Self {
        Self::RuneInitError(ce.to_string())
    }
}