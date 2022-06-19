use std::fs;
use std::time::Duration;
use actix::{Actor, Context, Handler};
use clap::Parser;
use hailstorm::simulation::rune::extension;
use hailstorm::simulation::rune::extension::env::EnvModuleConf;
use hailstorm::simulation::simulation_actor::UserStateChange;
use hailstorm::simulation::user::registry::UserRegistry;
use hailstorm::simulation::user_actor::UserActor;

struct StateChangeLoggerActor;

impl Actor for StateChangeLoggerActor {
    type Context = Context<Self>;
}

impl Handler<UserStateChange> for StateChangeLoggerActor {
    type Result = ();

    fn handle(&mut self, msg: UserStateChange, ctx: &mut Self::Context) -> Self::Result {
        log::info!("user state changed: {:?}", msg.state)
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// User model to load
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

    let mut rune_ctx = rune::Context::with_default_modules().expect("Error loading default rune modules");
    rune_ctx.install(&extension::storage::module().expect("Error initializing storage extension module")).expect("Error loading storage extension module");
    rune_ctx.install(&extension::env::module(EnvModuleConf { prefix: Some(String::from("hsa")) }).expect("Error initializing env extension module")).expect("Error loading env extension module");

    let mut registry = UserRegistry::new(rune_ctx).expect("Error in registry construction");
    registry.load_script(&fs::read_to_string(&args.script).expect("Error reading script file")).expect("Error loading script");

    let state_change_logger_actor = StateChangeLoggerActor::create(|_| StateChangeLoggerActor);

    let user = registry.build_user(1, &args.model).expect(&format!("No user found with model {}", args.model));
    let actor = UserActor::new(1, state_change_logger_actor, user);
    let _addr = actor.start();

    actix::clock::sleep(Duration::from_secs(60)).await;
}