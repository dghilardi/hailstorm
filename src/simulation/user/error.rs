use rune::BuildError;

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("No debug info in unit")]
    NoDebugInfo,
    #[error("Build Error")]
    BuildError,
}

impl From<BuildError> for UserError {
    fn from(_: BuildError) -> Self {
        Self::BuildError
    }
}