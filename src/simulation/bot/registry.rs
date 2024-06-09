use crate::agent::metrics::manager::message::{StartActionTimer, StopActionTimer};
use crate::simulation::bot::error::{BotError, LoadScriptError};
use crate::simulation::bot::model_factory::BotModelFactory;
use crate::simulation::bot::params::BotParams;
use crate::simulation::bot::scripted::ScriptedBot;
use crate::simulation::compound_id::CompoundId;
use crate::simulation::rune::extension::bot::BotBehaviour;
use crate::simulation::rune::extension::{bot, metrics};
use actix::{Actor, Addr, Handler};
use rune::compile::{Component, ItemBuf};
use rune::runtime::debug::DebugArgs;
use rune::runtime::RuntimeContext;
use rune::{Context, Diagnostics, Hash, Source, Sources, Unit, Vm};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug)]
/// Manages the registration and instantiation of bots within a simulation.
///
/// The `BotRegistry` is responsible for loading bot scripts written in Rune, registering the
/// discovered bot behaviors, and providing factories for creating instances of these bots.
pub struct BotRegistry {
    bot_types: HashMap<String, BotBehaviour>,
    context: Context,
    runtime: Arc<RuntimeContext>,
    unit: Arc<Unit>,
}

#[derive(Debug)]
struct FunSignature {
    hash: Hash,
    path: ItemBuf,
    args: DebugArgs,
}

impl BotRegistry {
    /// Creates a new instance of `BotRegistry`.
    ///
    /// Initializes the Rune context and installs necessary modules for bot and metrics management.
    ///
    /// # Parameters
    ///
    /// - `context`: The Rune context to be used for script execution.
    /// - `metrics_mgr_addr`: Address of the metrics manager actor for integrating metrics collection.
    ///
    /// # Returns
    ///
    /// Returns a `Result<Self, BotError>`, which is `Ok` with the new `BotRegistry` instance or
    /// an `Err` with a `BotError` if initialization fails.
    pub fn new<A>(mut context: Context, metrics_mgr_addr: Addr<A>) -> Result<Self, BotError>
    where
        A: Actor<Context = actix::Context<A>>
            + Handler<StartActionTimer>
            + Handler<StopActionTimer>,
    {
        context.install(&bot::module()?)?;
        context.install(&metrics::module(metrics_mgr_addr)?)?;
        let runtime = Arc::new(context.runtime());

        Ok(Self {
            bot_types: Default::default(),
            context,
            runtime,
            unit: Arc::new(Default::default()),
        })
    }

    /// Loads and compiles a Rune script, registering bot behaviors defined within.
    ///
    /// # Parameters
    ///
    /// - `script`: The source code of the Rune script to load.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the script is successfully loaded and compiled, or an `Err` with
    /// a `LoadScriptError` detailing any issues encountered during the process.
    pub fn load_script(&mut self, script: &str) -> Result<(), LoadScriptError> {
        let mut diagnostics = Diagnostics::new();

        let mut sources = Sources::new();
        sources.insert(Source::new("script", script));

        let unit = rune::prepare(&mut sources)
            .with_context(&self.context)
            .with_diagnostics(&mut diagnostics)
            .build()
            .map(Arc::new)
            .map_err(|_| LoadScriptError::InvalidScript(format!("diagnostics: {diagnostics:?}")))?;

        let mut vm = Vm::new(self.runtime.clone(), unit.clone());

        let bot_types = unit.debug_info()
            .ok_or(LoadScriptError::NoDebugInfo)?
            .functions
            .iter()
            .fold(HashMap::new(), |mut acc, (hash, dbg)| {
                let mut path = dbg.path.clone();
                let _last = path.pop().expect("Empty path");

                acc.entry(path.to_string())
                    .or_insert_with(Vec::new)
                    .push(FunSignature {
                        hash: *hash,
                        path: dbg.path.clone(),
                        args: match &dbg.args {
                            DebugArgs::EmptyArgs => DebugArgs::EmptyArgs,
                            DebugArgs::TupleArgs(ta) => DebugArgs::TupleArgs(*ta),
                            DebugArgs::Named(named) => DebugArgs::Named(named.clone())
                        },
                    });
                acc
            }).into_iter()
            .filter(|(k, v)| {
                let has_new_constructor = v.iter().any(|fun| fun.path.clone().pop().unwrap().eq(&Component::Str("new".into())));
                let has_register_bot_fn = v.iter().any(|fun| fun.path.clone().pop().unwrap().eq(&Component::Str("register_bot".into())));
                let is_a_bot = has_new_constructor && has_register_bot_fn;

                if !is_a_bot {
                    log::debug!("Skipping {k} it is not a bot - has new: {has_new_constructor}, has register: {has_register_bot_fn}");
                }
                is_a_bot
            })
            .flat_map(|(k, _sig)| {
                let mut bot = BotBehaviour::default();
                let register_out = vm.call(&[k.clone(), String::from("register_bot")], (&mut bot, ));

                match register_out {
                    Ok(_) => {
                        log::debug!("Registering {k}");
                        Some((k, bot))
                    }
                    Err(err) => {
                        log::error!("Error: {err}");
                        None
                    }
                }
            })
            .collect::<HashMap<_, _>>();

        self.unit = unit;
        self.bot_types = bot_types;

        Ok(())
    }

