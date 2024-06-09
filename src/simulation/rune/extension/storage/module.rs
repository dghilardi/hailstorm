use super::bot_storage::BotStorage;
use crate::simulation::rune::extension::storage::initializer::StorageInitializerRegistry;
use crate::simulation::rune::extension::storage::registry::StorageRegistry;
use rune::{ContextError, Module};

/// Configuration arguments for creating a storage module.
///
/// Encapsulates the initializer used to populate the storage with initial values. The initializer
/// must implement the `StorageInitializerRegistry` trait, enabling various strategies for data
/// initialization.
///
/// # Type Parameters
///
/// - `Initializer`: The type of the storage initializer, which determines how storage will be
/// populated at the start of the simulation or application.
///
/// # Default Implementation
///
/// The default implementation sets the initializer to an empty tuple, indicating no initialization
/// logic. This can be overridden using the `with_initializer` method to specify a custom initializer.
pub struct StorageModuleArgs<Initializer> {
    initializer: Initializer,
}

impl Default for StorageModuleArgs<()> {
    fn default() -> Self {
        Self { initializer: () }
    }
}

impl<I> StorageModuleArgs<I> {
    /// Specifies an initializer for the storage module.
    ///
    /// # Parameters
    ///
    /// - `initializer`: An instance of the `Initializer` type, responsible for providing initial values
    /// to the storage based on predefined logic or data sources.
    ///
    /// # Returns
    ///
    /// Returns a new `StorageModuleArgs` instance with the specified initializer.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::rune::extension::storage::initializer::empty::EmptyInitializer;
    /// use hailstorm::simulation::rune::extension::storage::StorageModuleArgs;
    ///
    /// let args = StorageModuleArgs::default().with_initializer(EmptyInitializer);
    /// ```
    pub fn with_initializer<Initializer>(
        self,
        initializer: Initializer,
    ) -> StorageModuleArgs<Initializer> {
        StorageModuleArgs { initializer }
    }
}

/// Constructs a Rune module with storage capabilities.
///
/// Creates a Rune `Module` that includes functions for interacting with bot storage, such as reading
/// and writing data. The module integrates a storage registry, which manages storage instances for
/// bots, and exposes this functionality to Rune scripts.
///
/// # Type Parameters
///
/// - `Initializer`: The type of the storage initializer. Must implement `StorageInitializerRegistry`
/// and be thread-safe (`Send + Sync`). It's also required to have a static lifetime since it will be
/// used across the context of the Rune VM.
///
/// # Parameters
///
/// - `args`: A `StorageModuleArgs` instance containing the storage initializer.
///
/// # Returns
///
/// Returns a `Result<Module, ContextError>`, which is `Ok(Module)` if the module was successfully
/// created and configured, or an `Err(ContextError)` if there was an issue during module creation.
///
/// # Examples
///
/// ```
/// use hailstorm::simulation::rune::extension::storage::{module, StorageModuleArgs};
/// use hailstorm::simulation::rune::extension::storage::initializer::empty::EmptyInitializer;
///
/// let storage_module = module(StorageModuleArgs::default().with_initializer(EmptyInitializer))
///     .expect("Failed to create storage module");
/// ```
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
    use rune::termcolor::{ColorChoice, StandardStream};
    use rune::{Context, Diagnostics, FromValue, Source, Sources, Vm};
    use std::sync::Arc;

    #[test]
    fn initialize_with_empty_initializer() {
        module(StorageModuleArgs {
            initializer: EmptyInitializer,
        })
        .expect("Error initializing storage module with empty initializer");
    }

    fn run_rune_script<Out>(script: &str, module: Module) -> Result<Out, rune::Error>
    where
        Out: FromValue,
    {
        let mut context = Context::with_default_modules()?;
        context.install(module).expect("Error registering module");
        let runtime = Arc::new(context.runtime());

        let mut sources = Sources::new();
        sources.insert(Source::new("mem", script));

        let mut diagnostics = Diagnostics::new();

        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        let unit = result?;
        let mut vm = Vm::new(runtime, Arc::new(unit));

        let output = vm.execute(["main"], ())?.complete()?;
        let output = Out::from_value(output)?;

        Ok(output)
    }

    #[test]
    fn retrieve_data_from_initialized_storage() {
        let temp_dir = tempfile::tempdir().expect("Failed to create a temporary directory");
        let csv_file_path = temp_dir.path().join("bot_data-1.csv");

        let csv_content = "id,name,value\n13,bot_name,bot_value";
        std::fs::write(&csv_file_path, csv_content).expect("Failed to write CSV content");

        let initializer =
            CsvStorageInitializer::new(csv_file_path.parent().unwrap().to_path_buf(), 1);
        let storage_module = module(StorageModuleArgs::default().with_initializer(initializer))
            .expect("Error initializing storage module with CSV initializer");

        let script = r#"
        pub fn main() {
            let storage = hailstorm::storage::get_bot_storage("bot_data", 13);
            storage.read("name")
        }
        "#;

        let result = run_rune_script::<Option<String>>(script, storage_module)
            .expect("Error running rune script");

        assert_eq!(result, Some(String::from("bot_name")));
    }

    #[test]
    fn write_and_read_data_from_storage() {
        let storage_module =
            module(StorageModuleArgs::default().with_initializer(EmptyInitializer))
                .expect("Error initializing storage module with empty initializer");

        let script = r#"
        pub fn main() {
            let storage = hailstorm::storage::get_bot_storage("storage", 13);
            storage.write("hello", "world 13");

            let storage = hailstorm::storage::get_bot_storage("storage", 14);
            storage.write("hello", "world 14");

            let storage = hailstorm::storage::get_bot_storage("storage", 13);
            storage.read("hello")
        }
        "#;

        let result = run_rune_script::<Option<String>>(script, storage_module)
            .expect("Error running rune script");

        assert_eq!(result, Some(String::from("world 13")));
    }
}
