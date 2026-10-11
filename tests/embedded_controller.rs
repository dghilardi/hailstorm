use actix::Addr;
use hailstorm::agent::builder::AgentBuilder;
use hailstorm::controller::actor::ControllerActor;
use hailstorm::controller::embedded::{EmbeddedUpstream, StartEmbeddedController};
use hailstorm::controller::message::{
    GetSimulationStatus, LoadSimulation, SimulationOutcome, SimulationStatus, StartSimulation,
    StopSimulation,
};
use hailstorm::controller::model::simulation::{BotDef, SimulationDef};
use hailstorm::grpc::{AgentSimulationState, AgentUpdate};
use hailstorm::MultiAgentUpdateMessage;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const SCRIPT: &str = r#"
    struct Bot;
    impl Bot {
        pub fn new(args) { Self }
        pub fn register_bot(bot) {
            bot.register_action(hailstorm::bot::alive(1.0), Self::action);
        }
        pub fn action(self) {}
    }
"#;

fn simulation(script: &str) -> SimulationDef {
    SimulationDef::default()
        .bots(vec![BotDef::default().model("Bot").shape("1")])
        .script(script.to_string())
}

async fn wait_for(
    controller: &Addr<ControllerActor>,
    outcome: SimulationOutcome,
) -> SimulationStatus {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let status = controller.send(GetSimulationStatus).await.unwrap();
            if status.outcome() == outcome {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {outcome:?}"))
}

#[actix::test]
async fn an_embedded_controller_drives_the_local_agent() {
    let runtime = AgentBuilder::default()
        .agent_id(7)
        .upstream([(String::from("embedded"), ())].into_iter().collect())
        .rune_context_builder(|_| rune::Context::with_default_modules().unwrap())
        .launch::<EmbeddedUpstream>();
    let controller = runtime.clients_ref()[0]
        .send(StartEmbeddedController)
        .await
        .unwrap();

    let invalid = controller
        .send(LoadSimulation::new(simulation("struct Bot;")))
        .await
        .unwrap()
        .unwrap();
    let status = wait_for(&controller, SimulationOutcome::Failed).await;
    assert_eq!(status.agents[0].simulation_id, invalid);
    assert!(status.agents[0].simulation_error.is_some());

    let valid = controller
        .send(LoadSimulation::new(simulation(SCRIPT)))
        .await
        .unwrap()
        .unwrap();
    let status = wait_for(&controller, SimulationOutcome::Ready).await;
    assert_eq!(status.agents[0].agent_id, 7);
    assert_eq!(status.agents[0].simulation_id, valid);

    controller
        .send(StartSimulation::at(SystemTime::now()))
        .await
        .unwrap()
        .unwrap();
    wait_for(&controller, SimulationOutcome::Running).await;

    controller.send(StopSimulation).await.unwrap();
    let status = wait_for(&controller, SimulationOutcome::Idle).await;
    assert_eq!(status.simulation_id, None);
}

#[actix::test]
async fn delayed_heartbeats_preserve_the_running_bot_session() {
    let created = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicUsize::new(0));
    let created_hook = created.clone();
    let stopped_hook = stopped.clone();
    let runtime = AgentBuilder::default()
        .agent_id(7)
        .upstream([(String::from("embedded"), ())].into_iter().collect())
        .rune_context_builder(move |_| {
            let mut context = rune::Context::with_default_modules().unwrap();
            let mut module = rune::Module::with_crate("test").unwrap();
            let counter = created_hook.clone();
            module
                .function("created", move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
                .build()
                .unwrap();
            let counter = stopped_hook.clone();
            module
                .function("stopped", move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
                .build()
                .unwrap();
            context.install(module).unwrap();
            context
        })
        .launch::<EmbeddedUpstream>();
    let controller = runtime.clients_ref()[0]
        .send(StartEmbeddedController)
        .await
        .unwrap();
    let script = r#"
        struct Bot;
        impl Bot {
            pub fn new(args) { test::created(); Self }
            pub fn register_bot(bot) {
                bot.register_action(hailstorm::bot::enter_state(hailstorm::bot::BotState::Stopping), Self::stop);
            }
            pub fn stop(self) { test::stopped(); }
        }
    "#;
    let id = controller
        .send(LoadSimulation::new(simulation(script)))
        .await
        .unwrap()
        .unwrap();
    wait_for(&controller, SimulationOutcome::Ready).await;
    controller
        .send(StartSimulation::at(SystemTime::now()))
        .await
        .unwrap()
        .unwrap();
    let running = wait_for(&controller, SimulationOutcome::Running).await;
    assert_eq!(created.load(Ordering::SeqCst), 1);
    let stale_timestamp = running.agents[0].last_update - Duration::from_millis(1);

    // A status captured before Launch can arrive afterwards. An older definition
    // can also be reported before the agent confirms its new one.
    for (simulation_id, state, timestamp) in [
        (id.as_str(), AgentSimulationState::Ready, SystemTime::now()),
        ("previous", AgentSimulationState::Idle, stale_timestamp),
        ("previous", AgentSimulationState::Idle, SystemTime::now()),
    ] {
        controller
            .send(MultiAgentUpdateMessage(vec![AgentUpdate {
                agent_id: 7,
                timestamp: Some(timestamp.into()),
                state: state as i32,
                simulation_id: simulation_id.to_string(),
                ..Default::default()
            }]))
            .await
            .unwrap();
    }
    // Observe several spawn ticks and at least one real heartbeat after recovery.
    for _ in 0..25 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            created.load(Ordering::SeqCst),
            1,
            "the bot must not be recreated"
        );
        assert_eq!(
            stopped.load(Ordering::SeqCst),
            0,
            "the session must remain active"
        );
    }
    let recovered = wait_for(&controller, SimulationOutcome::Running).await;
    assert_eq!(recovered.agents[0].simulation_id, id);
    controller.send(StopSimulation).await.unwrap();
    wait_for(&controller, SimulationOutcome::Idle).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while stopped.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(stopped.load(Ordering::SeqCst), 1);
}
