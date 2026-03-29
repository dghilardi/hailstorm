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
    module.function("new", move |model: String| {
        PerformanceRegistry::new(model, metrics_mgr_addr.clone())
    }).build()?;
    module.function_meta(PerformanceRegistry::observe)?;

    Ok(module)
}