    /// Resets the script state of the registry.
    ///
    /// Clears any previously loaded scripts and registered bot types, effectively resetting
    /// the registry to its initial state.
    pub fn reset_script(&mut self) {
        self.bot_types = Default::default();
        self.unit = Arc::new(Default::default());
    }

    pub(crate) fn has_registered_models(&self) -> bool {
        !self.bot_types.is_empty()
    }

    /// Attempts to create a new bot instance based on the specified model and compound ID.
    ///
    /// # Parameters
    ///
    /// - `compound_id`: The unique identifier for the bot instance.
    /// - `model`: The name of the bot model to instantiate.
    ///
    /// # Returns
    ///
    /// Returns an `Option<ScriptedBot>` which is `Some` with the new bot instance if successful,
    /// or `None` if the model could not be instantiated.
    pub fn build_bot(&self, compound_id: CompoundId<u32>, model: &str) -> Option<ScriptedBot> {
        self.bot_types.get(model).and_then(|b| {
            let mut vm = rune::Vm::new(self.runtime.clone(), self.unit.clone());
            let params = BotParams {
                bot_id: compound_id.bot_id(),
                internal_id: compound_id.internal_id(),
                global_id: compound_id.global_id(),
            };
            let bot_creation_result = vm.call([model, "new"], (params,));
            match bot_creation_result {
                Ok(instance) => Some(ScriptedBot::new(b.clone(), instance, vm)),
                Err(e) => {
                    log::error!("Error during '{model}' instantiation - {e}");
                    None
                }
            }
        })
    }

    /// Counts the number of bot models registered in the registry.
    ///
    /// # Returns
    ///
    /// Returns the count of registered bot models.
    pub(crate) fn count_bot_models(&self) -> usize {
        self.bot_types.len()
    }

    /// Retrieves a list of the names of all registered bot models.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<&String>` containing the names of all registered bot models.
    pub(crate) fn model_names(&self) -> Vec<&String> {
        self.bot_types.keys().collect()
    }

