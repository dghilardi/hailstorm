use std::collections::HashMap;
use std::sync::Arc;
use actix::{Actor, Addr, Handler};
use rune::{Context, Diagnostics, Hash, Source, Sources, Unit, Vm};
use rune::compile::{Component, Item};
use rune::runtime::debug::DebugArgs;
use rune::runtime::RuntimeContext;
use crate::agent::metrics::manager_actor::{StartActionTimer, StopActionTimer};
use crate::simulation::rune::extension::{metrics, bot};
use crate::simulation::rune::extension::bot::BotBehaviour;
use crate::simulation::bot::error::{LoadScriptError, BotError};
use crate::simulation::bot::model_factory::BotModelFactory;
use crate::simulation::bot::params::BotParams;
use crate::simulation::bot::scripted::ScriptedBot;
use crate::simulation::compound_id::CompoundId;

#[derive(Debug)]
pub struct BotRegistry {
    bot_types: HashMap<String, BotBehaviour>,
    context: Context,
    runtime: Arc<RuntimeContext>,
    unit: Arc<Unit>,
}

#[derive(Debug)]
pub struct FunSignature {
    hash: Hash,
    path: Item,
    args: DebugArgs,
}

impl BotRegistry {
    pub fn new<A>(
        mut context: Context,
        metrics_mgr_addr: Addr<A>,
    ) -> Result<Self, BotError>
        where A: Actor<Context=actix::Context<A>>
        + Handler<StartActionTimer>
        + Handler<StopActionTimer>
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
                let has_new_constructor= v.iter().any(|fun| fun.path.clone().pop().unwrap().eq(&Component::Str("new".into())));
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
                    },
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

    pub fn reset_script(&mut self) {
        self.bot_types = Default::default();
        self.unit = Arc::new(Default::default());
    }

    pub fn has_registered_models(&self) -> bool {
        !self.bot_types.is_empty()
    }

    pub fn build_bot(&self, compound_id: CompoundId<u32>, model: &str) -> Option<ScriptedBot> {
        self.bot_types
            .get(model)
            .and_then(|b| {
                let mut vm = rune::Vm::new(self.runtime.clone(), self.unit.clone());
                let params = BotParams {
                    bot_id: compound_id.bot_id(),
                    internal_id: compound_id.internal_id(),
                    global_id: compound_id.global_id(),
                };
                let bot_creation_result = vm.call([model, "new"], (params, ));
                match bot_creation_result {
                    Ok(instance) => Some(ScriptedBot::new(b.clone(), instance, vm)),
                    Err(e) => {
                        log::error!("Error during '{model}' instantiation - {e}");
                        None
                    }
                }
            })
    }

    pub fn count_bot_models(&self) -> usize {
        self.bot_types.len()
    }

    pub fn model_names(&self) -> Vec<&String> {
        self.bot_types.keys().collect()
    }

    pub fn build_factory(&self, model: &str) -> Option<BotModelFactory> {
        self.bot_types.get(model).map(|b|
            BotModelFactory {
                model: model.to_string(),
                behaviour: b.clone(),
                runtime: self.runtime.clone(),
                unit: self.unit.clone(),
            }
        )
    }
}

#[cfg(test)]
mod test {
    use crate::agent::metrics::manager_actor::MetricsManagerActor;
    use super::*;

    #[actix::test]
    async fn test_new_registry_creation() {
        let metrics_addr = MetricsManagerActor::start_default();
        let mut registry = BotRegistry::new(Context::with_default_modules().unwrap(), metrics_addr).unwrap();
        registry.load_script(r###"
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
        "###).expect("Error building registry");

        assert!(registry.bot_types.contains_key("Demo"));

        let bot = registry.bot_types.get("Demo").unwrap();

        let mut vm = Vm::new(registry.runtime, registry.unit);
        let instance = vm.call(&["Demo", "new"], ()).unwrap();
        vm.call(bot.random_action(), (&instance, )).expect("Error running action");
    }
}