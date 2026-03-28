use rune::Any;

/// Parameters passed to a bot's `new()` constructor in Rune scripts.
///
/// These fields are accessible from Rune via the `#[rune(get)]` attribute,
/// allowing scripts to use the bot's identity for logging, storage, etc.
#[derive(Any)]
pub struct BotParams {
    /// Sequential bot ID within this model (unique per agent).
    #[rune(get)]
    pub bot_id: u32,
    /// Compound ID encoding model and bot IDs (unique per agent).
    #[rune(get)]
    pub internal_id: u64,
    /// Compound ID encoding agent, model, and bot IDs (globally unique).
    #[rune(get)]
    pub global_id: u64,
}
