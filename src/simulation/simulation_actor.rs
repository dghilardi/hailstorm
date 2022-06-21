use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use actix::{Actor, AsyncContext, Context, Handler, Message, MessageResponse};
use crate::simulation::compound_id::U32Mask;
use crate::simulation::error::SimulationError;
use crate::simulation::simulation_user_model::SimulationUserModel;
use crate::simulation::user::registry::UserRegistry;
use crate::simulation::user_actor::UserState;

pub struct SimulationActor {
    agent_id: u64,
    start_ts: Option<SystemTime>,
    user_registry: UserRegistry,
    agents_count: u32,
    model_shapes: HashMap<String, Box<dyn Fn(f64) -> f64>>,
    sim_users: HashMap<String, SimulationUserModel>,
}

impl Actor for SimulationActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_millis(1000), |act, ctx| act.tick(ctx));
    }
}

impl SimulationActor {
    pub fn new(agent_id: u64, user_registry: UserRegistry) -> Self {
        Self {
            agent_id,
            start_ts: None,
            user_registry,
            agents_count: 1,
            model_shapes: Default::default(),
            sim_users: Default::default(),
        }
    }

    pub fn register_model(&mut self, model: String, shape: String) -> Result<(), SimulationError> {
        let expr: meval::Expr = shape.parse()?;
        let shape_fun = expr.bind("t")?;

        self.model_shapes.insert(model, Box::new(shape_fun));

        Ok(())
    }

    fn tick(&mut self, ctx: &mut Context<Self>) {
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
                let count = ((shape_val / self.agents_count as f64) + shift).floor() as usize;

                let model_users = if let Some(mu) = self.sim_users.get_mut(model) {
                    mu
                } else {
                    log::warn!("No simulation-user-model defined for model {model}");
                    break;
                };

                let running_count = model_users.count_active();

                model_users.retain(|_id, u| u.is_connected());

                match count.cmp(&running_count) {
                    Ordering::Less => {
                        model_users.users_mut()
                            .filter(|u| u.state() != UserState::Stopping)
                            .take(running_count - count)
                            .for_each(|u| u.stop_user());
                    }
                    Ordering::Equal => {
                        // running number is as expected
                    }
                    Ordering::Greater => {
                        for _idx in 0..(count - running_count) {
                            model_users.spawn_user(ctx.address());
                        }
                    }
                }
            }
        } else {
            self.sim_users.iter_mut()
                .flat_map(|(_m, u)| u.users_mut())
                .filter(|u| u.state() != UserState::Stopping)
                .for_each(|u| u.stop_user());
        }
    }
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct UserStateChange {
    pub user_id: u32,
    pub state: UserState,
}

impl Handler<UserStateChange> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: UserStateChange, _ctx: &mut Self::Context) -> Self::Result {
        let model_entry = self.sim_users.iter_mut()
            .find(|(_m, u)| u.contains_id(msg.user_id));

        if matches!(msg.state, UserState::Stopped) {
            if let Some((_m, u)) = model_entry {
                u.remove_user(msg.user_id);
            }
        } else {
            let maybe_simulation_user = model_entry
                .and_then(|(_m, u)| u.get_user_mut(msg.user_id));

            if let Some(u) = maybe_simulation_user {
                u.state = msg.state;
            }
        }
    }
}

pub enum SimulationCommand {
    LoadSimulation {
        model_shapes: HashMap<String, String>,
        script: String,
    },
    LaunchSimulation {
        start_ts: SystemTime,
    },
    UpdateAgentsCount {
        count: u32
    },
    StopSimulation {
        reset: bool,
    },
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SimulationCommandLst {
    pub commands: Vec<SimulationCommand>,
}

impl Handler<SimulationCommandLst> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: SimulationCommandLst, _ctx: &mut Self::Context) -> Self::Result {
        for cmd in msg.commands {
            match cmd {
                SimulationCommand::LoadSimulation { model_shapes, script } => {
                    let model_registration_out = model_shapes
                        .into_iter()
                        .map(|(model, shape)| self.register_model(model, shape))
                        .collect::<Result<Vec<_>, _>>();

                    if let Err(err) = model_registration_out {
                        log::error!("Error registering simulation clients - {err}")
                    }

                    let load_script_out = self.user_registry.load_script(&script);
                    if let Err(err) = load_script_out {
                        log::error!("Error loading script - {err}");
                    }

                    self.sim_users.drain();
                    let model_count = self.user_registry.count_user_models();
                    self.user_registry
                        .model_names()
                        .into_iter()
                        .enumerate()
                        .for_each(|(idx, model)| {
                            self.sim_users.insert(model.to_string(), SimulationUserModel::new(
                                idx as u32,
                                self.user_registry.build_factory(model)
                                    .unwrap_or_else(|| panic!("No factory for {model}")),
                            ));
                        });
                }
                SimulationCommand::LaunchSimulation { start_ts } => {
                    self.start_ts = Some(start_ts);
                }
                SimulationCommand::UpdateAgentsCount { count } => {
                    self.agents_count = count;
                }
                SimulationCommand::StopSimulation { reset } => {
                    self.start_ts = None;
                    if reset {
                        self.user_registry.reset_script();
                        self.model_shapes.clear();
                    }
                }
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
    pub model: String,
    pub count_by_state: HashMap<UserState, usize>,
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

    fn handle(&mut self, _msg: FetchSimulationStats, _ctx: &mut Self::Context) -> Self::Result {
        let state = match (self.start_ts.as_ref(), self.user_registry.has_registered_models()) {
            (_, false) => SimulationState::Idle,
            (None, true) => SimulationState::Ready,
            (Some(ts), true) if *ts < SystemTime::now() => SimulationState::Running,
            (Some(_), true) => SimulationState::Waiting,
        };

        let stats = self.sim_users.iter()
            .map(|(model, usr)| ClientStats {
                model: model.clone(),
                count_by_state: usr.count_by_state(),
            })
            .collect();

        SimulationStats {
            stats,
            timestamp: SystemTime::now(),
            state,
            simulation_id: "".to_string(),
        }
    }
}

