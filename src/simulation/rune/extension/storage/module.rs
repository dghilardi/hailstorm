use rune::{ContextError, Module};
use crate::simulation::rune::extension::storage::initializer::StorageInitializerRegistry;
use crate::simulation::rune::extension::storage::registry::StorageRegistry;
use super::user_storage::UserStorage;

pub struct StorageModuleArgs<Initializer> {
    pub initializer: Initializer,
}

pub fn module<Initializer>(args: StorageModuleArgs<Initializer>) -> Result<Module, ContextError>
where
    Initializer: StorageInitializerRegistry + Send + Sync + 'static
{
    let mut module = Module::with_crate_item("hailstorm", &["storage"]);

    let registry = StorageRegistry::new(args.initializer);
    module.function(&["get_user_storage"], move |name, user_id| registry.get_user_storage(name, user_id))?;

    module.ty::<UserStorage>()?;
    module.inst_fn("read", UserStorage::read)?;
    module.inst_fn("write", UserStorage::write)?;

    Ok(module)
}

#[cfg(test)]
mod test {
    use super::super::initializer::{csv::CsvStorageInitializer, empty::EmptyInitializer};
    use super::*;

    #[test]
    fn initialize_with_empty_initializer() {
        module(StorageModuleArgs { initializer: EmptyInitializer })
            .expect("Error initializing storage module with empty initializer");
    }
}