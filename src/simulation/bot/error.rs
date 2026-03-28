use rune::ContextError;

/// Errors that can occur during bot registry initialization.
#[derive(Debug, thiserror::Error)]
pub enum BotError {
    /// Failed to initialize the Rune VM context.
    #[error("Rune initialization error - {0}")]
    RuneInitError(String),
    /// Failed to build a bot instance.
    #[error("Build Error - {0}")]
    BuildError(String),
}

impl From<ContextError> for BotError {
    fn from(ce: ContextError) -> Self {
        Self::RuneInitError(ce.to_string())
    }
}

/// Errors that can occur when loading a Rune script.
#[derive(Debug, thiserror::Error)]
pub enum LoadScriptError {
    /// The compiled unit has no debug information — needed to discover bot types.
    #[error("No debug info in unit")]
    NoDebugInfo,
    /// The script failed to compile.
    #[error("Invalid script - {0}")]
    InvalidScript(String),
}
