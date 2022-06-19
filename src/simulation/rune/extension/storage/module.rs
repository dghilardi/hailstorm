use rune::{ContextError, Module};
use crate::simulation::rune::extension::storage::registry::StorageRegistry;
use super::storage::UserStorage;

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["storage"]);

    let registry = StorageRegistry::new();
    module.function(&["get_user_storage"], move |user_id| registry.get_user_storage(user_id))?;

    module.ty::<UserStorage>()?;
    module.inst_fn("read", UserStorage::read)?;
    module.inst_fn("write", UserStorage::write)?;

    Ok(module)
}