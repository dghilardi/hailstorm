use actix::{Actor, Addr, Context, Handler};
use rune::{ContextError, Module};
use crate::agent::metrics::manager_actor::{StartActionTimer, StopActionTimer};
use crate::simulation::rune::extension::metrics::performance::PerformanceRegistry;

pub fn module<A>(metrics_mgr_addr: Addr<A>) -> Result<Module, ContextError>
where A: Actor<Context=Context<A>>
    + Handler<StartActionTimer>
    + Handler<StopActionTimer>
{
    let mut module = Module::with_crate_item("hailstorm", &["metrics"]);

    module.ty::<PerformanceRegistry>()?;
    module.function(&["PerformanceRegistry", "new"], move || PerformanceRegistry::new(metrics_mgr_addr.clone()))?;
    module.async_inst_fn("observe", PerformanceRegistry::observe)?;

    Ok(module)
}