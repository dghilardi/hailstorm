use rune::{ContextError, Module};

/// Configuration for the environment module.
///
/// Holds configuration options for the environment module, including an optional prefix that
/// can be applied to all environment variable names accessed through this module.
///
/// # Usage
///
/// The configuration can be directly instantiated with its default, which does not use a prefix,
/// or it can be built using the `with_prefix` method to specify a prefix.
///
/// # Examples
///
/// Instantiating without a prefix:
///
/// ```
/// use hailstorm::simulation::rune::extension::env::EnvModuleConf;
///
/// let cfg = EnvModuleConf::default();
/// ```
///
/// Instantiating with a prefix:
///
/// ```
/// use hailstorm::simulation::rune::extension::env::EnvModuleConf;
///
/// let cfg = EnvModuleConf::default().with_prefix("MYAPP_");
/// ```
#[derive(Default)]
pub struct EnvModuleConf {
    prefix: Option<String>,
}

impl EnvModuleConf {
    /// Sets the prefix to be used when accessing environment variables through the module.
    ///
    /// # Parameters
    ///
    /// - `prefix`: A string slice representing the prefix to prepend to environment variable names.
    ///
    /// # Returns
    ///
    /// Returns a new `EnvModuleConf` instance with the specified prefix set.
    ///
    /// # Examples
    ///
    /// ```
    /// use hailstorm::simulation::rune::extension::env::EnvModuleConf;
    ///
    /// let cfg = EnvModuleConf::default().with_prefix("MYAPP_");
    /// ```
    pub fn with_prefix(self, prefix: &str) -> Self {
        Self {
            prefix: Some(String::from(prefix)),
            ..self
        }
    }
}

fn read_env(maybe_prefix: Option<&String>, name: &str) -> Option<String> {
    if let Some(prefix) = maybe_prefix {
        std::env::var(format!("{}_{}", prefix, name)).ok()
    } else {
        std::env::var(name).ok()
    }
}

/// Constructs a Rune module for accessing environment variables.
///
/// Creates and configures a Rune `Module` that enables scripts to read environment variables,
/// optionally using a configured prefix for variable names.
///
/// # Parameters
///
/// - `cfg`: An `EnvModuleConf` struct specifying the module's configuration, including any
/// optional prefix to use when reading environment variables.
///
/// # Returns
///
/// Returns a `Result<Module, ContextError>` which is `Ok(Module)` if the module was successfully
/// created and configured, or an `Err(ContextError)` if there was an issue during module creation
/// or function registration.
///
/// # Examples
///
/// Creating a module with a prefix:
///
/// ```
/// use hailstorm::simulation::rune::extension::env::{EnvModuleConf, module};
///
/// let cfg = EnvModuleConf::default().with_prefix("MYAPP_");
/// let env_module = module(cfg).unwrap();
/// ```
pub fn module(cfg: EnvModuleConf) -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["env"]);

    module.function(&["read"], move |name: &str| {
        read_env(cfg.prefix.as_ref(), name)
    })?;

    Ok(module)
}

#[cfg(test)]
mod module_tests {
    use super::*;
    use rune::termcolor::{ColorChoice, StandardStream};
    use rune::{runtime::Vm, Context, Diagnostics, FromValue, Source, Sources};
    use std::sync::Arc;

    fn run_rune_script(script: &str, env_module: Module) -> Result<Option<String>, rune::Error> {
        let mut context = Context::with_default_modules()?;
        context
            .install(env_module)
            .expect("Error registering module");
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
        let output = Option::<String>::from_value(output)?;

        Ok(output)
    }

    #[test]
    fn module_exposes_read_function() -> Result<(), rune::Error> {
        std::env::set_var("RUNE_TEST_VAR", "789");
        let cfg = EnvModuleConf::default().with_prefix("RUNE");
        let env_module = module(cfg)?;

        let script = r#"
        pub fn main() {
            hailstorm::env::read("TEST_VAR")
        }
        "#;

        let result = run_rune_script(script, env_module)?;
        assert_eq!(result, Some("789".to_string()));
        Ok(())
    }
}
