use rune::Any;

#[derive(Any)]
pub struct UserParams {
    #[rune(get)]
    pub user_id: u32,
}