use rune::Any;

#[derive(Any)]
pub struct BotParams {
    #[rune(get)]
    pub bot_id: u32,
    #[rune(get)]
    pub internal_id: u64,
    #[rune(get)]
    pub global_id: u64,
}