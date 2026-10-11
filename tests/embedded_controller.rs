use actix::Addr;
use hailstorm::agent::builder::AgentBuilder;
use hailstorm::controller::actor::ControllerActor;
use hailstorm::controller::embedded::{EmbeddedUpstream, StartEmbeddedController};
use hailstorm::controller::message::{
    GetSimulationStatus, LoadSimulation, SimulationOutcome, SimulationStatus, StartSimulation,
    StopSimulation,
};
use hailstorm::controller::model::simulation::{BotDef, SimulationDef};
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
