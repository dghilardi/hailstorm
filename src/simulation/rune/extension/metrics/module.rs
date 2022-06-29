use rune::{ContextError, Module};
use crate::simulation::rune::extension::metrics::performance::PerformanceRegistry;

pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["metrics"]);

    module.ty::<PerformanceRegistry>()?;
    module.function(&["PerformanceRegistry", "new"], PerformanceRegistry::new)?;
    module.async_inst_fn("observe", PerformanceRegistry::observe)?;

    Ok(module)
}