    /// Creates a factory for producing bots of the specified model.
    ///
    /// # Parameters
    ///
    /// - `model`: The name of the bot model for which to create a factory.
    ///
    /// # Returns
    ///
    /// Returns an `Option<BotModelFactory>` which is `Some` with the new factory if the model exists,
    /// or `None` if there is no such model registered.
    pub(crate) fn build_factory(&self, model: &str) -> Option<BotModelFactory> {
        self.bot_types.get(model).map(|b| BotModelFactory {
            model: model.to_string(),
            behaviour: b.clone(),
            runtime: self.runtime.clone(),
            unit: self.unit.clone(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::agent::metrics::manager::actor::MetricsManagerActor;

    const MINIMAL_VALID_SCRIPT: &str = r#"
            struct Util {}
            struct Demo { id }

            impl Demo {
              pub fn register_bot(bot) {}
              pub fn new(par) {
                Self { id: 10 }
              }
            }
    "#;

    const BOT_ERR_SCRIPT: &str = r#"
            struct Util {}
            struct Demo { id }

            impl Demo {
              pub fn register_bot(bot) {}
              pub fn new(par) {
                Self { id: 10 / 0 }
              }
            }
    "#;

    const BOT_REGISTER_ERR_SCRIPT: &str = r#"
            struct Util {}
            struct Demo { id }

            impl Demo {
              pub fn register_bot(bot) { 0 / 0 }
              pub fn new(par) {
                Self { id: 10 }
              }
            }
    "#;
    const MINIMAL_INVALID_SCRIPT: &str = r#"
            struct Demo { id }
            impl X {
              pub fn register_bot(bot) {}
              pub fn new(par) {
                Self { id: 10 }
              }
            }
    "#;

    #[actix::test]
    async fn test_new_registry_creation() {
        let metrics_addr = MetricsManagerActor::start_default();
        let mut registry =
            BotRegistry::new(Context::with_default_modules().unwrap(), metrics_addr).unwrap();
        registry
            .load_script(
                r###"
        use hailstorm::bot::ActionTrigger;

        struct Demo { id }
        impl Demo {
            pub fn register_bot(bot) {
                bot.register_action(ActionTrigger::alive(10.0), Self::do_something);
                bot.register_action(ActionTrigger::alive(10.0), Self::do_something_else);
          }
          pub fn new() {
            Self { id: 10 }
          }
          pub async fn do_something(self) {
              dbg(self)
          }
          pub async fn do_something_else(self) {
              println("something else")
          }
        }
        "###,
            )
            .expect("Error building registry");

        assert!(registry.bot_types.contains_key("Demo"));

        let bot = registry.bot_types.get("Demo").unwrap();

        let mut vm = Vm::new(registry.runtime, registry.unit);
        let instance = vm.call(&["Demo", "new"], ()).unwrap();
        vm.call(bot.random_action(), (&instance,))
            .expect("Error running action");
    }

    #[actix::test]
    async fn test_load_valid_script() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        assert!(bot_registry.load_script(MINIMAL_VALID_SCRIPT).is_ok());
    }

    #[actix::test]
    async fn test_load_valid_script_with_bot_registration_error() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();
        bot_registry.load_script(BOT_REGISTER_ERR_SCRIPT).unwrap();

        assert_eq!(0, bot_registry.count_bot_models());
    }

    #[actix::test]
    async fn test_load_invalid_script() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        assert!(matches!(
            bot_registry.load_script(MINIMAL_INVALID_SCRIPT),
            Err(LoadScriptError::InvalidScript(_))
        ));
    }

    #[actix::test]
    async fn test_reset_script() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        bot_registry.load_script(MINIMAL_VALID_SCRIPT).unwrap();
        assert!(!bot_registry.bot_types.is_empty());

        bot_registry.reset_script();
        assert!(bot_registry.bot_types.is_empty());
    }

    #[actix::test]
    async fn test_build_bot() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        bot_registry.load_script(MINIMAL_VALID_SCRIPT).unwrap();
        assert!(!bot_registry.bot_types.is_empty());

        let bot = bot_registry.build_bot(CompoundId::new(1, 2, 3), "Demo");
        assert!(bot.is_some());
    }

    #[actix::test]
    async fn test_build_bot_err() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        bot_registry.load_script(BOT_ERR_SCRIPT).unwrap();
        assert!(!bot_registry.bot_types.is_empty());

        let bot = bot_registry.build_bot(CompoundId::new(1, 2, 3), "Demo");
        assert!(bot.is_none());
    }

    #[actix::test]
    async fn test_count_bot_models() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        bot_registry.load_script(MINIMAL_VALID_SCRIPT).unwrap();
        assert!(!bot_registry.bot_types.is_empty());

        let count = bot_registry.count_bot_models();
        assert_eq!(1, count);
    }

    #[actix::test]
    async fn test_model_names() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        bot_registry.load_script(MINIMAL_VALID_SCRIPT).unwrap();
        assert!(!bot_registry.bot_types.is_empty());

        let names = bot_registry.model_names();

        assert_eq!(1, names.len());
        assert_eq!(Some("Demo"), names.first().map(|n| n.as_str()))
    }

    #[actix::test]
    async fn test_build_bot_factory() {
        let context = Context::with_default_modules().unwrap();
        let metrics_addr = MetricsManagerActor::start_default();

        let mut bot_registry = BotRegistry::new(context, metrics_addr).unwrap();

        bot_registry.load_script(MINIMAL_VALID_SCRIPT).unwrap();
        assert!(!bot_registry.bot_types.is_empty());

        let bot_factory = bot_registry.build_factory("Demo");
        assert!(bot_factory.is_some());
    }
}
