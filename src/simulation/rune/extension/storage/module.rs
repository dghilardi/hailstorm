use super::bot_storage::BotStorage;
use crate::simulation::rune::extension::storage::initializer::StorageInitializerRegistry;
use crate::simulation::rune::extension::storage::registry::StorageRegistry;
use rune::{ContextError, Module};

pub struct StorageModuleArgs<Initializer> {
    pub initializer: Initializer,
}

pub fn module<Initializer>(args: StorageModuleArgs<Initializer>) -> Result<Module, ContextError>
where
    Initializer: StorageInitializerRegistry + Send + Sync + 'static,
{
    let mut module = Module::with_crate_item("hailstorm", &["storage"]);

    let registry = StorageRegistry::new(args.initializer);
    module.function(&["get_bot_storage"], move |name, bot_id| {
        registry.get_bot_storage(name, bot_id)
    })?;

    module.ty::<BotStorage>()?;
    module.inst_fn("read", BotStorage::read)?;
    module.inst_fn("write", BotStorage::write)?;

    Ok(module)
}

#[cfg(test)]
mod test {
    use super::super::initializer::{csv::CsvStorageInitializer, empty::EmptyInitializer};
    use super::*;

    #[test]
    fn initialize_with_empty_initializer() {
        module(StorageModuleArgs {
            initializer: EmptyInitializer,
        })
        .expect("Error initializing storage module with empty initializer");
    }
}
