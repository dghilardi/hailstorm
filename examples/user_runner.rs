use std::fs;
use std::time::Duration;
use actix::{Actor, Context, Handler};
use clap::Parser;
use hailstorm::agent::metrics::manager_actor::MetricsManagerActor;
use hailstorm::simulation::rune::extension;
use hailstorm::simulation::rune::extension::env::EnvModuleConf;
use hailstorm::simulation::rune::extension::storage::initializer::empty::EmptyInitializer;
use hailstorm::simulation::rune::extension::storage::StorageModuleArgs;
use hailstorm::simulation::actor::simulation::BotStateChange;
use hailstorm::simulation::bot::registry::BotRegistry;
use hailstorm::simulation::actor::bot::BotActor;

struct StateChangeLoggerActor;

impl Actor for StateChangeLoggerActor {
    type Context = Context<Self>;
}

impl Handler<BotStateChange> for StateChangeLoggerActor {
    type Result = ();

    fn handle(&mut self, msg: BotStateChange, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("bot state changed: {:?}", msg.state)
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Bot model to load
    #[clap(short, long, value_parser)]
    model: String,

    /// Path of the script to load
    #[clap(short, long, value_parser)]
    script: String,
}

#[actix::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();
    let args = Args::parse();

    let metrics_actor_addr = MetricsManagerActor::start_default();

    let mut rune_ctx = rune::Context::with_default_modules().expect("Error loading default rune modules");
    rune_ctx.install(&extension::storage::module(StorageModuleArgs { initializer: EmptyInitializer }).expect("Error initializing storage extension module")).expect("Error loading storage extension module");
    rune_ctx.install(&extension::env::module(EnvModuleConf { prefix: Some(String::from("hsa")) }).expect("Error initializing env extension module")).expect("Error loading env extension module");

    let mut registry = BotRegistry::new(rune_ctx, metrics_actor_addr).expect("Error in registry construction");
    registry.load_script(&fs::read_to_string(&args.script).expect("Error reading script file")).expect("Error loading script");

    let state_change_logger_actor = StateChangeLoggerActor::create(|_| StateChangeLoggerActor);

    let bot = registry.build_bot(1, &args.model).unwrap_or_else(|| panic!("No bot found with model {}", args.model));
    let actor = BotActor::new(1, state_change_logger_actor, bot);
    let _addr = actor.start();

    actix::clock::sleep(Duration::from_secs(60)).await;
}