use rune::Any;

#[derive(Any)]
pub struct BotParams {
    #[rune(get)]
    pub bot_id: u64,
}