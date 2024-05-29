use rune::{ContextError, Module};

pub struct EnvModuleConf {
    pub prefix: Option<String>,
}

fn read_env(maybe_prefix: Option<&String>, name: &str) -> Option<String> {
    if let Some(prefix) = maybe_prefix {
        std::env::var(format!("{}_{}", prefix, name)).ok()
    } else {
        std::env::var(name).ok()
    }
}

pub fn module(cfg: EnvModuleConf) -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("hailstorm", &["env"]);

    module.function(&["read"], move |name: &str| {
        read_env(cfg.prefix.as_ref(), name)
    })?;

    Ok(module)
}
