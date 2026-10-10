use crate::agent::metrics::manager::message::{StartActionTimer, StopActionTimer};
use crate::simulation::rune::extension::metrics::performance::PerformanceRegistry;
use actix::{Actor, Addr, Context, Handler};
use rune::{ContextError, Module};

pub fn module<A>(metrics_mgr_addr: Addr<A>) -> Result<Module, ContextError>
where
    A: Actor<Context = Context<A>> + Handler<StartActionTimer> + Handler<StopActionTimer>,
{
    let mut module = Module::with_crate_item("hailstorm", ["metrics"])?;

    module.ty::<PerformanceRegistry>()?;
    let associated_metrics_addr = metrics_mgr_addr.clone();
    module
        .function("new", move |model: String| {
            PerformanceRegistry::new(model, metrics_mgr_addr.clone())
        })
        .build()?;
    module
        .function("new", move |model: String| {
            PerformanceRegistry::new(model, associated_metrics_addr.clone())
        })
        .build_associated::<PerformanceRegistry>()?;
    module.function_meta(PerformanceRegistry::observe)?;

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::metrics::manager::actor::MetricsManagerActor;
    use std::sync::Arc;

    #[actix::test]
    async fn legacy_metrics_constructor_and_module_alias_are_available() {
        let mut context = rune::Context::with_default_modules().unwrap();
        context
            .install(module(MetricsManagerActor::start_default()).unwrap())
            .unwrap();
        let mut sources = rune::Sources::new();
        sources
            .insert(
                rune::Source::new(
                    "test",
                    r#"
            pub fn main() {
                let first = hailstorm::metrics::PerformanceRegistry::new("Bot");
                let second = hailstorm::metrics::new("Bot");
                ()
            }
        "#,
                )
                .unwrap(),
            )
            .unwrap();
        let unit = rune::prepare(&mut sources)
            .with_context(&context)
            .build()
            .unwrap();
        let mut vm = rune::Vm::new(Arc::new(context.runtime().unwrap()), Arc::new(unit));
        vm.call(["main"], ()).unwrap().into_unit().unwrap();
    }
}
