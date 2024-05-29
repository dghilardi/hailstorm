use crate::simulation::actor::bot;
use rune::Any;

#[derive(Any, Debug)]
pub enum BotState {
    #[rune(constructor)]
    Initializing,
    #[rune(constructor)]
    Running,
    #[rune(constructor)]
    Stopping,
    #[rune(constructor)]
    Custom(#[rune(get)] u32),
}

impl From<BotState> for bot::BotState {
    fn from(state: BotState) -> Self {
        match state {
            BotState::Initializing => bot::BotState::Initializing,
            BotState::Running => bot::BotState::Running,
            BotState::Stopping => bot::BotState::Stopping,
            BotState::Custom(cst) => bot::BotState::Custom(cst),
        }
    }
}
