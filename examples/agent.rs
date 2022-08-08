use std::collections::HashMap;
use std::net::ToSocketAddrs;
use config::{Config, ConfigError, Environment, File};
use rand::{RngCore, thread_rng};
use serde::Deserialize;
use hailstorm::agent::builder::AgentBuilder;
use hailstorm::simulation::rune::extension;
use hailstorm::simulation::rune::extension::env::EnvModuleConf;
use hailstorm::simulation::rune::extension::storage::initializer::empty::EmptyInitializer;
use hailstorm::simulation::rune::extension::storage::StorageModuleArgs;

#[derive(Deserialize)]
pub struct HailstormAgentConfig {
    pub agent_id: Option<u32>,
    pub address: String,
    pub upstream: Option<HashMap<String, String>>,
}

pub fn compose_config<'de, CFG: Deserialize<'de>>(external_path: &str) -> Result<CFG, ConfigError> {
    Config::builder()

        // Start off by local configuration file
        .add_source(File::with_name(external_path).required(false))

        // Add in settings from the environment (with a prefix of hs)
        .add_source(Environment::with_prefix("hs"))

        .build()?
        .try_deserialize()
}

#[actix::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();
    let config: HailstormAgentConfig = compose_config("config/hailstorm-agent")
        .expect("Error loading config");

    log::info!("Starting Hailstorm Agent...");

    AgentBuilder {
        agent_id: config.agent_id
            .unwrap_or_else(|| thread_rng().next_u32()),
        downstream: config.address
            .to_socket_addrs().unwrap().next().unwrap(),
        upstream: config.upstream
            .unwrap_or_default(),
        rune_context_builder: |_sim| {
            let mut ctx = rune::Context::with_default_modules().expect("Error loading default rune modules");
            ctx.install(&extension::storage::module(StorageModuleArgs { initializer: EmptyInitializer } ).expect("Error initializing storage extension module")).expect("Error loading storage extension module");
            ctx.install(&extension::env::module(EnvModuleConf { prefix: Some(String::from("hsa")) }).expect("Error initializing env extension module")).expect("Error loading env extension module");

            ctx
        },
    }.launch_grpc().await;
}

#[cfg(test)]
mod test {
    #[test]
    fn asd() {
        let transcod = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for (pos, value) in [(4, 0), (2,0), (0,0)] {
            for i in 0..64 {
                if (i >> pos) & 1 == value {
                    println!("[{pos}:{value}];{i};{}", transcod.chars().nth(i).unwrap()); // 15922389 - 1998186
                }
            }
        }
    }
}