use rune::ContextError;

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("Rune initialization error")]
    RuneInitError(String),
    #[error("Build Error - {0}")]
    BuildError(String),
}

impl From<ContextError> for UserError {
    fn from(ce: ContextError) -> Self {
        Self::RuneInitError(ce.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadScriptError {
    #[error("No debug info in unit")]
    NoDebugInfo,
    #[error("Invalid script - {0}")]
    InvalidScript(String),
}