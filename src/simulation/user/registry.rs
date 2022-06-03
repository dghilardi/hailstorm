use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use rune::{Context, Diagnostics, Hash, InstFnNameHash, Source, Sources, Unit, Value, Vm};
use rune::compile::{Component, Item};
use rune::runtime::debug::DebugArgs;
use rune::runtime::RuntimeContext;
use crate::simulation::rune::user_mod;
use crate::simulation::rune::user_mod::UserBehaviour;
use crate::simulation::user::error::UserError;

#[derive(Debug)]
pub struct UserRegistry {
    user_types: HashMap<String, UserBehaviour>,
    runtime: Arc<RuntimeContext>,
    unit: Arc<Unit>,
}

#[derive(Debug)]
pub struct FunSignature {
    hash: Hash,
    path: Item,
    args: DebugArgs,
}

impl UserRegistry {
    pub fn new(script: &str) -> Result<Self, UserError> {
        let mut context = Context::with_default_modules().unwrap();
        context.install(&rune_modules::http::module(true)?)?;
        context.install(&rune_modules::json::module(true)?)?;
        context.install(&user_mod::module()?)?;
        let runtime = Arc::new(context.runtime());

        let mut diagnostics = Diagnostics::new();

        let mut sources = Sources::new();
        sources.insert(Source::new("script", script));

        let unit = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build()
            .map(Arc::new)
            .map_err(|_| UserError::BuildError(format!("diagnostics: {diagnostics:?}")))?;

        let mut vm = Vm::new(runtime.clone(), unit.clone());

        let user_types = unit.debug_info()
            .ok_or(UserError::NoDebugInfo)?
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
            .filter(|(_k, v)|
                v.iter().any(|fun| fun.path.clone().pop().unwrap().eq(&Component::Str("new".into_name()))) &&
                    v.iter().any(|fun| fun.path.clone().pop().unwrap().eq(&Component::Str("register_user".into_name())))
            )
            .flat_map(|(k, _sig)| {
                let mut user = UserBehaviour::default();
                let register_out = vm.call(&[k.clone(), String::from("register_user")], (&mut user, ));

                match register_out {
                    Ok(_) => Some((k, user)),
                    Err(err) => {
                        log::error!("Error: {err}");
                        None
                    }
                }
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            user_types,
            runtime,
            unit,
        })
    }

    pub fn build_user(&self, model: &str) -> Option<User> {
        self.user_types
            .get(model)
            .map(|b| {
                let mut vm = rune::Vm::new(self.runtime.clone(), self.unit.clone());
                let instance = vm.call([model, "new"], ()).expect("Error construction");
                User {
                    behaviour: b.clone(),
                    instance,
                    vm,
                }
            })
    }
}

pub struct User {
    behaviour: UserBehaviour,
    instance: Value,
    vm: rune::Vm,
}

impl User {
    pub fn get_interval(&self) -> Duration {
        self.behaviour.get_interval()
    }

    pub async fn run_random_action(&mut self) {
        let action_hash = self.behaviour.random_action();
        let action_out = self.vm.async_call(action_hash, (&self.instance,)).await;
        if let Err(e) = action_out {
            log::error!("Error executing action - {e}");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_registry_creation() {
        let registry = UserRegistry::new(r###"
        struct Demo { id }
        impl Demo {
            pub fn register_user(user) {
                user.register_action(10.0, Self::do_something);
                user.register_action(10.0, Self::do_something_else);
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

        assert!(registry.user_types.contains_key("Demo"));

        let user = registry.user_types.get("Demo").unwrap();

        let mut vm = Vm::new(registry.runtime, registry.unit);
        let instance = vm.call(&["Demo", "new"], ()).unwrap();
        vm.call(user.random_action(), (&instance, )).expect("Error running action");
    }
}