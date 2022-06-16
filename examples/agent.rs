use std::collections::HashMap;
use std::net::ToSocketAddrs;
use config::{Config, ConfigError, Environment, File};
use rand::{RngCore, thread_rng};
use serde::Deserialize;
use hailstorm::agent::builder::AgentBuilder;

#[derive(Deserialize)]
pub struct HailstormAgentConfig {
    pub agent_id: Option<u64>,
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
            .unwrap_or_else(|| thread_rng().next_u64()),
        address: config.address
            .to_socket_addrs().unwrap().next().unwrap(),
        upstream: config.upstream
            .unwrap_or_default(),
        rune_context_builder: |_sim| rune::Context::with_default_modules()
            .expect("Error loading default rune modules"),
    }.launch().await;
}