use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use actix::{Actor, AsyncContext, Context, Handler, Message, MessageResponse};
use crate::simulation::error::SimulationError;

pub struct SimulationActor {
    agent_id: u64,
    start_ts: Option<SystemTime>,
    script: Option<String>,
    agents_count: u32,
    model_shapes: HashMap<String, Box<dyn Fn(f64) -> f64>>,
}

impl Actor for SimulationActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_millis(1000), |act, _ctx| act.tick());
    }
}

impl SimulationActor {
    pub fn new(agent_id: u64) -> Self {
        Self {
            agent_id,
            start_ts: None,
            script: None,
            agents_count: 1,
            model_shapes: Default::default()
        }
    }

    pub fn register_model(&mut self, model: String, shape: String) -> Result<(), SimulationError> {
        let expr: meval::Expr = shape.parse()?;
        let shape_fun = expr.bind("t")?;

        self.model_shapes.insert(model, Box::new(shape_fun));
        
        Ok(())
    }

    fn tick(&mut self) {
        let maybe_elapsed = self.start_ts
            .as_ref()
            .filter(|start_ts| **start_ts < SystemTime::now())
            .map(SystemTime::elapsed)
            .transpose().ok().flatten() // ignore errors
            .map(|dur| dur.as_secs_f64())
            ;

        if let Some(elapsed) = maybe_elapsed {
            for (model, shape) in self.model_shapes.iter() {
                let shape_val = shape(elapsed);
                let shift = (self.agent_id % 1000) as f64 / 1000f64;
                let count = (shape_val + shift).floor() as u64;
                log::info!("{model} -> {count}");
            }
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub enum SimulationCommand {
    LoadSimulation {
        model_shapes: HashMap<String, String>,
        script: String,
    },
    LaunchSimulation {
        start_ts: SystemTime,
    },
}

impl Handler<SimulationCommand> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: SimulationCommand, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            SimulationCommand::LoadSimulation { model_shapes, script } => {
                model_shapes
                    .into_iter()
                    .map(|(model, shape)| self.register_model(model, shape))
                    .collect::<Result<Vec<_>, _>>()
                    .err()
                    .map(|err| log::error!("Error registering simulation clients - {err}"));

                self.script = Some(script);
            }
            SimulationCommand::LaunchSimulation { start_ts } => {
                self.start_ts = Some(start_ts);
            }
        }
    }
}

pub enum SimulationState {
    Idle,
    Ready,
    Waiting,
    Running,
    Stopping,
}

pub struct ClientStats {

}

#[derive(MessageResponse)]
pub struct SimulationStats {
    pub stats: Vec<ClientStats>,
    pub timestamp: SystemTime,
    pub state: SimulationState,
    pub simulation_id: String,
}

#[derive(Message)]
#[rtype(result = "SimulationStats")]
pub struct FetchSimulationStats;

impl Handler<FetchSimulationStats> for SimulationActor {
    type Result = SimulationStats;

    fn handle(&mut self, msg: FetchSimulationStats, ctx: &mut Self::Context) -> Self::Result {
        let state = match (self.start_ts.as_ref(), self.script.as_ref()) {
            (_, None) => SimulationState::Idle,
            (None, Some(_)) => SimulationState::Ready,
            (Some(ts), Some(_)) if *ts < SystemTime::now() => SimulationState::Running,
            (Some(_), Some(_)) => SimulationState::Waiting,
        };

        SimulationStats {
            stats: vec![],
            timestamp: SystemTime::now(),
            state,
            simulation_id: "".to_string()
        }
    }
}