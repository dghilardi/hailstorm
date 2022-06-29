use rune::Any;
use crate::simulation::user_actor;

#[derive(Any, Debug)]
pub enum UserState {
    #[rune(constructor)] Initializing,
    #[rune(constructor)] Running,
    #[rune(constructor)] Stopping,
    #[rune(constructor)] Custom(#[rune(get)] u32),
}

impl From<UserState> for user_actor::UserState {
    fn from(state: UserState) -> Self {
        match state {
            UserState::Initializing => user_actor::UserState::Initializing,
            UserState::Running => user_actor::UserState::Running,
            UserState::Stopping => user_actor::UserState::Stopping,
            UserState::Custom(cst) => user_actor::UserState::Custom(cst),
        }
    }
}